//! Words for a person, from what the bridge came back with.

use hlin_module::hlin_bridge::Refusal;
use hlin_module::{BridgeError, Refused, Reply};
use serde::Deserialize;

/// A successful answer, or what to tell the person.
pub fn outcome(reply: Result<Reply, BridgeError>) -> Result<hlin_module::Answer, String> {
    match reply {
        Ok(Reply::Answered(answer)) if answer.is_success() => Ok(answer),
        Ok(Reply::Answered(answer)) => Err(platform_words(&answer)),
        Ok(Reply::Refused(refused)) => Err(shell_words(&refused)),
        Err(_) => Err("The panel closed before the widget answered.".to_string()),
    }
}

/// The platform's own words for a refusal, shown exactly as it wrote them.
///
/// Every widget answers a refusal it decides with `{"message": ...}`, written
/// for the person who clicked. Anything else gets a plain sentence with the
/// status in it.
pub fn platform_words(answer: &hlin_module::Answer) -> String {
    #[derive(Deserialize)]
    struct Message {
        message: String,
    }
    match answer.json::<Message>() {
        Ok(said) if !said.message.trim().is_empty() => said.message,
        _ => format!("The widget said no (status {}).", answer.status),
    }
}

/// What to say when the shell, not the platform, refused. The shell's own
/// `reason` is for a developer, so it is not shown.
pub fn shell_words(refused: &Refused) -> String {
    match refused.refusal {
        Refusal::ReadOnly => "This surface is read-only, so nothing is sent.".to_string(),
        Refusal::NotSignedIn => "You have been signed out. Sign in again to carry on.".to_string(),
        Refusal::Unreachable | Refusal::Timeout => {
            "The widget could not be reached. Try again in a moment.".to_string()
        }
        Refusal::TooMany => "Too much at once. Try again in a moment.".to_string(),
        Refusal::TooLarge => "That is too much to send.".to_string(),
        _ => "Hlin did not send this to the widget.".to_string(),
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

    fn answer(status: u16, body: &str) -> hlin_module::Answer {
        hlin_module::Answer {
            status,
            headers: BTreeMap::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_refusal_is_shown_in_the_widgets_own_words() {
        assert_eq!(
            platform_words(&answer(
                409,
                r#"{"message":"The counter is already at zero"}"#
            )),
            "The counter is already at zero"
        );
    }

    #[test]
    fn an_answer_without_words_still_says_what_happened() {
        assert_eq!(
            platform_words(&answer(500, "<html>")),
            "The widget said no (status 500)."
        );
    }

    #[test]
    fn the_shells_reason_is_never_shown_as_if_the_widget_said_it() {
        let refused = Refused {
            refusal: Refusal::OutsidePrefix,
            status: 403,
            reason: "internal detail".to_string(),
        };
        assert!(!shell_words(&refused).contains("internal detail"));
        assert!(!worth_retrying(&Ok(Reply::Refused(refused))));
    }

    #[test]
    fn only_the_networks_failures_are_worth_retrying() {
        let unreachable = Refused {
            refusal: Refusal::Unreachable,
            status: 502,
            reason: String::new(),
        };
        assert!(worth_retrying(&Ok(Reply::Refused(unreachable))));
        assert!(worth_retrying(&Ok(Reply::Answered(answer(503, "")))));
        assert!(!worth_retrying(&Ok(Reply::Answered(answer(409, "")))));
    }
}
