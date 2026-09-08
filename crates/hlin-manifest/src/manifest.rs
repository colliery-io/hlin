//! The manifest document: the contract each platform serves about itself.
//!
//! These types mirror specification HLIN-S-0001 field for field. They are
//! deliberately permissive on parse: unknown fields at every level are captured
//! rather than rejected, so a platform can add fields ahead of the shell without
//! coordinating an upgrade (REQ-1.3). Rules that a well-formed document must
//! satisfy are checked separately, in [`crate::validate`], so that one bad panel
//! rejects only itself.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use semver::Version;
use serde::de::{Error as DeError, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// The manifest schema version this crate understands.
///
/// The shell accepts this version and every version below it. A manifest
/// declaring a higher version is read at this version: known fields are
/// honoured and unknown ones ignored (REQ-2.1).
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// The well-known path, relative to a platform's base, where its manifest lives.
pub const WELL_KNOWN_PATH: &str = ".well-known/hlin.json";

/// A platform's manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Version of the document format. Monotonic, additive, shell-owned.
    pub schema_version: u32,

    /// The platform's own declared contract version. Majors declare breakage
    /// (decision HLIN-A-0002).
    pub contract_version: Version,

    /// Identity of the platform serving this document.
    pub platform: Platform,

    /// Navigation entries this platform contributes to the shell.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub navigation: Vec<NavigationEntry>,

    /// The panels this platform offers. This array is the contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panels: Vec<Panel>,

    /// Health endpoint, relative to the platform base.
    pub health: String,

    /// Where this platform reports that a panel's data changed, relative to the
    /// platform base (specification HLIN-S-0006).
    ///
    /// Absent means this platform is polled and nothing changes for it, which is
    /// the whole adoption story: a platform that never hears of this field
    /// behaves exactly as it always did.
    ///
    /// Not contract, and excluded from the hash for the reason `kind` and
    /// `refresh_ms` are — the shell is correct without it by construction, so
    /// adding, moving or withdrawing an event stream is not a breaking change
    /// and owes nobody a major version (decision HLIN-A-0011).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<String>,

    /// Fields this version of the crate does not know about, preserved so that
    /// re-serialising a manifest does not discard a newer platform's additions.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Identity of the platform serving a manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Platform {
    /// Stable identifier. Shell configuration is authoritative; the manifest
    /// restates it so the document is self-describing (REQ-1.5).
    pub id: String,

    /// Human-readable display name.
    pub name: String,

    /// Icon name from the shared vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One navigation entry contributed by a platform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationEntry {
    /// Display label.
    pub label: String,

    /// Target within the platform's own frontend, relative to its base.
    pub path: String,

    /// Icon name from the shared vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Sort hint within the platform's group. The shell owns overall ordering.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub weight: i64,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn is_zero(weight: &i64) -> bool {
    *weight == 0
}

/// One panel a platform offers.
///
/// The contract content of a panel is `key`, `envelope`, `data`, `params` and
/// `lifecycle`. `kind` is the platform's *default* rendering rather than
/// contract, because a user may switch a panel to any kind that accepts its
/// envelope (decision HLIN-A-0003).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    /// Stable panel identity, unique within the manifest. Layouts reference
    /// `platform.id/key`. Removing a key is breaking.
    pub key: String,

    /// Default title. Users override this per layout.
    pub title: String,

    /// Shown in the panel picker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Default view kind, named from the shared vocabulary. Not contract.
    pub kind: String,

    /// How often this panel's data is worth refetching, in milliseconds.
    ///
    /// Not contract, and a hint rather than an instruction (decision
    /// HLIN-A-0009). The publisher knows their data's cadence and the shell has
    /// no way to guess it: a queue depth changes several times a second and a
    /// nightly batch count changes once a day, and one operator setting cannot
    /// be right for both.
    ///
    /// Excluded from the contract hash for the same reason `kind` is — it is
    /// what a platform suggests, not what it promises — so a platform that
    /// discovers its data is slower than it thought can say so without a major
    /// version bump.
    ///
    /// The shell clamps it. A panel asking to be polled a thousand times a
    /// second gets the shell's floor: the shell pays for the requests and
    /// answers for the load on every platform it fronts, so it keeps the last
    /// word. Asking to be polled *less* often is always honoured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ms: Option<u64>,

    /// That this platform reports changes to this panel on its event stream.
    ///
    /// Deliberately separate from the platform-level `events` path, and not
    /// derivable from it. Without a per-panel declaration the shell cannot tell
    /// *no event has arrived because nothing changed* from *this panel is never
    /// reported on* — and would relax the polling of a panel it will never hear
    /// about, trading redundant requests for silent staleness. That is a worse
    /// deal than the one it started with, so the panel has to say.
    ///
    /// A panel declaring this on a platform that declares no `events` is not an
    /// error. It is a platform that has not finished, and the shell polls it the
    /// way it always has.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pushed: bool,

    /// A component in the design system, by whatever name that design system
    /// uses. Not contract, and never interpreted here.
    ///
    /// The vocabulary in `kind` is closed on purpose, and closing it has a
    /// cost: a design system's own components are unreachable, because a
    /// platform has no way to ask for one. This is the way to ask. The shell
    /// forwards the string to whichever design pack is mounted and forms no
    /// opinion about it, so a platform and a pack can agree on a component the
    /// shell has never heard of.
    ///
    /// `kind` stays required and stays a word Hlin knows, which is what makes
    /// this safe: a pack that does not recognise the component draws the kind
    /// instead, and validation has already proved the kind can draw the
    /// declared envelope. There is no way for naming a component to make a
    /// panel undrawable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,

    /// The envelope this panel's data endpoint returns. Contract.
    pub envelope: String,

    /// Data endpoint, relative to the platform base. Contract.
    pub data: String,

    /// Shell-level controls this panel responds to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<ParamDecl>,

    /// Lifecycle status. Absent means active.
    #[serde(default)]
    pub lifecycle: Lifecycle,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A parameter declaration: a name from the shared vocabulary, plus whatever
/// configuration that vocabulary entry defines.
///
/// Two forms are accepted on the wire. A bare string is shorthand for a
/// parameter that takes no configuration (`"time_range"`); an object names the
/// parameter in a `param` field and carries its configuration alongside
/// (`{"param": "select", "id": "cluster", ...}`). Both parse to this type, and
/// canonicalisation always uses the object form.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    /// Parameter name from the shared vocabulary.
    pub param: String,

    /// Configuration defined by that parameter's vocabulary entry.
    pub config: Map<String, Value>,
}

impl ParamDecl {
    /// A parameter that takes no configuration.
    pub fn bare(param: impl Into<String>) -> Self {
        Self {
            param: param.into(),
            config: Map::new(),
        }
    }

    /// This declaration in its object form, the shape canonicalisation uses.
    pub fn to_object(&self) -> Value {
        let mut object = Map::new();
        object.insert("param".to_string(), Value::String(self.param.clone()));
        for (key, value) in &self.config {
            object.insert(key.clone(), value.clone());
        }
        Value::Object(object)
    }

    /// The value that identifies this parameter within a panel across
    /// revisions.
    ///
    /// A parameter that names a query key in an `id` field is identified by the
    /// pair, so that two `select` controls on one panel are distinguishable.
    /// Everything else is identified by name alone.
    pub fn identity(&self) -> (String, Option<String>) {
        let id = self
            .config
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string);
        (self.param.clone(), id)
    }
}

impl Serialize for ParamDecl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.config.is_empty() {
            serializer.serialize_str(&self.param)
        } else {
            self.to_object().serialize(serializer)
        }
    }
}

impl<'de> Deserialize<'de> for ParamDecl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::String(param) => Ok(Self::bare(param)),
            Value::Object(mut object) => {
                let param = match object.remove("param") {
                    Some(Value::String(param)) => param,
                    Some(other) => {
                        return Err(D::Error::invalid_type(
                            unexpected(&other),
                            &"a parameter name",
                        ));
                    }
                    None => {
                        return Err(D::Error::missing_field("param"));
                    }
                };
                Ok(Self {
                    param,
                    config: object,
                })
            }
            other => Err(D::Error::invalid_type(
                unexpected(&other),
                &"a parameter name or a parameter object",
            )),
        }
    }
}

fn unexpected(value: &Value) -> Unexpected<'_> {
    match value {
        Value::Null => Unexpected::Unit,
        Value::Bool(inner) => Unexpected::Bool(*inner),
        Value::Number(_) => Unexpected::Other("a number"),
        Value::String(inner) => Unexpected::Str(inner),
        Value::Array(_) => Unexpected::Seq,
        Value::Object(_) => Unexpected::Map,
    }
}

/// Where a panel sits in its life.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Lifecycle {
    /// Whether the panel is active or on its way out.
    pub status: LifecycleStatus,

    /// End of the deprecation window. Required when deprecated; past it, the
    /// panel is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sunset: Option<NaiveDate>,

    /// The panel key that replaces this one, if there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor: Option<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            status: LifecycleStatus::Active,
            sunset: None,
            successor: None,
            extra: BTreeMap::new(),
        }
    }
}

impl Lifecycle {
    /// Whether this panel has been deprecated.
    pub fn is_deprecated(&self) -> bool {
        self.status == LifecycleStatus::Deprecated
    }
}

/// The lifecycle states a panel can declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleStatus {
    /// Offered normally.
    Active,
    /// Still served, but on a window that ends at `sunset`.
    Deprecated,
}

impl Manifest {
    /// The panel with this key, if the manifest declares one.
    pub fn panel(&self, key: &str) -> Option<&Panel> {
        self.panels.iter().find(|panel| panel.key == key)
    }
}
