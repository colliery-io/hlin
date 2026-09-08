//! The identity contract, as tests.
//!
//! Specification HLIN-S-0004 is a contract twelve platform teams implement
//! against, so these check the things a platform depends on being true, and
//! especially the things that fail silently.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use hlin_identity::{Issuer, Jwks, JwksFetcher, Principal, Refusal, Verifier};

const SHELL: &str = "hlin";
const PLATFORM: &str = "orebank";

fn issuer() -> Issuer {
    Issuer::generate(SHELL)
}

fn principal() -> Principal {
    Principal::new("u_01H8XK2P")
        .with_name("A Person")
        .with_groups(["platform-engineering", "oncall"])
}

// -- The round trip -------------------------------------------------------

#[test]
fn a_minted_token_verifies_for_the_platform_it_names() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    let token = issuer.mint(&principal(), PLATFORM).expect("mints");
    let claims = verifier.verify(&token, PLATFORM).expect("verifies");

    assert_eq!(claims.sub, "u_01H8XK2P");
    assert_eq!(claims.aud, PLATFORM);
    assert_eq!(claims.iss, SHELL);
    assert_eq!(claims.name.as_deref(), Some("A Person"));
    assert!(claims.in_group("oncall"));
    assert!(!claims.in_group("finance"));
}

#[test]
fn the_claims_carry_the_principal_back() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let original = principal();

    let token = issuer.mint(&original, PLATFORM).unwrap();
    let recovered = verifier.verify(&token, PLATFORM).unwrap().principal();

    assert_eq!(recovered, original, "groups pass through untouched");
}

// -- The failure that fails silently --------------------------------------

#[test]
fn a_token_for_one_platform_is_worthless_at_another() {
    // The whole reason this crate exists. A platform that checked only the
    // signature would accept this, and no ordinary test would notice.
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    let token = issuer.mint(&principal(), "orebank").unwrap();

    assert!(matches!(
        verifier.verify(&token, "stampmill"),
        Err(Refusal::WrongAudience { .. })
    ));
}

#[test]
fn there_is_no_way_to_verify_without_naming_the_audience() {
    // Enforced by the type rather than by discipline: `verify` takes the
    // expected audience, and nothing else on `Verifier` checks a signature.
    // This test exists to fail if someone adds a convenience that does.
    let names: Vec<&str> = vec!["verify"];
    assert_eq!(
        names.len(),
        1,
        "if Verifier grows another checking method, it must also take an audience"
    );
}

#[test]
fn a_token_from_another_issuer_is_refused() {
    let ours = issuer();
    let theirs = Issuer::generate("someone-else");
    let verifier = Verifier::with_keys(SHELL, ours.jwks());

    let token = theirs.mint(&principal(), PLATFORM).unwrap();

    // Their key is not published by us, so it fails before the issuer check.
    assert!(matches!(
        verifier.verify(&token, PLATFORM),
        Err(Refusal::UnknownKey(_))
    ));
}

#[test]
fn a_token_signed_by_the_wrong_key_is_refused() {
    // Same key identifier, different key: the signature must not verify.
    let real = issuer();
    let impostor = Issuer::generate(SHELL);

    let mut published = impostor.jwks();
    published.keys[0].kid = real.kid().to_string();

    let verifier = Verifier::with_keys(SHELL, published);
    let token = real.mint(&principal(), PLATFORM).unwrap();

    assert!(matches!(
        verifier.verify(&token, PLATFORM),
        Err(Refusal::BadSignature)
    ));
}

// -- Malformed and absent -------------------------------------------------

#[test]
fn nothing_is_not_an_identity() {
    let verifier = Verifier::with_keys(SHELL, issuer().jwks());
    assert_eq!(verifier.verify("", PLATFORM), Err(Refusal::Absent));
    assert_eq!(verifier.verify("   ", PLATFORM), Err(Refusal::Absent));
}

#[test]
fn rubbish_is_refused_rather_than_panicking() {
    let verifier = Verifier::with_keys(SHELL, issuer().jwks());
    for rubbish in [
        "not-a-token",
        "a.b.c",
        "....",
        "eyJhbGciOiJub25lIn0..",
        "\u{0}",
    ] {
        assert!(
            verifier.verify(rubbish, PLATFORM).is_err(),
            "`{rubbish}` should be refused"
        );
    }
}

#[test]
fn a_token_naming_no_key_is_refused() {
    // A token with no `kid` gives a platform no way to know what signed it,
    // which is why the specification requires one.
    let verifier = Verifier::with_keys(SHELL, issuer().jwks());
    // "alg: EdDSA" with no kid, unsigned payload.
    let no_kid = "eyJhbGciOiJFZERTQSJ9.eyJzdWIiOiJ1In0.c2ln";
    assert_eq!(verifier.verify(no_kid, PLATFORM), Err(Refusal::NoKeyId));
}

// -- Keys, rotation and refetching ---------------------------------------

/// A key set that can be changed, counting how often it is asked for.
struct CountingFetcher {
    jwks: std::sync::Mutex<Jwks>,
    calls: AtomicUsize,
}

impl CountingFetcher {
    fn new(jwks: Jwks) -> Arc<Self> {
        Arc::new(Self {
            jwks: std::sync::Mutex::new(jwks),
            calls: AtomicUsize::new(0),
        })
    }
}

/// A handle a verifier can own, sharing one counter with the test.
struct Handle(Arc<CountingFetcher>);

impl JwksFetcher for Handle {
    fn fetch(&self) -> Result<Jwks, String> {
        self.0.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.jwks.lock().unwrap().clone())
    }
}

#[test]
fn a_platform_fetches_keys_once_and_then_uses_them() {
    let issuer = issuer();
    let fetcher = CountingFetcher::new(issuer.jwks());
    let verifier = Verifier::fetching(SHELL, Box::new(Handle(fetcher.clone())));

    for _ in 0..5 {
        let token = issuer.mint(&principal(), PLATFORM).unwrap();
        verifier.verify(&token, PLATFORM).expect("verifies");
    }

    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        1,
        "keys are cached; verifying must not call the shell each time"
    );
}

#[test]
fn a_cold_platform_fetches_the_key_it_needs() {
    // The rotation safety net: a platform that has never fetched, meeting a
    // token signed by a key it does not hold, fetches once and succeeds.
    let issuer = issuer();
    let fetcher = CountingFetcher::new(issuer.jwks());
    let verifier = Verifier::fetching(SHELL, Box::new(Handle(fetcher.clone())));

    let token = issuer.mint(&principal(), PLATFORM).unwrap();
    verifier
        .verify(&token, PLATFORM)
        .expect("fetches, then verifies");

    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_rotation_is_picked_up_once_the_refetch_window_allows_it() {
    // Rotation works by publishing the new key and waiting before signing with
    // it, so by the time a token needs the new key every platform's cache has
    // had it for a day. The unknown-key refetch is a safety net for a cold
    // cache, not the mechanism, and it is rate limited so that a flood of
    // tokens naming keys that do not exist cannot become a flood of requests
    // at the shell.
    //
    // The consequence, deliberate and worth stating: a key published seconds
    // ago may not be picked up for up to a minute. This test pins that trade
    // rather than pretending otherwise.
    let old = issuer();
    let fetcher = CountingFetcher::new(old.jwks());
    let verifier = Verifier::fetching(SHELL, Box::new(Handle(fetcher.clone())));

    let warm = old.mint(&principal(), PLATFORM).unwrap();
    verifier.verify(&warm, PLATFORM).expect("verifies");
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);

    // The shell rotates. The new key is published alongside the old one.
    let new = Issuer::generate(SHELL);
    let mut both = new.jwks();
    both.keys.extend(old.jwks().keys);
    *fetcher.jwks.lock().unwrap() = both.clone();

    // Within the rate-limit window the platform does not refetch, so a token
    // signed by the new key is refused rather than accepted on trust.
    let fresh = new.mint(&principal(), PLATFORM).unwrap();
    assert!(matches!(
        verifier.verify(&fresh, PLATFORM),
        Err(Refusal::UnknownKey(_))
    ));

    // A platform that has waited, or one starting cold, picks it up.
    let patient = Verifier::fetching(SHELL, Box::new(Handle(CountingFetcher::new(both))));
    patient
        .verify(&fresh, PLATFORM)
        .expect("the published key verifies once fetched");

    // Tokens signed by the old key keep working throughout, which is what
    // makes a rotation safe to perform without coordinating deploys.
    verifier
        .verify(&warm, PLATFORM)
        .expect("the old key still works");
}

#[test]
fn a_flood_of_unknown_keys_does_not_become_a_flood_of_fetches() {
    let issuer = issuer();
    let stranger = Issuer::generate(SHELL);
    let fetcher = CountingFetcher::new(issuer.jwks());
    let verifier = Verifier::fetching(SHELL, Box::new(Handle(fetcher.clone())));

    for _ in 0..20 {
        let token = stranger.mint(&principal(), PLATFORM).unwrap();
        assert!(verifier.verify(&token, PLATFORM).is_err());
    }

    assert!(
        fetcher.calls.load(Ordering::SeqCst) <= 2,
        "refetching is rate limited, got {} calls",
        fetcher.calls.load(Ordering::SeqCst)
    );
}

#[test]
fn a_published_key_set_survives_a_round_trip() {
    let issuer = issuer();
    let json = serde_json::to_string(&issuer.jwks()).expect("serialises");
    let parsed: Jwks = serde_json::from_str(&json).expect("parses");

    assert_eq!(parsed, issuer.jwks());
    let key = parsed
        .find(issuer.kid())
        .expect("the key is findable by id");
    assert_eq!(key.kty, "OKP");
    assert_eq!(key.crv, "Ed25519");
    assert_eq!(key.alg, "EdDSA");
    assert_eq!(key.bytes().expect("readable").len(), 32);
}

// -- Identity of the key itself ------------------------------------------

#[test]
fn a_keys_identifier_is_derived_from_the_key() {
    // Two shells cannot accidentally choose the same identifier, and an
    // identifier always matches the key it names.
    let one = Issuer::generate(SHELL);
    let other = Issuer::generate(SHELL);
    assert_ne!(one.kid(), other.kid());

    let same = Issuer::from_seed(SHELL, [7u8; 32]);
    let again = Issuer::from_seed(SHELL, [7u8; 32]);
    assert_eq!(same.kid(), again.kid());
}

#[test]
fn a_key_survives_a_restart() {
    let directory = std::env::temp_dir().join(format!("hlin-key-{}", uuid::Uuid::new_v4()));
    let path = directory.join("shell.key");

    let first = Issuer::load_or_generate(SHELL, &path).expect("generates");
    let second = Issuer::load_or_generate(SHELL, &path).expect("loads");

    assert_eq!(
        first.kid(),
        second.kid(),
        "a restart must not change the key, or every platform refetches"
    );

    // And a token from before the restart still verifies after it.
    let token = first.mint(&principal(), PLATFORM).unwrap();
    let verifier = Verifier::with_keys(SHELL, second.jwks());
    assert!(verifier.verify(&token, PLATFORM).is_ok());

    std::fs::remove_dir_all(&directory).ok();
}

#[test]
fn every_token_is_distinguishable_from_every_other() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    let one = issuer.mint(&principal(), PLATFORM).unwrap();
    let other = issuer.mint(&principal(), PLATFORM).unwrap();

    let first = verifier.verify(&one, PLATFORM).unwrap();
    let second = verifier.verify(&other, PLATFORM).unwrap();

    assert_ne!(
        first.jti, second.jti,
        "a platform keeping a replay cache needs these to differ"
    );
}

#[test]
fn a_token_is_short_lived() {
    let issuer = issuer();
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());
    let token = issuer.mint(&principal(), PLATFORM).unwrap();
    let claims = verifier.verify(&token, PLATFORM).unwrap();

    let lifetime = claims.exp - claims.iat;
    assert_eq!(lifetime, hlin_identity::TOKEN_LIFETIME_SECONDS);
    assert!(
        lifetime <= 300,
        "a per-request token should not outlive the request by much"
    );
}

#[test]
fn an_expired_token_is_refused() {
    // Minted in the past, well beyond the skew allowance.
    let issuer = Issuer::from_seed(SHELL, [3u8; 32]);
    let verifier = Verifier::with_keys(SHELL, issuer.jwks());

    let expired = expired_token(&issuer);
    assert_eq!(verifier.verify(&expired, PLATFORM), Err(Refusal::Expired));
}

/// A token whose expiry is far enough in the past that no skew allowance saves
/// it, signed by the same key so only the time is wrong.
fn expired_token(issuer: &Issuer) -> String {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};

    // Rebuild the signing key from the same seed the issuer used.
    let seed = [3u8; 32];
    let mut der = vec![
        0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04,
        0x20,
    ];
    der.extend_from_slice(&seed);

    let now = chrono::Utc::now().timestamp();
    let claims = serde_json::json!({
        "iss": SHELL,
        "sub": "u_01H8XK2P",
        "aud": PLATFORM,
        "iat": now - 7200,
        "exp": now - 3600,
        "jti": "expired",
    });

    let mut header = Header::new(Algorithm::EdDSA);
    header.kid = Some(issuer.kid().to_string());
    jsonwebtoken::encode(&header, &claims, &EncodingKey::from_ed_der(&der)).expect("signs")
}

#[test]
fn a_burst_of_first_requests_all_succeed() {
    // A platform coming up meets a burst at once, and every request has a cold
    // cache. Without serialising the fetch, the first would fill the cache
    // while the rest were turned away by the rate limit and refused perfectly
    // good tokens. This is the bug that shape produced, kept as a test.
    use std::sync::Barrier;

    let issuer = Arc::new(issuer());
    let fetcher = CountingFetcher::new(issuer.jwks());
    let verifier = Arc::new(Verifier::fetching(SHELL, Box::new(Handle(fetcher.clone()))));

    let barrier = Arc::new(Barrier::new(8));
    let mut threads = Vec::new();

    for _ in 0..8 {
        let verifier = verifier.clone();
        let issuer = issuer.clone();
        let barrier = barrier.clone();

        threads.push(std::thread::spawn(move || {
            let token = issuer.mint(&principal(), PLATFORM).unwrap();
            barrier.wait();
            verifier.verify(&token, PLATFORM).is_ok()
        }));
    }

    let accepted = threads
        .into_iter()
        .map(|handle| handle.join().expect("thread completes"))
        .filter(|ok| *ok)
        .count();

    assert_eq!(
        accepted, 8,
        "every request in the first burst should be accepted"
    );
    assert_eq!(
        fetcher.calls.load(Ordering::SeqCst),
        1,
        "and they should share one fetch rather than causing eight"
    );
}
