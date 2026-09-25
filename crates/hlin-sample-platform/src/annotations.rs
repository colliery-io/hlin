//! Notes pinned to a moment, which this platform's own module reads and writes
//! (HLIN-T-0071).
//!
//! The one thing on this platform a viewer changes. Everything else here is a
//! function of the clock, which is right for panels and useless for proving
//! that a module's write arrives as the person who made it: so a module drawn
//! with the SDK reads the notes inside the surface's time range, and adds one
//! at the present moment, both through the shell and both as the viewer.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/module/annotations?from=&to=` | The notes in that window (milliseconds), newest first, and who is asking |
//! | `POST` | `/api/module/annotations/pin` | `{ "text": "…" }`, pinned to now, by whoever sent it |
//!
//! A write needs an `Idempotency-Key`, and the same key again is answered with
//! the first answer rather than a second note, so the SDK's retry of a write a
//! person asked to try again never pins it twice. Kept in memory, like the rest
//! of this platform, and bounded, like everything it remembers.

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// Where the module reads and writes notes, beneath the platform's base.
pub const PATH: &str = "/api/module/annotations";

/// The prefix a module may write under, as the manifest declares it: a prefix
/// ends with `/`, so a write is a path beneath it rather than [`PATH`] itself.
pub const WRITES: &str = "/api/module/annotations/";

/// Where a module pins a note.
pub const PIN: &str = "/api/module/annotations/pin";

/// How many notes are kept; the oldest go first.
pub const KEPT: usize = 200;

/// The longest note, in characters.
pub const LONGEST: usize = 280;

/// How many idempotency keys are remembered, oldest forgotten first.
const KEYS_KEPT: usize = 400;

/// One note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Annotation {
    /// Unique for the life of the platform.
    pub id: u64,
    /// When it was pinned, in milliseconds since the Unix epoch.
    pub at_millis: i64,
    /// What it says.
    pub text: String,
    /// Who pinned it, as they are called.
    pub author: String,
}

/// What a write asks for.
#[derive(Debug, Deserialize)]
pub struct Pin {
    /// What the note says.
    pub text: String,
}

/// Why a note was not pinned, in words for the person who tried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Nothing to say.
    Empty,
    /// More than [`LONGEST`] characters.
    TooLong,
}

impl Refused {
    /// The platform's own words.
    pub fn words(&self) -> String {
        match self {
            Self::Empty => "A note needs some text.".to_string(),
            Self::TooLong => format!("A note is at most {LONGEST} characters."),
        }
    }
}

/// Every note, and the writes already answered.
#[derive(Debug, Default)]
pub struct Annotations {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    next: u64,
    notes: VecDeque<Annotation>,
    answered: VecDeque<(String, Annotation)>,
}

impl Annotations {
    /// None yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The notes pinned inside `[from, to)`, newest first. An open end is
    /// unbounded.
    pub fn within(&self, from: Option<i64>, to: Option<i64>) -> Vec<Annotation> {
        let inner = self.inner.lock().expect("the notes are not poisoned");
        inner
            .notes
            .iter()
            .rev()
            .filter(|note| from.is_none_or(|from| note.at_millis >= from))
            .filter(|note| to.is_none_or(|to| note.at_millis < to))
            .cloned()
            .collect()
    }

    /// Pin a note, or answer the one already pinned under this key. `true`
    /// with a note that is new.
    pub fn pin(
        &self,
        key: &str,
        author: &str,
        text: &str,
        at_millis: i64,
    ) -> Result<(Annotation, bool), Refused> {
        let mut inner = self.inner.lock().expect("the notes are not poisoned");
        if let Some((_, note)) = inner.answered.iter().find(|(seen, _)| seen == key) {
            return Ok((note.clone(), false));
        }

        let text = text.trim();
        if text.is_empty() {
            return Err(Refused::Empty);
        }
        if text.chars().count() > LONGEST {
            return Err(Refused::TooLong);
        }

        inner.next += 1;
        let note = Annotation {
            id: inner.next,
            at_millis,
            text: text.to_string(),
            author: author.to_string(),
        };
        inner.notes.push_back(note.clone());
        while inner.notes.len() > KEPT {
            inner.notes.pop_front();
        }
        inner.answered.push_back((key.to_string(), note.clone()));
        while inner.answered.len() > KEYS_KEPT {
            inner.answered.pop_front();
        }
        Ok((note, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_pinned_once_whatever_the_retries() {
        let notes = Annotations::new();
        let (first, new) = notes.pin("k-1", "Ada", "  deploy  ", 1_000).unwrap();
        assert!(new);
        assert_eq!(first.text, "deploy");
        assert_eq!(first.author, "Ada");

        let (again, new) = notes.pin("k-1", "Ada", "deploy", 2_000).unwrap();
        assert!(!new, "the same key is the same write");
        assert_eq!(again, first);
        assert_eq!(notes.within(None, None).len(), 1);
    }

    #[test]
    fn the_window_is_half_open_and_newest_first() {
        let notes = Annotations::new();
        for (key, at) in [("a", 100), ("b", 200), ("c", 300)] {
            notes.pin(key, "Ada", key, at).unwrap();
        }
        let texts = |from, to| {
            notes
                .within(from, to)
                .into_iter()
                .map(|note| note.text)
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(Some(100), Some(300)), ["b", "a"]);
        assert_eq!(texts(None, None), ["c", "b", "a"]);
        assert_eq!(texts(Some(301), None), Vec::<String>::new());
    }

    #[test]
    fn empty_and_overlong_notes_are_refused_in_words() {
        let notes = Annotations::new();
        assert_eq!(notes.pin("a", "Ada", "   ", 0), Err(Refused::Empty));
        let long = "x".repeat(LONGEST + 1);
        assert_eq!(notes.pin("b", "Ada", &long, 0), Err(Refused::TooLong));
        assert!(Refused::TooLong.words().contains("280"));
        assert!(notes.within(None, None).is_empty());
    }

    #[test]
    fn what_is_remembered_is_bounded() {
        let notes = Annotations::new();
        for n in 0..(KEPT + 5) {
            notes.pin(&n.to_string(), "Ada", "x", n as i64).unwrap();
        }
        let kept = notes.within(None, None);
        assert_eq!(kept.len(), KEPT);
        assert_eq!(kept.last().unwrap().at_millis, 5, "the oldest went first");
    }
}
