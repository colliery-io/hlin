//! The HTTP clients the shell reaches platforms with.
//!
//! One place, because there are two kinds and they were previously built in
//! four: the manifest client, the shell's own, the one for held-open streams,
//! and a fresh one per surface that duplicated the second. Four sites is the
//! shape of thing a new setting reaches three of — and the setting this module
//! exists for is a trust anchor, where reaching three of four means a shell
//! that talks to some of its platforms and silently not the rest.
//!
//! # Two kinds, and why the difference matters
//!
//! A fetch has a request timeout, because a platform that never answers should
//! not hold a panel forever. A subscription must not, because a request timeout
//! covers the whole exchange including the body, and an event stream is all
//! body — every subscription died after ten seconds until these were separated.
//!
//! # Trust
//!
//! The default is the platform's own trust store, which is what
//! `rustls-platform-verifier` reads. That is right for a platform on a public
//! certificate and useless for one on an internal CA — which is how most of the
//! platforms this shell is built to front are actually served.
//!
//! So a configured bundle is *merged* with that store rather than replacing it:
//! a deployment with one internal CA and one public IdP needs both, and a shell
//! that made you choose would be a shell nobody could configure correctly.

use std::path::Path;

use crate::config::Config;

/// The clients the shell uses, built once.
#[derive(Clone)]
pub struct Clients {
    /// For anything that answers and finishes: manifests, panel data, options,
    /// an identity provider's discovery and token endpoints.
    pub fetching: reqwest::Client,

    /// For connections meant to stay open, which take a connect timeout and no
    /// request timeout.
    pub streaming: reqwest::Client,
}

impl Clients {
    /// Build both from configuration.
    pub fn build(config: &Config) -> Result<Self, String> {
        let anchors = extra_anchors(config.ca_bundle.as_deref())?;

        let fetching = reqwest::Client::builder()
            .timeout(config.timings.upstream_timeout())
            .user_agent(concat!("hlin/", env!("CARGO_PKG_VERSION")))
            .tls_certs_merge(anchors.clone())
            .build()
            .map_err(|error| format!("could not build the client: {error}"))?;

        let streaming = reqwest::Client::builder()
            .connect_timeout(config.timings.upstream_timeout())
            .user_agent(concat!("hlin/", env!("CARGO_PKG_VERSION")))
            .tls_certs_merge(anchors)
            .build()
            .map_err(|error| format!("could not build the streaming client: {error}"))?;

        Ok(Self {
            fetching,
            streaming,
        })
    }

    /// Clients trusting nothing but the platform's own store, for tests.
    pub fn plain() -> Self {
        Self {
            fetching: reqwest::Client::new(),
            streaming: reqwest::Client::new(),
        }
    }
}

/// The certificates in a configured bundle, or none where none was configured.
///
/// A bundle rather than one certificate, because an internal chain is usually
/// more than one and splitting it across settings would be arithmetic the
/// operator has to get right for no reason.
pub fn extra_anchors(bundle: Option<&Path>) -> Result<Vec<reqwest::Certificate>, String> {
    let Some(path) = bundle else {
        return Ok(Vec::new());
    };

    let pem = std::fs::read(path).map_err(|error| {
        format!(
            "could not read the CA bundle at {}: {error}",
            path.display()
        )
    })?;

    let certs = reqwest::Certificate::from_pem_bundle(&pem).map_err(|error| {
        format!(
            "{} is not a PEM certificate bundle: {error}",
            path.display()
        )
    })?;

    if certs.is_empty() {
        // An empty file parses. Refusing it is the point: somebody who named a
        // bundle meant to add trust, and a shell that quietly added none would
        // fail later, against every platform at once, looking like an outage.
        return Err(format!("{} contains no certificates", path.display()));
    }

    Ok(certs)
}

#[cfg(test)]
mod tests {
    use super::extra_anchors;
    use std::io::Write;
    use std::path::PathBuf;

    /// A self-signed CA, generated for the rig that proved this module works.
    /// Expiry does not matter: parsing a bundle is not verifying a chain.
    const INTERNAL_CA: &str = include_str!("../tests/data/internal-ca.pem");

    /// A file that lives for one test, named after it.
    fn scratch(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("hlin-anchors-{name}-{}", std::process::id()));
        let mut file = std::fs::File::create(&path).expect("could not create the scratch file");
        file.write_all(contents.as_bytes())
            .expect("could not write the scratch file");
        path
    }

    #[test]
    fn no_bundle_configured_adds_nothing() {
        assert!(
            extra_anchors(None)
                .expect("none is not an error")
                .is_empty()
        );
    }

    #[test]
    fn a_bundle_becomes_anchors() {
        let path = scratch("good", INTERNAL_CA);
        let anchors = extra_anchors(Some(&path)).expect("a valid bundle should parse");
        assert_eq!(anchors.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    /// Two, because an internal chain usually is, and splitting it across
    /// settings would be arithmetic the operator has to get right for nothing.
    #[test]
    fn a_bundle_may_hold_more_than_one() {
        let path = scratch("chain", &format!("{INTERNAL_CA}{INTERNAL_CA}"));
        let anchors = extra_anchors(Some(&path)).expect("a chain should parse");
        assert_eq!(anchors.len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    /// Naming a bundle that is not there is a typo, and a typo that started
    /// anyway would take down every platform at once, looking like an outage.
    #[test]
    fn a_missing_bundle_is_refused_by_name() {
        let path = std::env::temp_dir().join("hlin-anchors-nothing-here.pem");
        let _ = std::fs::remove_file(&path);
        let reason = extra_anchors(Some(&path)).expect_err("a missing bundle is an error");
        assert!(reason.contains("hlin-anchors-nothing-here.pem"), "{reason}");
    }

    /// An empty file parses. Somebody who named a bundle meant to add trust.
    #[test]
    fn an_empty_bundle_is_refused() {
        let path = scratch("empty", "");
        let reason = extra_anchors(Some(&path)).expect_err("an empty bundle is an error");
        assert!(reason.contains("no certificates"), "{reason}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn something_that_is_not_a_bundle_is_refused() {
        let path = scratch("garbage", "-----BEGIN CERTIFICATE-----\nnot base64\n");
        let reason = extra_anchors(Some(&path)).expect_err("garbage is an error");
        assert!(
            reason.contains("not a PEM certificate bundle") || reason.contains("no certificates"),
            "{reason}"
        );
        let _ = std::fs::remove_file(&path);
    }
}
