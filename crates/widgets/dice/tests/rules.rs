//! The dice, called the way the shell calls them.

use hlin_widget_dice::{DICE, Dice, KEPT, PANEL, api, notation, widget};
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use hlin_widget_support::{ModuleFiles, Site};
use serde_json::json;

async fn dice(seed: u64) -> Running {
    testing::start(widget(), Dice::seeded(seed), api()).await
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_the_rolls_as_a_table_and_the_table_is_shared() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(panel.pushed, "everybody sees every roll");
}

#[test]
fn a_roll_is_written_the_way_a_table_writes_it() {
    assert_eq!(notation(20, 1), "d20");
    assert_eq!(notation(6, 3), "3d6");
}

#[tokio::test]
async fn a_roll_is_the_platforms_and_everybody_sees_who_made_it() {
    let dice = dice(7).await;
    let (alice, bob) = (person("u-alice", "Alice"), person("u-bob", "Bob"));

    let rolled = dice
        .write(
            &alice,
            "POST",
            "/api/dice/roll",
            Some(json!({ "sides": 6, "count": 3 })),
        )
        .await;
    assert_eq!(rolled.status, 201, "{}", rolled.body);
    let faces: Vec<u64> = rolled.body["faces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|face| face.as_u64().unwrap())
        .collect();
    assert_eq!(faces.len(), 3);
    assert!(faces.iter().all(|face| (1..=6).contains(face)));
    assert_eq!(rolled.body["total"], faces.iter().sum::<u64>());

    let seen = dice.get(&bob, "/api/dice").await;
    assert_eq!(seen.body["rolls"][0], rolled.body);
    assert_eq!(seen.body["rolls"][0]["name"], "Alice");
}

#[tokio::test]
async fn every_face_comes_up_and_none_is_off_the_die() {
    let dice = dice(42).await;
    let alice = person("u-alice", "Alice");
    let mut seen = [0_u32; 21];
    for _ in 0..40 {
        let rolled = dice
            .write(
                &alice,
                "POST",
                "/api/dice/roll",
                Some(json!({ "sides": 4, "count": 6 })),
            )
            .await;
        for face in rolled.body["faces"].as_array().unwrap() {
            seen[face.as_u64().unwrap() as usize] += 1;
        }
    }
    assert_eq!(seen[0], 0);
    assert!(seen[1..=4].iter().all(|count| *count > 0), "{seen:?}");
    assert!(seen[5..].iter().all(|count| *count == 0), "{seen:?}");
}

#[tokio::test]
async fn only_the_dice_a_table_has_and_one_to_six_of_them() {
    let dice = dice(1).await;
    let alice = person("u-alice", "Alice");
    for (body, message) in [
        (
            json!({ "sides": 7 }),
            "There is no such die here: d4, d6, d8, d10, d12 or d20",
        ),
        (
            json!({ "sides": 6, "count": 0 }),
            "Roll one to 6 dice at once",
        ),
        (
            json!({ "sides": 6, "count": 7 }),
            "Roll one to 6 dice at once",
        ),
    ] {
        let refused = dice
            .write(&alice, "POST", "/api/dice/roll", Some(body.clone()))
            .await;
        assert_eq!(refused.status, 400, "{body}");
        assert_eq!(refused.body["message"], message);
    }
    for sides in DICE {
        let rolled = dice
            .write(
                &alice,
                "POST",
                "/api/dice/roll",
                Some(json!({ "sides": sides })),
            )
            .await;
        assert_eq!(rolled.status, 201, "d{sides}, one die when not said");
        assert_eq!(rolled.body["faces"].as_array().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn the_table_remembers_the_last_ten_newest_first() {
    let dice = dice(3).await;
    let alice = person("u-alice", "Alice");
    for _ in 0..KEPT + 2 {
        dice.write(
            &alice,
            "POST",
            "/api/dice/roll",
            Some(json!({ "sides": 20 })),
        )
        .await;
    }
    let rolls = dice.get(&alice, "/api/dice").await.body["rolls"].clone();
    let numbers: Vec<u64> = rolls
        .as_array()
        .unwrap()
        .iter()
        .map(|roll| roll["number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, (3..=12).rev().collect::<Vec<u64>>());
}

#[tokio::test]
async fn a_retried_roll_is_the_same_roll() {
    let dice = dice(9).await;
    let alice = person("u-alice", "Alice");
    let body = Some(json!({ "sides": 20, "count": 2 }));
    let first = dice
        .write_keyed(&alice, "POST", "/api/dice/roll", body.clone(), "k1")
        .await;
    let again = dice
        .write_keyed(&alice, "POST", "/api/dice/roll", body, "k1")
        .await;
    assert!(again.replayed);
    assert_eq!(
        again.body, first.body,
        "not a second chance at a better roll"
    );
    let rolls = dice.get(&alice, "/api/dice").await.body["rolls"].clone();
    assert_eq!(rolls.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_roll_is_announced_and_the_fallback_lists_it() {
    let dice = dice(5).await;
    let alice = person("u-alice", "Alice");
    let mut events = dice.listen(&alice).await;
    let rolled = dice
        .write(
            &alice,
            "POST",
            "/api/dice/roll",
            Some(json!({ "sides": 8, "count": 2 })),
        )
        .await;
    assert_eq!(events.changed().await, Some(json!({ "panel": PANEL })));

    let Envelope::Records(table) = dice.fallback(&alice).await else {
        panic!("the dice's fallback is a table");
    };
    assert_eq!(table.rows[0]["who"], "Alice");
    assert_eq!(table.rows[0]["total"], rolled.body["total"]);
    assert!(table.rows[0]["roll"].as_str().unwrap().starts_with("2d8 ("));
}

// -- Its own UI (HLIN-I-0013) ---------------------------------------------

/// The dice as its own UI reaches it: `/api/` from the root, as the demo's
/// local user, over the same handlers as Hlin's `/hlin/api/`.
async fn dice_with_its_own_ui() -> Running {
    let layout = Site {
        local_user: Some("Local User".to_string()),
        ..Site::default()
    };
    testing::start_site(
        widget(),
        Dice::seeded(1),
        api(),
        ModuleFiles::none(),
        layout,
    )
    .await
}

#[tokio::test]
async fn its_own_ui_rolls_on_the_same_table_hlin_reads() {
    let dice = dice_with_its_own_ui().await;
    let rolled = dice
        .own(
            "POST",
            "/api/dice/roll",
            Some(&testing::fresh_key()),
            Some(json!({ "sides": 6, "count": 3 })),
        )
        .await;
    assert_eq!(rolled.status, 201, "{}", rolled.body);

    let seen = dice.get(&person("u-bob", "Bob"), "/api/dice").await;
    assert_eq!(seen.body["rolls"][0]["name"], "Local User");
    assert_eq!(seen.body["rolls"][0]["total"], rolled.body["total"]);
}
