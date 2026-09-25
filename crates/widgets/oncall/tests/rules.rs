//! The on-call rota, called the way the shell calls it, and its rules on
//! weeks the test chooses.

use chrono::{NaiveDate, Utc};
use hlin_widget_oncall::{
    AHEAD, PANEL, ROSTER, Rota, api, epoch, monday, on_call, parse_week, week_name, widget,
};
use hlin_widget_support::Claims;
use hlin_widget_support::envelope::Envelope;
use hlin_widget_support::testing::{self, Running, person};
use serde_json::{Value, json};

async fn rota() -> Running {
    testing::start(widget(), Rota::seed(), api()).await
}

fn roster() -> Vec<String> {
    ROSTER.iter().map(|name| name.to_string()).collect()
}

fn day(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn claims(name: &str) -> Claims {
    Claims {
        iss: "hlin".to_string(),
        sub: format!("u-{name}"),
        aud: PANEL.to_string(),
        iat: 0,
        exp: 0,
        jti: "t".to_string(),
        name: Some(name.to_string()),
        email: None,
        groups: Vec::new(),
        extra: Default::default(),
    }
}

#[test]
fn the_manifest_is_one_the_shell_accepts_whole() {
    assert_eq!(widget().defects(PANEL), None);
}

#[test]
fn the_fallback_is_a_table_that_is_asked_for_again_and_not_pushed() {
    let document = widget().manifest(PANEL);
    let panel = document.panel(PANEL).unwrap();
    assert_eq!(panel.kind.as_deref(), Some("table"));
    assert!(!panel.pushed, "nobody writes to the rota");
}

#[test]
fn the_roster_takes_turns_in_order_a_week_each() {
    let roster = roster();
    assert_eq!(on_call(&roster, epoch()), "Amara");
    for (turn, name) in ROSTER.iter().enumerate() {
        let week = epoch() + chrono::Duration::weeks(turn as i64);
        assert_eq!(on_call(&roster, week), *name);
    }
    assert_eq!(
        on_call(
            &roster,
            epoch() + chrono::Duration::weeks(ROSTER.len() as i64)
        ),
        "Amara",
        "and round again"
    );
    assert_eq!(
        on_call(&roster, epoch() - chrono::Duration::weeks(1)),
        "Freya",
        "and before the rota began, backwards"
    );
}

#[test]
fn every_day_of_a_week_has_the_same_person_and_monday_hands_over() {
    let roster = roster();
    let sunday = day(2026, 9, 27);
    let next_monday = day(2026, 9, 28);
    let this = on_call(&roster, monday(day(2026, 9, 21)));
    for date in 21..=27 {
        assert_eq!(on_call(&roster, monday(day(2026, 9, date))), this);
    }
    assert_ne!(
        on_call(&roster, monday(sunday)),
        on_call(&roster, monday(next_monday))
    );
}

#[test]
fn the_new_year_does_not_skip_or_repeat_a_turn_even_in_a_53_week_year() {
    let roster = roster();
    // 2026 has 53 ISO weeks: 2026-W53 is followed by 2027-W01.
    let last = parse_week("2026-W53").expect("2026 has a week 53");
    let first = parse_week("2027-W01").unwrap();
    assert_eq!(first - last, chrono::Duration::weeks(1));
    let at = |week| {
        roster
            .iter()
            .position(|name| name == on_call(&roster, week))
    };
    assert_eq!((at(last).unwrap() + 1) % roster.len(), at(first).unwrap());
}

#[test]
fn a_week_is_named_the_iso_way_and_nothing_else_is_one() {
    assert_eq!(week_name(day(2026, 9, 21)), "2026-W39");
    assert_eq!(week_name(day(2024, 12, 30)), "2025-W01");
    assert_eq!(parse_week("2026-W39"), Some(day(2026, 9, 21)));
    for nonsense in [
        "2025-W53", "2026-W00", "2026-W9", "26-W39", "2026-39", "next",
    ] {
        assert_eq!(parse_week(nonsense), None, "{nonsense}");
    }
}

#[test]
fn someone_on_the_roster_is_told_their_next_turn() {
    let rota = Rota::seed();
    let today = day(2026, 9, 23);
    let this_week = monday(today);
    let mine = rota.week(&claims("Chen"), this_week, today);
    let yours = mine.yours.expect("Chen is on the roster");
    assert_eq!(yours.name, "Chen");
    assert!(yours.starts >= this_week);
    assert!(yours.starts < this_week + chrono::Duration::weeks(ROSTER.len() as i64));

    assert_eq!(rota.week(&claims("Visitor"), this_week, today).yours, None);
}

#[test]
fn a_week_shows_the_turns_after_it_and_whether_it_is_now() {
    let rota = Rota::seed();
    let today = day(2026, 9, 23);
    let week = rota.week(&claims("Visitor"), monday(today), today);
    assert!(week.current);
    assert_eq!(week.turn.week, "2026-W39");
    assert_eq!(week.previous_week, "2026-W38");
    assert_eq!(week.following_week, "2026-W40");
    assert_eq!(week.next.len(), AHEAD);
    assert_eq!(week.next[0].week, "2026-W40");

    let later = rota.week(&claims("Visitor"), parse_week("2026-W45").unwrap(), today);
    assert!(!later.current);
}

#[tokio::test]
async fn this_week_and_any_other_through_the_shell() {
    let rota = rota().await;
    let alice = person("u-alice", "Alice");

    let now = rota.get(&alice, "/api/oncall").await;
    assert_eq!(now.status, 200, "{}", now.body);
    assert_eq!(now.body["current"], true);
    let this_week = Utc::now().date_naive();
    assert_eq!(now.body["turn"]["week"], week_name(monday(this_week)));
    assert_eq!(now.body["yours"], Value::Null);

    let then = rota.get(&alice, "/api/oncall/2024-W01").await;
    assert_eq!(then.body["turn"]["name"], "Amara");
    assert_eq!(then.body["turn"]["starts"], "2024-01-01");
    assert_eq!(then.body["turn"]["ends"], "2024-01-07");

    let nonsense = rota.get(&alice, "/api/oncall/soon").await;
    assert_eq!(nonsense.status, 404);
    assert_eq!(
        nonsense.body,
        json!({ "message": "There is no such week; weeks are written like 2026-W39" })
    );
}

#[tokio::test]
async fn the_fallback_is_this_week_and_the_next_few() {
    let rota = rota().await;
    let alice = person("u-alice", "Alice");

    let Envelope::Records(table) = rota.fallback(&alice).await else {
        panic!("the rota's fallback is a table");
    };
    assert_eq!(table.rows.len(), 1 + AHEAD);
    let this_week = monday(Utc::now().date_naive());
    assert_eq!(table.rows[0]["week"], week_name(this_week));
    assert_eq!(table.rows[0]["name"], on_call(&roster(), this_week));
}
