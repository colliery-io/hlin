//! The checklist's API, as its module sees it: what it asks for and how it
//! reads the answers.
//!
//! Nothing here touches the browser, so all of it is tested natively. The
//! paths are the platform's own (`/api/lists/...`), relative to its base: the
//! shell decides which platform a request goes to from the frame it came from,
//! and forwards it with the viewer's identity bound to it.

use hlin_module::hlin_bridge::Refusal;
use hlin_module::{Answer, BridgeError, Module, Refused, Reply, Request};
use serde::Deserialize;

/// The panel this module draws, as the manifest names it.
pub const PANEL: &str = "items";

/// The panel's parameter holding the chosen list.
pub const LIST_PARAM: &str = "list";

/// A list the viewer belongs to, for the picker.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListSummary {
    /// How the list is addressed.
    pub id: String,
    /// What a person sees.
    pub name: String,
}

#[derive(Deserialize)]
struct Lists {
    lists: Vec<ListSummary>,
}

/// One line on a list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
pub struct Item {
    /// Unique across the platform.
    pub id: String,
    /// What needs doing.
    pub text: String,
    /// Crossed off.
    pub done: bool,
    /// Who added it, for display.
    pub author: String,
    /// Who added it, as the platform knows them.
    pub author_id: String,
}

/// Who the viewer is to this list, as the platform told its own module.
///
/// A hint and nothing more. The platform decides every write again, and says
/// no in its own words if the hint was wrong or out of date.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Viewer {
    /// The viewer's id, compared with an item's `author_id`.
    pub id: String,
    /// Whether the viewer owns the list, and so may change any item on it.
    pub owner: bool,
}

/// One list and everything on it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListPage {
    /// Which list.
    pub list: ListSummary,
    /// Who is looking.
    pub viewer: Viewer,
    /// Oldest first.
    pub items: Vec<Item>,
}

impl ListPage {
    /// Whether to offer editing and deleting this item: the platform allows
    /// it to the item's author and to the list's owner. Crossing off and
    /// adding are for every member, so they are always offered.
    pub fn may_change(&self, item: &Item) -> bool {
        self.viewer.owner || item.author_id == self.viewer.id
    }
}

// -- Requests ----------------------------------------------------------------

/// The lists the viewer belongs to.
pub fn lists() -> Request {
    Request::get("/api/lists").header("accept", "application/json")
}

/// One list and its items.
pub fn list(list: &str) -> Request {
    Request::get(format!("/api/lists/{}/items", segment(list))).header("accept", "application/json")
}

/// Add an item.
pub fn add(list: &str, text: &str) -> Request {
    with_text(
        Request::post(format!("/api/lists/{}/items", segment(list))),
        text,
    )
}

/// Cross an item off, or back on.
pub fn toggle(list: &str, item: &str) -> Request {
    Request::post(format!(
        "/api/lists/{}/items/{}/toggle",
        segment(list),
        segment(item)
    ))
}

/// Change what an item says.
pub fn edit(list: &str, item: &str, text: &str) -> Request {
    with_text(
        Request::patch(format!(
            "/api/lists/{}/items/{}",
            segment(list),
            segment(item)
        )),
        text,
    )
}

/// Take an item off the list.
pub fn delete(list: &str, item: &str) -> Request {
    Request::delete(format!(
        "/api/lists/{}/items/{}",
        segment(list),
        segment(item)
    ))
}

fn with_text(request: Request, text: &str) -> Request {
    request
        .json(&serde_json::json!({ "text": text }))
        .expect("a string serialises")
}

/// An id as one path segment.
///
/// Ids come from the platform and are plain, but a module should not trust
/// that to build a path: anything but a letter, digit, `-` or `_` is
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

/// Reads the platform's answer to [`lists`].
pub fn read_lists(answer: &Answer) -> Result<Vec<ListSummary>, String> {
    answer
        .json::<Lists>()
        .map(|found| found.lists)
        .map_err(|_| UNREADABLE.to_string())
}

/// Reads the platform's answer to [`list`].
pub fn read_list(answer: &Answer) -> Result<ListPage, String> {
    answer
        .json::<ListPage>()
        .map_err(|_| UNREADABLE.to_string())
}

const UNREADABLE: &str = "The checklist answered with something this module cannot read.";

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
        Err(_) => Err("The panel closed before the checklist answered.".to_string()),
    }
}

/// The platform's own words for a refusal, shown exactly as it wrote them.
///
/// The checklist answers every refusal with `{"message": ...}`, written for the
/// person who clicked. Anything else (a 401 from the identity layer, a proxy's
/// error page) gets a plain sentence with the status in it.
pub fn platform_words(answer: &Answer) -> String {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match answer.json::<Message>() {
        Ok(said) if !said.message.trim().is_empty() => said.message,
        _ => format!("The checklist said no (status {}).", answer.status),
    }
}

/// What to say when the shell, not the checklist, refused.
///
/// The shell's own `reason` is for a developer, so it is not shown. These are
/// the refusals a person can do something about.
pub fn shell_words(refused: &Refused) -> String {
    match refused.refusal {
        Refusal::ReadOnly => "This surface is read-only, so changes are not sent.".to_string(),
        Refusal::NotSignedIn => "You have been signed out. Sign in again to carry on.".to_string(),
        Refusal::Unreachable | Refusal::Timeout => {
            "The checklist could not be reached. Try again in a moment.".to_string()
        }
        Refusal::TooMany => "Too much at once. Try again in a moment.".to_string(),
        _ => "Hlin did not send this to the checklist.".to_string(),
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

    fn refused(refusal: Refusal) -> Refused {
        Refused {
            refusal,
            status: 403,
            reason: "internal detail".to_string(),
        }
    }

    #[test]
    fn a_list_reads_as_the_platform_sends_it() {
        let page = read_list(&answer(
            200,
            r#"{"list":{"id":"team","name":"Team","owner":"alice@example.com","members":["alice@example.com"]},
                "viewer":{"id":"bob","owner":false},
                "items":[{"id":"i1","text":"Milk","done":false,"author":"Alice","author_id":"alice"}]}"#,
        ))
        .unwrap();
        assert_eq!(page.list.name, "Team");
        assert_eq!(page.items[0].text, "Milk");
        assert!(!page.viewer.owner);
    }

    #[test]
    fn the_author_and_the_owner_may_change_an_item_and_nobody_else() {
        let item = Item {
            id: "i1".into(),
            text: "Milk".into(),
            done: false,
            author: "Alice".into(),
            author_id: "alice".into(),
        };
        let page = |id: &str, owner| ListPage {
            list: ListSummary {
                id: "team".into(),
                name: "Team".into(),
            },
            viewer: Viewer {
                id: id.into(),
                owner,
            },
            items: vec![item.clone()],
        };
        assert!(page("alice", false).may_change(&item));
        assert!(page("carol", true).may_change(&item));
        assert!(!page("bob", false).may_change(&item));
    }

    #[test]
    fn a_refusal_is_shown_in_the_platforms_own_words() {
        let words = platform_words(&answer(
            403,
            r#"{"message":"Only members of Team can see or change it, and you are not one."}"#,
        ));
        assert_eq!(
            words,
            "Only members of Team can see or change it, and you are not one."
        );
    }

    #[test]
    fn an_answer_without_words_still_says_what_happened() {
        assert_eq!(
            platform_words(&answer(401, "")),
            "The checklist said no (status 401)."
        );
    }

    #[test]
    fn the_shells_reason_is_never_shown_as_if_the_platform_said_it() {
        let words = shell_words(&refused(Refusal::OutsidePrefix));
        assert!(!words.contains("internal detail"));
    }

    #[test]
    fn only_a_failure_of_the_network_is_worth_retrying() {
        assert!(worth_retrying(&Ok(Reply::Refused(refused(
            Refusal::Timeout
        )))));
        assert!(worth_retrying(&Ok(Reply::Answered(answer(502, "")))));
        assert!(!worth_retrying(&Ok(Reply::Answered(answer(403, "")))));
        assert!(!worth_retrying(&Ok(Reply::Refused(refused(
            Refusal::ReadOnly
        )))));
    }

    #[test]
    fn an_id_can_never_add_a_path_segment() {
        assert_eq!(
            delete("team", "../x").path(),
            "/api/lists/team/items/%2E%2E%2Fx"
        );
        assert_eq!(
            toggle("team", "i-1_a").path(),
            "/api/lists/team/items/i-1_a/toggle"
        );
    }

    #[test]
    fn writes_carry_their_text_as_json() {
        let request = add("team", "Milk");
        assert_eq!(request.method(), hlin_module::hlin_bridge::Method::Post);
        assert_eq!(request.path(), "/api/lists/team/items");
    }
}
