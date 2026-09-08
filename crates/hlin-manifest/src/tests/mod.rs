//! Unit tests for the crate's internals.
//!
//! The worked example from specification HLIN-S-0001 is read from the same
//! fixture the integration tests use, so the specification's document is
//! exercised in one place and cannot drift between suites.

use crate::validate::is_valid_key;

pub const SPEC_EXAMPLE: &str = include_str!("../../tests/fixtures/spec-example.json");

#[test]
fn the_specifications_example_is_readable_and_valid() {
    let manifest = crate::parse_str(SPEC_EXAMPLE).expect("the specification's example parses");
    let checked = crate::validate(&manifest, "orebank");
    assert!(checked.is_document_valid(), "{:?}", checked.document);
    assert!(
        checked.rejected().is_empty(),
        "no panel should be rejected: {:?}",
        checked.rejected()
    );
}

#[test]
fn identifiers_follow_the_specifications_pattern() {
    for accepted in ["ab", "orebank", "queue-depth-by-stage", "a1", "9x"] {
        assert!(is_valid_key(accepted), "`{accepted}` should be accepted");
    }
    for rejected in [
        "",
        "a",
        "-leading",
        "trailing-",
        "Upper",
        "under_score",
        "has space",
        "dot.ted",
    ] {
        assert!(!is_valid_key(rejected), "`{rejected}` should be rejected");
    }
    let too_long = "a".repeat(65);
    assert!(!is_valid_key(&too_long));
    let longest = "a".repeat(64);
    assert!(is_valid_key(&longest));
}
