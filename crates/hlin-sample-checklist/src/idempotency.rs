//! Remembering the answer to a write, so a retried write is not applied twice.
//!
//! The shell sends every write with an `Idempotency-Key` and never retries on
//! its own; the SDK retries only when a person asks, with the same key
//! ([[HLIN-S-0007]], *The request proxy*). That makes the key the platform's
//! only way to tell "do it again" from "did that work?", and for a toggle the
//! difference is the whole outcome: applied twice, a crossed-off item is back
//! on the list.
//!
//! What is remembered is the first answer, refusals included. A retry of a
//! write that was refused is refused again in the same words, which is what
//! the person saw the first time and what they should see now.
//!
//! Bounded, oldest forgotten first. A key is only useful for as long as
//! somebody might retry with it, which is seconds, and a platform that kept
//! every key it had ever seen would be a memory leak with a rationale.

use std::collections::{HashMap, VecDeque};

use serde_json::Value;

/// How many recent keys are remembered.
pub const REMEMBERED: usize = 1024;

/// An answer to a write, kept so it can be given again.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The status the first request was answered with.
    pub status: u16,
    /// Its JSON body, if it had one.
    pub body: Option<Value>,
}

/// What the memory says about a key.
#[derive(Debug, Clone, PartialEq)]
pub enum Recall {
    /// Not seen recently: do the write.
    Fresh,
    /// Seen for this same request: answer this, and change nothing.
    Replay(Answer),
    /// Seen for a *different* request. Answering either request's outcome
    /// would be a lie about the other, so the caller is told the key is spent.
    Reused,
}

/// Recent keys and their answers.
///
/// Keys are scoped to the caller. Two people who happen to send the same key
/// are two different writes, and one must never be answered with the other's
/// result: that would tell Bob what Alice's write returned.
#[derive(Debug)]
pub struct Idempotency {
    capacity: usize,
    order: VecDeque<(String, String)>,
    answers: HashMap<(String, String), (String, Answer)>,
}

impl Idempotency {
    /// A memory of this many keys.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            order: VecDeque::new(),
            answers: HashMap::new(),
        }
    }

    /// What is known about this caller's key, given what the request is.
    ///
    /// `request` identifies the write — method, path and body — so a key
    /// reused for something else is caught rather than replayed.
    pub fn recall(&self, caller: &str, key: &str, request: &str) -> Recall {
        match self.answers.get(&(caller.to_string(), key.to_string())) {
            None => Recall::Fresh,
            Some((seen, answer)) if seen == request => Recall::Replay(answer.clone()),
            Some(_) => Recall::Reused,
        }
    }

    /// Keep the answer to a write.
    pub fn remember(&mut self, caller: &str, key: &str, request: &str, answer: Answer) {
        let slot = (caller.to_string(), key.to_string());
        if self
            .answers
            .insert(slot.clone(), (request.to_string(), answer))
            .is_none()
        {
            self.order.push_back(slot);
        }
        while self.order.len() > self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.answers.remove(&oldest);
            }
        }
    }

    /// How many keys are held, for the tests.
    pub fn len(&self) -> usize {
        self.answers.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.answers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> Answer {
        Answer {
            status: 200,
            body: None,
        }
    }

    #[test]
    fn a_key_seen_for_the_same_request_is_replayed() {
        let mut memory = Idempotency::new(4);
        memory.remember("alice", "k", "POST /a", ok());
        assert_eq!(memory.recall("alice", "k", "POST /a"), Recall::Replay(ok()));
    }

    #[test]
    fn a_key_seen_for_another_request_is_spent() {
        let mut memory = Idempotency::new(4);
        memory.remember("alice", "k", "POST /a", ok());
        assert_eq!(memory.recall("alice", "k", "POST /b"), Recall::Reused);
    }

    #[test]
    fn the_same_key_from_two_people_is_two_writes() {
        let mut memory = Idempotency::new(4);
        memory.remember("alice", "k", "POST /a", ok());
        assert_eq!(memory.recall("bob", "k", "POST /a"), Recall::Fresh);
    }

    #[test]
    fn the_oldest_key_is_forgotten_first() {
        let mut memory = Idempotency::new(2);
        memory.remember("a", "1", "r", ok());
        memory.remember("a", "2", "r", ok());
        memory.remember("a", "3", "r", ok());
        assert_eq!(memory.len(), 2);
        assert_eq!(memory.recall("a", "1", "r"), Recall::Fresh);
        assert_eq!(memory.recall("a", "3", "r"), Recall::Replay(ok()));
    }
}
