//! The feed's API, as its module sees it: what it asks for and how it reads
//! the answers.
//!
//! Nothing here touches the browser, so all of it is tested natively. The
//! paths are the platform's own (`/api/posts`), relative to its base: the shell
//! decides which platform a request goes to from the frame it came from, and
//! forwards it with the viewer's identity bound to it.

use chrono::{DateTime, Utc};
use hlin_module::hlin_bridge::Refusal;
use hlin_module::{Answer, BridgeError, Module, Refused, Reply, Request};
use serde::Deserialize;

/// The panel this module draws, as the manifest names it.
pub const PANEL: &str = "posts";

/// Who wrote a post.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Author {
    /// Stable, as the feed knows them.
    pub id: String,
    /// What to call them.
    pub name: String,
}

/// One post.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Post {
    /// Stable, and never reused.
    pub id: String,
    /// Who wrote it.
    pub author: Author,
    /// What it says.
    pub body: String,
    /// When it was posted, as the feed wrote it (RFC 3339).
    pub posted_at: String,
    /// When it was last edited, if it was.
    #[serde(default)]
    pub edited_at: Option<String>,
}

#[derive(Deserialize)]
struct Posts {
    posts: Vec<Post>,
}

// -- Requests ----------------------------------------------------------------

/// Every post.
pub fn posts() -> Request {
    Request::get("/api/posts").header("accept", "application/json")
}

/// Write a post.
pub fn create(body: &str) -> Request {
    with_body(Request::post("/api/posts"), body)
}

/// Change what a post says.
pub fn edit(post: &str, body: &str) -> Request {
    with_body(Request::put(format!("/api/posts/{}", segment(post))), body)
}

/// Take a post down.
pub fn delete(post: &str) -> Request {
    Request::delete(format!("/api/posts/{}", segment(post)))
}

fn with_body(request: Request, body: &str) -> Request {
    request
        .json(&serde_json::json!({ "body": body }))
        .expect("a string serialises")
}

/// An id as one path segment: anything but a letter, digit, `-` or `_` is
/// percent-encoded, so an id can never add a segment of its own.
fn segment(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

// -- Answers -----------------------------------------------------------------

/// Reads the platform's answer to [`posts`], newest first.
///
/// The feed already sends them newest first. Sorted here anyway, because the
/// order is what this module promises to show, and the platform's order is a
/// detail of its API.
pub fn read_posts(answer: &Answer) -> Result<Vec<Post>, String> {
    let mut posts = answer
        .json::<Posts>()
        .map(|found| found.posts)
        .map_err(|_| "The feed answered with something this module cannot read.".to_string())?;
    posts.sort_by_key(|post| std::cmp::Reverse(millis(&post.posted_at).unwrap_or(0)));
    Ok(posts)
}

/// Sends a request once, and returns the platform's answer if it succeeded or
/// what to tell the person if it did not.
pub async fn send(module: &Module, request: Request) -> Result<Answer, String> {
    outcome(module.fetch(request).await)
}

/// A successful answer, or what to tell the person.
pub fn outcome(reply: Result<Reply, BridgeError>) -> Result<Answer, String> {
    match reply {
        Ok(Reply::Answered(answer)) if answer.is_success() => Ok(answer),
        Ok(Reply::Answered(answer)) => Err(platform_words(&answer)),
        Ok(Reply::Refused(refused)) => Err(shell_words(&refused)),
        Err(_) => Err("The panel closed before the feed answered.".to_string()),
    }
}

/// The feed's own words for a refusal, shown exactly as it wrote them.
///
/// The feed answers every refusal it decides with `{"message": ...}`, written
/// for the person who clicked ("Only the author can edit this post").
/// Anything else gets a plain sentence with the status in it.
pub fn platform_words(answer: &Answer) -> String {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match answer.json::<Message>() {
        Ok(said) if !said.message.trim().is_empty() => said.message,
        _ => format!("The feed said no (status {}).", answer.status),
    }
}

/// What to say when the shell, not the feed, refused. The shell's own
/// `reason` is for a developer, so it is not shown.
pub fn shell_words(refused: &Refused) -> String {
    match refused.refusal {
        Refusal::ReadOnly => "This surface is read-only, so posts are not sent.".to_string(),
        Refusal::NotSignedIn => "You have been signed out. Sign in again to carry on.".to_string(),
        Refusal::Unreachable | Refusal::Timeout => {
            "The feed could not be reached. Try again in a moment.".to_string()
        }
        Refusal::TooMany => "Too much at once. Try again in a moment.".to_string(),
        Refusal::TooLarge => "That is too long to send.".to_string(),
        _ => "Hlin did not send this to the feed.".to_string(),
    }
}

/// Whether trying the same write again might work: the network, not a rule.
pub fn worth_retrying(reply: &Result<Reply, BridgeError>) -> bool {
    match reply {
        Ok(Reply::Refused(refused)) => matches!(
            refused.refusal,
            Refusal::Unreachable | Refusal::Timeout | Refusal::TooMany
        ),
        Ok(Reply::Answered(answer)) => answer.status >= 500,
        Err(_) => false,
    }
}

// -- Time --------------------------------------------------------------------

/// A timestamp from the feed, in milliseconds since the epoch.
pub fn millis(stamp: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|at| at.with_timezone(&Utc).timestamp_millis())
}

/// How long ago, for a person: "just now", "5 min ago", "3 h ago", or the
/// date. `now` is passed in, so this is the same function in a test and in
/// the browser.
pub fn ago(stamp: &str, now_millis: i64) -> String {
    let Some(then) = millis(stamp) else {
        return stamp.to_string();
    };
    let minutes = (now_millis - then).max(0) / 60_000;
    match minutes {
        0 => "just now".to_string(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => DateTime::parse_from_rfc3339(stamp)
            .map(|at| at.format("%-d %b %Y").to_string())
            .unwrap_or_else(|_| stamp.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn answer(status: u16, body: &str) -> Answer {
        Answer {
            status,
            headers: BTreeMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn posts_are_shown_newest_first_whatever_order_they_came_in() {
        let posts = read_posts(&answer(
            200,
            r#"{"posts":[
                {"id":"p1","author":{"id":"a","name":"Dana"},"body":"old","posted_at":"2026-09-24T09:00:00Z","edited_at":null},
                {"id":"p2","author":{"id":"b","name":"Eli"},"body":"new","posted_at":"2026-09-24T10:00:00.5Z","edited_at":null}
            ]}"#,
        ))
        .unwrap();
        assert_eq!(posts[0].id, "p2");
        assert_eq!(posts[1].author.name, "Dana");
    }

    #[test]
    fn a_refusal_is_shown_in_the_feeds_own_words() {
        assert_eq!(
            platform_words(&answer(
                403,
                r#"{"message":"Only the author can edit this post"}"#
            )),
            "Only the author can edit this post"
        );
    }

    #[test]
    fn an_answer_without_words_still_says_what_happened() {
        assert_eq!(
            platform_words(&answer(500, "<html>")),
            "The feed said no (status 500)."
        );
    }

    #[test]
    fn the_shells_reason_is_never_shown_as_if_the_feed_said_it() {
        let refused = Refused {
            refusal: Refusal::OutsidePrefix,
            status: 403,
            reason: "internal detail".to_string(),
        };
        assert!(!shell_words(&refused).contains("internal detail"));
        assert!(!worth_retrying(&Ok(Reply::Refused(refused))));
    }

    #[test]
    fn times_read_as_a_person_would_say_them() {
        let posted = "2026-09-24T10:00:00Z";
        let at = millis(posted).unwrap();
        assert_eq!(ago(posted, at + 20_000), "just now");
        assert_eq!(ago(posted, at + 5 * 60_000), "5 min ago");
        assert_eq!(ago(posted, at + 3 * 3_600_000), "3 h ago");
        assert_eq!(ago(posted, at + 3 * 86_400_000), "24 Sep 2026");
        assert_eq!(ago("not a time", at), "not a time");
    }

    #[test]
    fn an_id_can_never_add_a_path_segment() {
        assert_eq!(delete("../x").path(), "/api/posts/%2E%2E%2Fx");
        assert_eq!(edit("p12", "hi").path(), "/api/posts/p12");
    }
}
