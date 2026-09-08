//! The OpenID Connect authorization code flow, with PKCE.
//!
//! Everything here is protocol. What a session is, how the cookie is written
//! and where a person lands afterwards belong to the shell and live in the
//! parent module; this file knows only how to ask a provider who somebody is
//! and how to check the answer.
//!
//! The checking is the part worth reading. An id token is a claim by a party
//! the shell trusts, and every step below exists because skipping it turns that
//! into a claim by whoever is talking: the signature says the provider wrote
//! it, `iss` says which provider, `aud` says it was meant for this shell rather
//! than obtained legitimately for another, `exp` says it is still true, and the
//! nonce says it was minted for the sign-in in progress rather than replayed
//! from another one.

use std::collections::HashMap;

use base64::Engine;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;

use crate::bounded::read_bounded;
use crate::config::OidcConfig;

/// How much of a provider's answer the shell will read.
///
/// A discovery document, a key set and a token response are all small. Reading
/// without a bound means a provider — or something answering as one — decides
/// how much memory the shell uses.
const MOST: usize = 1024 * 1024;

/// What the shell learned about somebody.
#[derive(Debug, Clone, PartialEq)]
pub struct Claims {
    /// The provider's identifier for them. Stable, and not an email address.
    pub subject: String,

    /// Their display name, where the provider offered one.
    pub name: Option<String>,

    /// The groups claim, which platforms may use for their own decisions.
    pub groups: Vec<String>,
}

/// A provider, as its discovery document describes it.
#[derive(Debug, Clone)]
pub struct Provider {
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    /// As the provider names itself, which is what `iss` must equal.
    issuer: String,
}

/// The subset of the discovery document the shell uses.
#[derive(Debug, Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

impl Provider {
    /// Read a provider's own description of itself.
    ///
    /// Fetched rather than configured, because these endpoints are the
    /// provider's to move and a shell holding a copy of them is a shell that
    /// breaks on a migration it was not told about. Fetched per sign-in rather
    /// than cached, because a sign-in is a rare event on a human timescale and
    /// a stale cache here is an outage nobody can explain.
    pub async fn discover(client: &reqwest::Client, config: &OidcConfig) -> Result<Self, String> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            config.issuer.trim_end_matches('/')
        );

        let answer = client
            .get(&url)
            .send()
            .await
            .map_err(|error| format!("could not reach {url}: {error}"))?;

        if !answer.status().is_success() {
            return Err(format!("{url} answered {}", answer.status()));
        }

        let body = read_bounded(answer, MOST)
            .await
            .map_err(|error| format!("{url}: {error}"))?;

        let discovery: Discovery = serde_json::from_slice(&body)
            .map_err(|error| format!("{url} is not a discovery document: {error}"))?;

        // The document says who it is for, and the shell was told who it wanted.
        // A document served from the configured issuer that names a different
        // one is either a misconfiguration or a redirection, and neither should
        // proceed quietly.
        if discovery.issuer.trim_end_matches('/') != config.issuer.trim_end_matches('/') {
            return Err(format!(
                "`{url}` describes `{}`, not the configured issuer `{}`",
                discovery.issuer, config.issuer
            ));
        }

        Ok(Self {
            authorization_endpoint: discovery.authorization_endpoint,
            token_endpoint: discovery.token_endpoint,
            jwks_uri: discovery.jwks_uri,
            issuer: discovery.issuer,
        })
    }

    /// Where to send a browser to be asked who they are.
    pub fn authorization_url(
        &self,
        config: &OidcConfig,
        state: &str,
        nonce: &str,
        verifier: &str,
    ) -> String {
        let query = [
            ("response_type", "code"),
            ("client_id", &config.client_id),
            ("redirect_uri", &config.redirect_uri()),
            ("scope", &config.scopes.join(" ")),
            ("state", state),
            ("nonce", nonce),
            ("code_challenge", &challenge(verifier)),
            ("code_challenge_method", "S256"),
        ]
        .into_iter()
        .map(|(key, value)| format!("{key}={}", urlencode(value)))
        .collect::<Vec<_>>()
        .join("&");

        let joiner = if self.authorization_endpoint.contains('?') {
            '&'
        } else {
            '?'
        };
        format!("{}{joiner}{query}", self.authorization_endpoint)
    }

    /// Exchange a code for tokens, and read who the id token says this is.
    pub async fn redeem(
        &self,
        client: &reqwest::Client,
        config: &OidcConfig,
        code: &str,
        verifier: &str,
        nonce: &str,
    ) -> Result<Claims, String> {
        let secret = std::env::var(&config.client_secret_env)
            .map_err(|_| format!("`{}` is not in the environment", config.client_secret_env))?;

        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &config.redirect_uri()),
            ("client_id", &config.client_id),
            ("client_secret", &secret),
            // What makes the code useless to anyone who intercepted it: only
            // the party that sent the challenge can produce the verifier.
            ("code_verifier", verifier),
        ];

        // Encoded here rather than by `reqwest`'s `form`, which this build does
        // not include: the shell's client is built without the features it does
        // not need, and one join is cheaper than carrying another.
        let body = form
            .iter()
            .map(|(key, value)| format!("{key}={}", urlencode(value)))
            .collect::<Vec<_>>()
            .join("&");

        let answer = client
            .post(&self.token_endpoint)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|error| format!("could not reach the token endpoint: {error}"))?;

        let status = answer.status();
        let body = read_bounded(answer, MOST)
            .await
            .map_err(|error| format!("the token endpoint: {error}"))?;

        if !status.is_success() {
            // Deliberately without the body: a failed token exchange can echo
            // parts of the request, and the request carried the client secret.
            return Err(format!("the token endpoint answered {status}"));
        }

        let tokens: HashMap<String, Value> = serde_json::from_slice(&body)
            .map_err(|error| format!("the token endpoint did not answer with tokens: {error}"))?;

        let id_token = tokens
            .get("id_token")
            .and_then(Value::as_str)
            .ok_or("the token endpoint returned no id token")?;

        self.claims_of(client, config, id_token, nonce).await
    }

    /// Verify an id token and read what it says.
    async fn claims_of(
        &self,
        client: &reqwest::Client,
        config: &OidcConfig,
        id_token: &str,
        nonce: &str,
    ) -> Result<Claims, String> {
        let head = jsonwebtoken::decode_header(id_token)
            .map_err(|error| format!("the id token has no readable header: {error}"))?;

        let keys = self.keys(client).await?;
        let key = match &head.kid {
            Some(kid) => keys
                .find(kid)
                .ok_or_else(|| format!("the provider has no key `{kid}`"))?,
            // A key set with exactly one key needs no `kid` to choose between.
            // More than one and a token that does not say which is a token
            // nothing can check without guessing, and guessing here means
            // trying keys until one works.
            None if keys.keys.len() == 1 => &keys.keys[0],
            None => {
                return Err("the id token names no key and the provider has several".to_string());
            }
        };

        let key = DecodingKey::from_jwk(key)
            .map_err(|error| format!("the provider's key cannot be used: {error}"))?;

        let mut validation = Validation::new(head.alg);
        validation.set_audience(&[&config.client_id]);
        validation.set_issuer(&[&self.issuer]);
        // Present by default, and named here because they are the reason to
        // trust anything below: without them this is a base64 decoder.
        validation.validate_exp = true;
        validation.required_spec_claims = ["iss", "aud", "exp", "sub"]
            .into_iter()
            .map(str::to_string)
            .collect();

        // `none` is an algorithm in the specification and never a legitimate
        // one for an id token: it means "unsigned", and accepting it accepts
        // whatever anybody writes.
        if matches!(
            head.alg,
            Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
        ) {
            return Err("an id token signed with a shared secret is not acceptable".to_string());
        }

        let token = jsonwebtoken::decode::<HashMap<String, Value>>(id_token, &key, &validation)
            .map_err(|error| format!("the id token is not valid: {error}"))?;
        let claims = token.claims;

        // The nonce ties this token to the sign-in in progress. Without it a
        // token obtained legitimately in one sign-in can be presented in
        // another, which is the whole of the attack the parameter exists for.
        match claims.get("nonce").and_then(Value::as_str) {
            Some(found) if found == nonce => {}
            Some(_) => return Err("the id token belongs to a different sign-in".to_string()),
            None => return Err("the id token carries no nonce".to_string()),
        }

        let subject = claims
            .get("sub")
            .and_then(Value::as_str)
            .ok_or("the id token names no subject")?
            .to_string();

        let name = claims
            .get("name")
            .or_else(|| claims.get("preferred_username"))
            .or_else(|| claims.get("email"))
            .and_then(Value::as_str)
            .map(str::to_string);

        // Providers disagree about whether groups are an array or one
        // comma-separated string, and a shell that understands only one of
        // those silently gives everybody no groups at all against the other.
        let groups = match claims.get(&config.groups_claim) {
            Some(Value::Array(values)) => values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            Some(Value::String(value)) => value
                .split(',')
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };

        Ok(Claims {
            subject,
            name,
            groups,
        })
    }

    /// The provider's signing keys.
    async fn keys(&self, client: &reqwest::Client) -> Result<JwkSet, String> {
        let answer = client
            .get(&self.jwks_uri)
            .send()
            .await
            .map_err(|error| format!("could not reach {}: {error}", self.jwks_uri))?;

        if !answer.status().is_success() {
            return Err(format!("{} answered {}", self.jwks_uri, answer.status()));
        }

        let body = read_bounded(answer, MOST)
            .await
            .map_err(|error| format!("{}: {error}", self.jwks_uri))?;

        serde_json::from_slice(&body)
            .map_err(|error| format!("{} is not a key set: {error}", self.jwks_uri))
    }
}

/// The S256 challenge for a PKCE verifier.
fn challenge(verifier: &str) -> String {
    use sha2::Digest;

    let digest = sha2::Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

/// Percent-encode a query parameter.
///
/// Written out rather than pulled in: the shell needs this in one place, for
/// values it produced itself plus a configured client id, and the rule is four
/// lines.
fn urlencode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_challenge_is_the_one_the_specification_gives() {
        // RFC 7636 appendix B, so this checks the shell against the standard
        // rather than against itself.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn a_query_value_cannot_carry_its_own_parameters() {
        // The reason this function exists: an unencoded value with an `&` in it
        // is two parameters, and the second is whatever the value's author
        // wanted to say.
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("https://x/y"), "https%3A%2F%2Fx%2Fy");
    }

    #[test]
    fn unguessable_values_are_not_the_same_twice() {
        assert_ne!(super::super::unguessable(), super::super::unguessable());
    }

    #[test]
    fn a_session_is_stored_under_a_hash_of_its_cookie() {
        let value = "a-session-cookie";
        let stored = super::super::fingerprint(value);

        assert_ne!(stored, value);
        assert_eq!(stored.len(), 64);
        assert_eq!(stored, super::super::fingerprint(value));
    }
}
