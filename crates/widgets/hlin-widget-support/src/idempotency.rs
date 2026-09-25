//! Making the same write twice, once.
//!
//! The shell never retries a write, but a person may: the module sends the
//! same `Idempotency-Key` when they press the button again after a timeout
//! ([[HLIN-S-0007]], *The request proxy*). By then the first attempt may well
//! have landed. So a widget remembers which keys it has acted on and what it
//! answered, and answers a repeat with the same thing instead of acting twice:
//! a counter bumped once, not twice.
//!
//! The feed's (`hlin-sample-feed`) memory, moved here unchanged so no widget
//! has to decide this again. [`crate::Platform::write`] is the only caller.
//!
//! Three choices worth copying, or at least deciding on deliberately:
//!
//! - **Keys are per person.** The same key from two people is two writes. A
//!   key is chosen by the client, and one person must never be able to read
//!   another's answer by guessing theirs.
//! - **A key is bound to its request.** The same key on a different method,
//!   path or body is a client bug, answered 422, rather than silently replaying
//!   an answer to a question that was not asked.
//! - **Only writes that happened are remembered.** A refusal or a malformed
//!   request changed nothing, so a retry is simply decided again. That way a
//!   person who fixes what was wrong is not held to an answer about a write
//!   that never occurred.

use std::collections::{HashMap, VecDeque};

use serde_json::Value;

/// How many keys are remembered before the oldest is forgotten.
///
/// Bounded so the memory a client can make this platform hold is bounded. A
/// retry comes seconds after its original, not thousands of writes later, so
/// a short memory is enough.
pub const REMEMBERED: usize = 1024;

/// The longest key accepted, in bytes.
pub const MAX_KEY_BYTES: usize = 255;

/// Which request a key was first used for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl Fingerprint {
    /// The request, as it arrived.
    pub fn new(method: &str, path: &str, body: &[u8]) -> Self {
        Self {
            method: method.to_string(),
            path: path.to_string(),
            body: body.to_vec(),
        }
    }
}

/// What this platform answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The status code.
    pub status: u16,
    /// The body, if the answer had one.
    pub body: Option<Value>,
}

/// What a key says about a request.
#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    /// Never used by this person: act on it.
    New,
    /// Used for this same request: here is what was answered.
    Again(Answer),
    /// Used for a different request.
    Conflict,
}

/// The keys this platform has acted on, per person.
#[derive(Debug, Default)]
pub struct Remembered {
    answers: HashMap<(String, String), (Fingerprint, Answer)>,
    order: VecDeque<(String, String)>,
}

impl Remembered {
    /// What `key` from `who` means for this request.
    pub fn check(&self, who: &str, key: &str, request: &Fingerprint) -> Seen {
        match self.answers.get(&(who.to_string(), key.to_string())) {
            None => Seen::New,
            Some((first, answer)) if first == request => Seen::Again(answer.clone()),
            Some(_) => Seen::Conflict,
        }
    }

    /// Remember what a write that happened was answered.
    pub fn remember(&mut self, who: &str, key: &str, request: Fingerprint, answer: Answer) {
        let id = (who.to_string(), key.to_string());
        if self.answers.insert(id.clone(), (request, answer)).is_none() {
            self.order.push_back(id);
        }
        while self.order.len() > REMEMBERED {
            if let Some(oldest) = self.order.pop_front() {
                self.answers.remove(&oldest);
            }
        }
    }
}

/// Whether a key is one this platform will remember.
///
/// Visible ASCII and not too long, which every key the SDK mints is. Anything
/// else is refused rather than stored, since it is a key a client made up and
/// there is no reason to hold on to it.
pub fn acceptable(key: &str) -> bool {
    !key.is_empty() && key.len() <= MAX_KEY_BYTES && key.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created() -> Answer {
        Answer {
            status: 201,
            body: Some(serde_json::json!({ "id": "p4" })),
        }
    }

    #[test]
    fn the_same_key_for_the_same_request_is_answered_again() {
        let mut remembered = Remembered::default();
        let request = Fingerprint::new("POST", "/api/posts", b"{}");
        remembered.remember("alice", "k1", request.clone(), created());

        assert_eq!(
            remembered.check("alice", "k1", &request),
            Seen::Again(created())
        );
    }

    #[test]
    fn the_same_key_from_somebody_else_is_a_new_write() {
        let mut remembered = Remembered::default();
        let request = Fingerprint::new("POST", "/api/posts", b"{}");
        remembered.remember("alice", "k1", request.clone(), created());

        assert_eq!(remembered.check("bob", "k1", &request), Seen::New);
    }

    #[test]
    fn the_same_key_for_a_different_request_is_a_conflict() {
        let mut remembered = Remembered::default();
        remembered.remember(
            "alice",
            "k1",
            Fingerprint::new("POST", "/api/posts", b"{}"),
            created(),
        );

        for other in [
            Fingerprint::new("POST", "/api/posts", b"{\"body\":\"x\"}"),
            Fingerprint::new("PUT", "/api/posts", b"{}"),
            Fingerprint::new("POST", "/api/posts/p1", b"{}"),
        ] {
            assert_eq!(remembered.check("alice", "k1", &other), Seen::Conflict);
        }
    }

    #[test]
    fn the_oldest_keys_are_forgotten_first() {
        let mut remembered = Remembered::default();
        let request = Fingerprint::new("POST", "/api/posts", b"{}");
        for n in 0..=REMEMBERED {
            remembered.remember("alice", &format!("k{n}"), request.clone(), created());
        }

        assert_eq!(remembered.check("alice", "k0", &request), Seen::New);
        assert_eq!(
            remembered.check("alice", &format!("k{REMEMBERED}"), &request),
            Seen::Again(created())
        );
    }

    #[test]
    fn keys_the_sdk_would_never_send_are_not_accepted() {
        assert!(acceptable("01J8Z3Q4R5S6T7V8W9X0Y1Z2A3"));
        assert!(!acceptable(""));
        assert!(!acceptable("has space"));
        assert!(!acceptable(&"k".repeat(MAX_KEY_BYTES + 1)));
    }
}
