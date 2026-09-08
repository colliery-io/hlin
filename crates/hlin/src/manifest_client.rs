//! Fetching what a platform says about itself.
//!
//! Manifests are fetched by the shell as itself, on a schedule, with no viewer
//! in the loop. That is why a manifest is identical for every person and why
//! per-user visibility can only be decided at data-fetch time
//! (decision HLIN-A-0004).
//!
//! A trait, so the registry's tests never touch a network.

use std::time::Duration;

use async_trait::async_trait;
use hlin_manifest::Manifest;

/// What came back when the shell asked a platform for its manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// A document the shell could read. Whether it is *valid* is a separate
    /// question, answered by `hlin_manifest::validate`.
    Document(Box<Manifest>),

    /// The platform did not answer, or answered with a status that is not a
    /// document: a refused connection, a timeout, a 5xx.
    Unreachable {
        /// What happened, for an operator.
        reason: String,
    },

    /// The platform answered, but not with a manifest.
    Unreadable {
        /// What was wrong with it.
        reason: String,
    },
}

/// Fetches manifests.
#[async_trait]
pub trait ManifestClient: Send + Sync {
    /// Ask one platform for its manifest.
    async fn fetch(&self, base_url: &str) -> Fetched;
}

/// The real one.
pub struct HttpManifestClient {
    client: reqwest::Client,
}

impl HttpManifestClient {
    /// A client that gives up after `timeout`.
    pub fn new(timeout: Duration) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("hlin/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self { client })
    }
}

#[async_trait]
impl ManifestClient for HttpManifestClient {
    async fn fetch(&self, base_url: &str) -> Fetched {
        let url = format!(
            "{}/{}",
            base_url.trim_end_matches('/'),
            hlin_manifest::manifest::WELL_KNOWN_PATH
        );

        let response = match self.client.get(&url).send().await {
            Ok(response) => response,
            Err(error) => {
                return Fetched::Unreachable {
                    reason: error.to_string(),
                };
            }
        };

        let status = response.status();
        if !status.is_success() {
            // A platform that answers with an error is reachable but not
            // serving a manifest. Either way the shell has nothing to read, and
            // an operator wants the status.
            return Fetched::Unreachable {
                reason: format!("answered {status}"),
            };
        }

        // A manifest is small — panels, kinds, endpoints — and this is the one
        // call the shell makes to a platform it has never successfully spoken
        // to, so it is the least trustworthy of the three. Bounded generously:
        // a manifest over a megabyte is a defect whatever else it is.
        let body = match crate::bounded::read_bounded(
            response,
            hlin_manifest::envelope::MAX_DOCUMENT_BYTES,
        )
        .await
        {
            Ok(body) => body,
            Err(crate::bounded::TooMuch::Oversized { bytes, limit }) => {
                // Unreadable rather than unreachable: the platform answered.
                return Fetched::Unreadable {
                    reason: format!("manifest is at least {bytes} bytes, over the {limit} limit"),
                };
            }
            Err(error) => {
                return Fetched::Unreachable {
                    reason: error.to_string(),
                };
            }
        };

        match hlin_manifest::parse(&body) {
            Ok(manifest) => Fetched::Document(Box::new(manifest)),
            Err(error) => Fetched::Unreadable {
                reason: error.to_string(),
            },
        }
    }
}
