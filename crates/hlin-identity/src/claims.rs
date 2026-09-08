//! What a token says, and who it says it about.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A person, as the shell knows them.
///
/// Produced by whichever authenticator strategy the deployment runs
/// (HLIN-S-0005) and carried unchanged through minting.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Principal {
    /// Stable, opaque identifier. The deduplication key, and the token's `sub`.
    pub sub: String,
    /// Display name, where the source supplies one.
    pub name: Option<String>,
    /// Email, where the source supplies one.
    pub email: Option<String>,
    /// Whatever the identity provider asserts.
    ///
    /// Passed through and never interpreted. The shell does not know what any
    /// group means and decides nothing from one; a platform using groups is
    /// using its own provider's data, with Hlin as courier.
    pub groups: Vec<String>,
}

impl Principal {
    /// A principal with just an identifier.
    pub fn new(sub: impl Into<String>) -> Self {
        Self {
            sub: sub.into(),
            ..Default::default()
        }
    }

    /// The same principal, with a display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The same principal, in these groups.
    pub fn with_groups<I, S>(mut self, groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.groups = groups.into_iter().map(Into::into).collect();
        self
    }
}

/// What a verified token asserts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Claims {
    /// The shell that minted this.
    pub iss: String,
    /// The principal.
    pub sub: String,
    /// The platform this token is for, and only this one.
    pub aud: String,
    /// Issued at, epoch seconds.
    pub iat: i64,
    /// Expires at, epoch seconds.
    pub exp: i64,
    /// Unique per token, so a platform may keep a replay cache if it wants one.
    pub jti: String,

    /// Display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Email.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Groups, exactly as the identity provider asserted them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,

    /// Claims a newer shell added that this crate does not know.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Claims {
    /// The principal these claims describe.
    pub fn principal(&self) -> Principal {
        Principal {
            sub: self.sub.clone(),
            name: self.name.clone(),
            email: self.email.clone(),
            groups: self.groups.clone(),
        }
    }

    /// Whether the principal is in a group.
    ///
    /// A convenience for platforms, which is the only place group membership
    /// means anything.
    pub fn in_group(&self, group: &str) -> bool {
        self.groups.iter().any(|held| held == group)
    }
}
