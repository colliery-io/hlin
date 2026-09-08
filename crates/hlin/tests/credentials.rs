//! One suite, every strategy.
//!
//! A strategy that behaves differently from the others is a bug rather than a
//! feature: the aggregator asks any of them for headers and must not care
//! which it got. These are the properties every strategy shares, run against
//! all of them, plus the safety rules that must be refused at startup rather
//! than at the first request (HLIN-S-0005).

use std::sync::Arc;

use hlin::config::{AuthConfig, Config, PlatformConfig, Timings};
use hlin::identity::{Carried, CredentialConfig, Viewer, build};
use hlin_identity::{Issuer, Principal};

fn issuer() -> Arc<Issuer> {
    Arc::new(Issuer::generate("hlin"))
}

fn viewer() -> Viewer {
    let mut viewer = Viewer::new(Principal::new("u_dev").with_groups(["oncall"]));
    viewer.carried = Carried::from_cookie_header(Some("session=abc123; other=ignored"));
    viewer
}

/// Every strategy, built and ready to be asked the same questions.
fn every_strategy() -> Vec<(&'static str, Box<dyn hlin::identity::Credentialer>)> {
    // SAFETY of the test, not of the code: static-bearer reads its key from the
    // environment, so the test provides one.
    unsafe { std::env::set_var("HLIN_TEST_BEARER", "a-shared-key") };

    vec![
        (
            "forward-session",
            build(
                &CredentialConfig::ForwardSession {
                    cookies: vec!["session".to_string()],
                },
                "orebank",
                issuer(),
            )
            .expect("builds"),
        ),
        (
            "hlin-token",
            build(&CredentialConfig::HlinToken, "orebank", issuer()).expect("builds"),
        ),
        (
            "static-bearer",
            build(
                &CredentialConfig::StaticBearer {
                    token_env: "HLIN_TEST_BEARER".to_string(),
                    acknowledge_shared_principal: true,
                },
                "orebank",
                issuer(),
            )
            .expect("builds"),
        ),
    ]
}

// -- What every strategy must do ------------------------------------------

#[test]
fn every_strategy_produces_headers_and_nothing_else() {
    for (name, credentialer) in every_strategy() {
        let headers = credentialer
            .headers(&viewer())
            .unwrap_or_else(|reason| panic!("{name} should produce headers: {reason}"));

        assert!(!headers.is_empty(), "{name} produced no headers");
        for (key, value) in &headers {
            assert_eq!(
                key,
                &key.to_lowercase(),
                "{name}: header names are lowercase"
            );
            assert!(!value.is_empty(), "{name}: header `{key}` is empty");
        }
    }
}

#[test]
fn every_strategy_names_itself() {
    for (name, credentialer) in every_strategy() {
        assert_eq!(credentialer.name(), name);
    }
}

#[test]
fn a_strategy_is_deterministic_or_says_why_not() {
    // Two calls for one viewer either agree, or differ only because the
    // strategy mints something fresh each time. Nothing else may vary.
    for (name, credentialer) in every_strategy() {
        let once = credentialer.headers(&viewer()).expect("headers");
        let twice = credentialer.headers(&viewer()).expect("headers");

        let keys_once: Vec<&String> = once.iter().map(|(key, _)| key).collect();
        let keys_twice: Vec<&String> = twice.iter().map(|(key, _)| key).collect();
        assert_eq!(
            keys_once, keys_twice,
            "{name} changed which headers it sets"
        );

        if name != "hlin-token" {
            assert_eq!(once, twice, "{name} should be stable for one viewer");
        }
    }
}

#[test]
fn only_static_bearer_admits_to_collapsing_everyone_into_one_caller() {
    for (name, credentialer) in every_strategy() {
        assert_eq!(
            credentialer.collapses_principals(),
            name == "static-bearer",
            "{name} reported the wrong thing about principal collapsing"
        );
    }
}

#[test]
fn nothing_a_viewer_carries_leaks_into_an_unrelated_strategys_headers() {
    // The viewer carries a cookie. Only the strategy that is meant to forward
    // it may do so.
    for (name, credentialer) in every_strategy() {
        let headers = credentialer.headers(&viewer()).expect("headers");
        let rendered = format!("{headers:?}");

        if name == "forward-session" {
            assert!(rendered.contains("abc123"));
        } else {
            assert!(
                !rendered.contains("abc123"),
                "{name} leaked the viewer's session cookie"
            );
        }
    }
}

// -- What each strategy does in particular --------------------------------

#[test]
fn forward_session_sends_only_the_cookies_it_was_told_to() {
    let credentialer = build(
        &CredentialConfig::ForwardSession {
            cookies: vec!["session".to_string()],
        },
        "orebank",
        issuer(),
    )
    .expect("builds");

    let headers = credentialer.headers(&viewer()).expect("headers");
    let (name, value) = &headers[0];

    assert_eq!(name, "cookie");
    assert!(value.contains("session=abc123"));
    assert!(
        !value.contains("other"),
        "a cookie the configuration did not name must not be forwarded"
    );
}

#[test]
fn forward_session_says_so_when_the_viewer_has_no_session() {
    let credentialer = build(
        &CredentialConfig::ForwardSession {
            cookies: vec!["session".to_string()],
        },
        "orebank",
        issuer(),
    )
    .expect("builds");

    let bare = Viewer::new(Principal::new("u_dev"));
    assert!(credentialer.headers(&bare).is_err());
}

#[test]
fn hlin_token_mints_a_token_the_target_platform_will_accept() {
    let issuer = issuer();
    let credentialer =
        build(&CredentialConfig::HlinToken, "orebank", issuer.clone()).expect("builds");

    let headers = credentialer.headers(&viewer()).expect("headers");
    let (name, token) = &headers[0];
    assert_eq!(name, &hlin_identity::IDENTITY_HEADER.to_lowercase());

    let verifier = hlin_identity::Verifier::with_keys("hlin", issuer.jwks());
    let claims = verifier
        .verify(token, "orebank")
        .expect("the platform accepts it");
    assert_eq!(claims.sub, "u_dev");
    assert!(claims.in_group("oncall"), "groups reach the platform");

    assert!(
        verifier.verify(token, "stampmill").is_err(),
        "and it is worthless anywhere else"
    );
}

#[test]
fn hlin_token_mints_a_fresh_token_each_time() {
    // Per request, never per stream: a browser stream outlives any sane token
    // lifetime.
    let credentialer = build(&CredentialConfig::HlinToken, "orebank", issuer()).expect("builds");
    let once = credentialer.headers(&viewer()).expect("headers");
    let twice = credentialer.headers(&viewer()).expect("headers");
    assert_ne!(once, twice);
}

// -- The rules that must fail at startup ----------------------------------

fn config_with(platform: PlatformConfig) -> Config {
    Config {
        bind: "127.0.0.1".to_string(),
        port: 8080,
        issuer: "hlin".to_string(),
        key_path: "/tmp/unused.key".into(),
        database_url: None,
        database_url_env: None,
        frontend: "unused".into(),
        auth: AuthConfig::default(),
        timings: Timings::default(),
        platforms: vec![platform],
    }
}

#[test]
fn forward_session_is_refused_across_an_origin_boundary() {
    // A cookie is a bearer credential for everything on its origin, so sending
    // one elsewhere hands that everything to whoever answers. Refused when an
    // operator is watching, not at the first request.
    let config = config_with(PlatformConfig {
        id: "orebank".to_string(),
        base_url: "https://somewhere-else.example.com/orebank".to_string(),
        auth: CredentialConfig::ForwardSession {
            cookies: vec!["session".to_string()],
        },
    });

    let complaint = config.check().expect_err("must be refused").to_string();
    assert!(complaint.contains("orebank"), "the platform is named");
    assert!(complaint.contains("same-origin"));
    assert!(
        complaint.contains("hlin-token"),
        "and the alternative is offered"
    );
}

#[test]
fn forward_session_is_allowed_on_the_shells_own_origin() {
    let config = config_with(PlatformConfig {
        id: "orebank".to_string(),
        base_url: "http://localhost:8080/orebank".to_string(),
        auth: CredentialConfig::ForwardSession {
            cookies: vec!["session".to_string()],
        },
    });
    assert!(config.check().is_ok());
}

#[test]
fn static_bearer_must_be_acknowledged() {
    unsafe { std::env::set_var("HLIN_TEST_BEARER", "a-shared-key") };

    let config = config_with(PlatformConfig {
        id: "orebank".to_string(),
        base_url: "http://127.0.0.1:9000".to_string(),
        auth: CredentialConfig::StaticBearer {
            token_env: "HLIN_TEST_BEARER".to_string(),
            acknowledge_shared_principal: false,
        },
    });

    let complaint = config.check().expect_err("must be refused").to_string();
    assert!(complaint.contains("same caller"));
    assert!(
        complaint.contains("lack access"),
        "the consequences are spelled out"
    );
}

#[test]
fn static_bearer_needs_its_key_to_exist() {
    let config = config_with(PlatformConfig {
        id: "orebank".to_string(),
        base_url: "http://127.0.0.1:9000".to_string(),
        auth: CredentialConfig::StaticBearer {
            token_env: "HLIN_TEST_DEFINITELY_UNSET".to_string(),
            acknowledge_shared_principal: true,
        },
    });
    assert!(config.check().is_err());
}

#[test]
fn two_platforms_cannot_claim_one_identity() {
    let mut config = config_with(PlatformConfig {
        id: "orebank".to_string(),
        base_url: "http://127.0.0.1:9000".to_string(),
        auth: CredentialConfig::HlinToken,
    });
    config.platforms.push(config.platforms[0].clone());

    assert!(config.check().is_err());
}

#[test]
fn a_platform_id_must_be_usable() {
    let config = config_with(PlatformConfig {
        id: "Ore Bank".to_string(),
        base_url: "http://127.0.0.1:9000".to_string(),
        auth: CredentialConfig::HlinToken,
    });
    assert!(config.check().is_err());
}
