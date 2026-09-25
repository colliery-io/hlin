//! A write's token, bound to its request.
//!
//! Decision HLIN-A-0013 binds a write's identity to its method and path so a
//! read token captured in a log cannot be replayed as a write. These check
//! that the binding holds, and especially that no spelling of a different path
//! passes for the bound one: a comparison that normalises differently on the
//! two sides fails silently, in the attacker's favour.

use hlin_identity::{
    BOUND_TOKEN_LIFETIME_SECONDS, BoundRequest, Issuer, Principal, Refusal, RequestRefusal,
    UnsafeRequest, Verifier, is_read, normalise_path,
};

const SHELL: &str = "hlin";
const PLATFORM: &str = "orebank";
const SEED: [u8; 32] = [7u8; 32];

fn issuer() -> Issuer {
    Issuer::from_seed(SHELL, SEED)
}

fn principal() -> Principal {
    Principal::new("u_01H8XK2P")
        .with_name("A Person")
        .with_groups(["platform-engineering", "oncall"])
}

fn bound(issuer: &Issuer, method: &str, path: &str) -> String {
    let request = BoundRequest::new(method, path).expect("a safe request");
    issuer
        .mint_bound(&principal(), PLATFORM, &request)
        .expect("mints")
}

// -- The round trip -------------------------------------------------------

#[test]
fn a_bound_token_verifies_for_the_request_it_names() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions/42");

    let claims = verifier
        .verify_request(&token, PLATFORM, "POST", "/api/actions/42")
        .expect("verifies");

    assert_eq!(claims.htm(), Some("POST"));
    assert_eq!(claims.htu(), Some("/api/actions/42"));
    assert!(claims.is_bound());
    assert_eq!(
        claims.principal(),
        principal(),
        "the usual claims ride along"
    );
    assert_eq!(claims.aud, PLATFORM);
}

#[test]
fn a_bound_token_lives_thirty_seconds() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "DELETE", "/api/actions/42");

    let claims = verifier.verify(&token, PLATFORM).expect("verifies");

    assert_eq!(claims.exp - claims.iat, BOUND_TOKEN_LIFETIME_SECONDS);
    assert_eq!(BOUND_TOKEN_LIFETIME_SECONDS, 30);
}

#[test]
fn every_method_can_be_bound_and_verified() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    for method in ["POST", "PUT", "PATCH", "DELETE", "GET", "HEAD"] {
        let token = bound(&issuer, method, "/api/actions");
        assert!(
            verifier
                .verify_request(&token, PLATFORM, method, "/api/actions")
                .is_ok(),
            "{method} should verify at its own path"
        );
    }
}

#[test]
fn the_query_is_not_part_of_the_binding() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions?dry-run=true");

    let claims = verifier
        .verify_request(&token, PLATFORM, "POST", "/api/actions?dry-run=false")
        .expect("the query differs but the endpoint is the same");
    assert_eq!(claims.htu(), Some("/api/actions"));
}

// -- Writes need a binding ------------------------------------------------

#[test]
fn a_write_with_an_unbound_token_is_refused() {
    // The replay this exists to stop: a read token, lifted from a log, spent
    // on a write.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = issuer.mint(&principal(), PLATFORM).unwrap();

    for method in ["POST", "PUT", "PATCH", "DELETE", "OPTIONS", "TRACE", "BREW"] {
        assert_eq!(
            verifier.verify_request(&token, PLATFORM, method, "/api/actions"),
            Err(RequestRefusal::Unbound(method.to_string())),
            "{method} is a write and needs a bound token"
        );
    }
}

#[test]
fn only_exact_get_and_head_count_as_reads() {
    // Methods are case-sensitive, so `get` is not `GET`, and a lenient server
    // must not turn a lowercase method into an unbound write.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = issuer.mint(&principal(), PLATFORM).unwrap();

    assert!(is_read("GET"));
    assert!(is_read("HEAD"));
    for method in ["get", "Get", "head", "GET ", " GET", ""] {
        assert!(!is_read(method), "`{method}` is not a read");
        assert!(
            verifier
                .verify_request(&token, PLATFORM, method, "/api/actions")
                .is_err(),
            "`{method}` with an unbound token is refused"
        );
    }
}

#[test]
fn a_read_with_an_unbound_token_is_unchanged() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = issuer.mint(&principal(), PLATFORM).unwrap();

    for method in ["GET", "HEAD"] {
        let through_request = verifier
            .verify_request(&token, PLATFORM, method, "/api/hlin/throughput?window=1h")
            .expect("a read needs no binding");
        assert_eq!(through_request, verifier.verify(&token, PLATFORM).unwrap());
        assert!(!through_request.is_bound());
    }
}

#[test]
fn a_bound_token_is_still_checked_for_everything_else() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions");

    assert!(matches!(
        verifier.verify_request(&token, "stampmill", "POST", "/api/actions"),
        Err(RequestRefusal::Identity(Refusal::WrongAudience { .. }))
    ));
    assert_eq!(
        verifier.verify_request("", PLATFORM, "POST", "/api/actions"),
        Err(RequestRefusal::Identity(Refusal::Absent))
    );
}

// -- The binding holds ----------------------------------------------------

#[test]
fn a_token_bound_to_one_method_is_refused_for_another() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions/42");

    for method in ["DELETE", "PUT", "post", "Post"] {
        assert!(
            matches!(
                verifier.verify_request(&token, PLATFORM, method, "/api/actions/42"),
                Err(RequestRefusal::WrongMethod { .. })
            ),
            "a token bound to POST is not good for `{method}`"
        );
    }
}

#[test]
fn a_bound_token_is_held_to_its_binding_even_on_a_read() {
    // It was minted for one request. Letting it wander onto other paths as a
    // read would be harmless today and a surprise tomorrow.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions/42");

    assert!(matches!(
        verifier.verify_request(&token, PLATFORM, "GET", "/api/actions/42"),
        Err(RequestRefusal::WrongMethod { .. })
    ));
}

#[test]
fn a_token_bound_to_one_path_is_refused_at_another() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "DELETE", "/api/actions/42");

    assert_eq!(
        verifier.verify_request(&token, PLATFORM, "DELETE", "/api/actions/43"),
        Err(RequestRefusal::WrongPath {
            bound: "/api/actions/42".to_string(),
            presented: "/api/actions/43".to_string(),
        })
    );
}

#[test]
fn no_look_alike_passes_for_the_bound_path() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "DELETE", "/api/actions/42");

    let look_alikes = [
        // Trailing and doubled slashes.
        "/api/actions/42/",
        "/api/actions/42//",
        "/api//actions/42",
        "//api/actions/42",
        // Dot segments, plain and encoded.
        "/api/actions/./42",
        "/api/actions/x/../42",
        "/api/%2e%2e/api/actions/42",
        "/api/actions/%2E/42",
        "/api/actions/.%2e/actions/42",
        // An encoded slash that would join or split segments.
        "/api/actions%2F42",
        "/api/actions%2f42",
        "/api%2Factions%2F42",
        // Backslashes, raw and encoded.
        "/api\\actions\\42",
        "/api/actions%5C42",
        // Case.
        "/API/actions/42",
        "/api/Actions/42",
        "/api/ACTIONS/42",
        // Decoding more than once.
        "/api/actions/%2542",
        "/api/actions/%34%2532",
        // Control characters and malformed escapes.
        "/api/actions/42%00",
        "/api/actions/42%0a",
        "/api/actions/4%2",
        "/api/actions/42%",
        "/api/actions/%zz",
        // Invalid UTF-8 once decoded.
        "/api/actions/%ff",
        // Not relative to the base at all.
        "api/actions/42",
        "http://evil.example/api/actions/42",
        "",
        // A lookalike segment.
        "/api/actions/42 ",
        "/api/actions/042",
    ];

    for path in look_alikes {
        assert!(
            verifier
                .verify_request(&token, PLATFORM, "DELETE", path)
                .is_err(),
            "`{path}` must not pass for /api/actions/42"
        );
    }
}

#[test]
fn an_escaped_spelling_of_the_same_path_is_the_same_path() {
    // Percent-decoding once is the normal form both sides agree on, so an
    // unreserved character written as an escape is the same path, whichever
    // side escaped it.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    let plain = bound(&issuer, "POST", "/api/actions/42");
    for path in [
        "/api/%61ctions/42",
        "/api/actions/%34%32",
        "/%61pi/actions/42",
    ] {
        assert!(
            verifier
                .verify_request(&plain, PLATFORM, "POST", path)
                .is_ok(),
            "`{path}` is /api/actions/42 decoded once"
        );
    }

    let escaped = bound(&issuer, "POST", "/api/%61ctions/42");
    assert!(
        verifier
            .verify_request(&escaped, PLATFORM, "POST", "/api/actions/42")
            .is_ok()
    );
}

#[test]
fn a_path_is_decoded_exactly_once() {
    assert_eq!(normalise_path("/a/%2541").unwrap(), "/a/%41");
    assert_ne!(normalise_path("/a/%2541").unwrap(), "/a/A");
    assert_eq!(normalise_path("/a/%41").unwrap(), "/a/A");
    assert_eq!(normalise_path("/a/%c3%a9").unwrap(), "/a/é");
    assert_eq!(normalise_path("/a/%C3%A9").unwrap(), "/a/é");
}

#[test]
fn the_normal_form_refuses_what_it_cannot_represent() {
    assert_eq!(normalise_path("/"), Ok("/".to_string()));
    assert_eq!(normalise_path("/?q=1"), Ok("/".to_string()));
    assert_eq!(normalise_path("/a/b#frag"), Ok("/a/b".to_string()));

    assert_eq!(normalise_path("a"), Err(UnsafeRequest::NotAbsolute));
    assert_eq!(normalise_path(""), Err(UnsafeRequest::NotAbsolute));
    assert_eq!(normalise_path("/a/"), Err(UnsafeRequest::EmptySegment));
    assert_eq!(normalise_path("//a"), Err(UnsafeRequest::EmptySegment));
    assert_eq!(normalise_path("/a/%2e"), Err(UnsafeRequest::DotSegment));
    assert_eq!(normalise_path("/a/.."), Err(UnsafeRequest::DotSegment));
    assert_eq!(normalise_path("/a/%2"), Err(UnsafeRequest::BadEscape));
    assert_eq!(
        normalise_path("/a%2Fb"),
        Err(UnsafeRequest::EncodedSeparator)
    );
    assert_eq!(normalise_path("/a\\b"), Err(UnsafeRequest::Backslash));
    assert_eq!(normalise_path("/a/%80"), Err(UnsafeRequest::NotUtf8));
}

#[test]
fn a_request_that_cannot_be_bound_is_refused_before_signing() {
    assert!(matches!(
        BoundRequest::new("POST", "/api//actions"),
        Err(UnsafeRequest::EmptySegment)
    ));
    assert!(matches!(
        BoundRequest::new("", "/api/actions"),
        Err(UnsafeRequest::Method(_))
    ));
    assert!(matches!(
        BoundRequest::new("PO ST", "/api/actions"),
        Err(UnsafeRequest::Method(_))
    ));

    let request = BoundRequest::new("PATCH", "/api/%61ctions?x=1").unwrap();
    assert_eq!(request.method(), "PATCH");
    assert_eq!(request.path(), "/api/actions", "stored in normal form");
}

// -- Bindings a shell should never mint -----------------------------------

#[test]
fn a_bound_token_that_lives_too_long_is_refused() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let now = chrono::Utc::now().timestamp();

    let token = forged(
        &issuer,
        serde_json::json!({
            "iat": now,
            "exp": now + 120,
            "htm": "POST",
            "htu": "/api/actions",
        }),
    );

    assert_eq!(
        verifier.verify_request(&token, PLATFORM, "POST", "/api/actions"),
        Err(RequestRefusal::TooLong)
    );
}

#[test]
fn half_a_binding_is_refused_whatever_the_method() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let now = chrono::Utc::now().timestamp();

    let bindings = [
        serde_json::json!({ "htm": "POST" }),
        serde_json::json!({ "htu": "/api/actions" }),
        serde_json::json!({ "htm": "POST", "htu": 42 }),
        serde_json::json!({ "htm": ["POST"], "htu": "/api/actions" }),
        serde_json::json!({ "htm": null, "htu": "/api/actions" }),
    ];

    for binding in bindings {
        let mut claims = binding.clone();
        claims["iat"] = now.into();
        claims["exp"] = (now + 30).into();
        let token = forged(&issuer, claims);

        for method in ["POST", "GET"] {
            assert_eq!(
                verifier.verify_request(&token, PLATFORM, method, "/api/actions"),
                Err(RequestRefusal::MalformedBinding),
                "{binding} on {method}"
            );
        }
    }
}

#[test]
fn the_plain_check_still_accepts_a_bound_token() {
    // `verify` knows nothing of bindings; a platform that has not adopted the
    // request-aware check still accepts a bound token as an identity, so
    // shipping bound tokens breaks nobody.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = bound(&issuer, "POST", "/api/actions");

    assert!(verifier.verify(&token, PLATFORM).is_ok());
}

/// A token with the usual claims plus these, signed with the test issuer's
/// key, for bindings the real issuer refuses to mint.
fn forged(issuer: &Issuer, overrides: serde_json::Value) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};

    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(&SEED);

    let mut claims = serde_json::json!({
        "iss": SHELL,
        "sub": "u_01H8XK2P",
        "aud": PLATFORM,
        "jti": "forged",
    });
    for (key, value) in overrides.as_object().expect("an object") {
        claims[key] = value.clone();
    }

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(issuer.kid().to_string());
    jsonwebtoken::encode(&header, &claims, &EncodingKey::from_ed_der(&der)).expect("signs")
}

// -- The axum extractor ---------------------------------------------------

#[cfg(feature = "axum")]
mod extractor {
    use std::sync::Arc;

    use axum::extract::FromRequestParts;
    use axum::http::{Request, StatusCode};
    use hlin_identity::extract::{HlinRequestIdentity, IdentityState};
    use hlin_identity::{IDENTITY_HEADER, Verifier};

    use super::{PLATFORM, SHELL, bound, issuer, principal};

    fn state(verifier: Verifier) -> IdentityState {
        IdentityState {
            verifier: Arc::new(verifier),
            audience: PLATFORM.to_string(),
            #[cfg(feature = "dev-identity")]
            development_principal: principal(),
        }
    }

    async fn extract(
        state: &IdentityState,
        method: &str,
        uri: &str,
        token: Option<&str>,
    ) -> Result<HlinRequestIdentity, axum::response::Response> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            builder = builder.header(IDENTITY_HEADER, token);
        }
        let (mut parts, ()) = builder.body(()).unwrap().into_parts();
        HlinRequestIdentity::from_request_parts(&mut parts, state).await
    }

    async fn reason(response: axum::response::Response) -> String {
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        body["reason"].as_str().unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn a_write_with_a_bound_token_is_let_through() {
        let issuer = issuer();
        let state = state(Verifier::with_keys(SHELL, issuer.jwks()));
        let token = bound(&issuer, "POST", "/api/actions/42");

        let HlinRequestIdentity(claims) =
            extract(&state, "POST", "/api/actions/42?confirm=yes", Some(&token))
                .await
                .expect("accepted");
        assert_eq!(claims.principal(), principal());
    }

    #[tokio::test]
    async fn a_write_with_a_read_token_is_a_401_that_says_why() {
        let issuer = issuer();
        let state = state(Verifier::with_keys(SHELL, issuer.jwks()));
        let token = issuer.mint(&principal(), PLATFORM).unwrap();

        let refused = extract(&state, "POST", "/api/actions/42", Some(&token))
            .await
            .expect_err("refused");
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        assert!(reason(refused).await.contains("bound to the request"));
    }

    #[tokio::test]
    async fn a_write_at_a_look_alike_path_is_a_401_that_says_why() {
        let issuer = issuer();
        let state = state(Verifier::with_keys(SHELL, issuer.jwks()));
        let token = bound(&issuer, "POST", "/api/actions/42");

        for uri in ["/api/actions/42/", "/api/actions/43", "/api//actions/42"] {
            let refused = extract(&state, "POST", uri, Some(&token))
                .await
                .expect_err("refused");
            assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
            assert!(!reason(refused).await.is_empty(), "{uri} says why");
        }
    }

    #[tokio::test]
    async fn a_read_with_an_unbound_token_is_let_through() {
        let issuer = issuer();
        let state = state(Verifier::with_keys(SHELL, issuer.jwks()));
        let token = issuer.mint(&principal(), PLATFORM).unwrap();

        assert!(
            extract(&state, "GET", "/api/hlin/throughput", Some(&token))
                .await
                .is_ok()
        );
    }

    #[cfg(not(feature = "dev-identity"))]
    #[tokio::test]
    async fn a_write_with_no_token_is_a_401() {
        let issuer = issuer();
        let state = state(Verifier::with_keys(SHELL, issuer.jwks()));

        let refused = extract(&state, "DELETE", "/api/actions/42", None)
            .await
            .expect_err("refused");
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    }
}
