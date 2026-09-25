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

pub use crate::bridge::SUPPORTED_BRIDGE_MAJORS;

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

    /// Where this platform's module assets live, as one prefix relative to the
    /// platform base: `/ui/` (specification HLIN-S-0007, *Assets*).
    ///
    /// The shell serves a module's code from its own origin, fetching it from
    /// the platform as itself, and only from under this prefix. Declaring it is
    /// what lets a platform ship modules at all: every `ui.entry` must fall
    /// under it, and a `ui` on a platform without it is refused.
    ///
    /// Contract. A layout holding a module relies on its code being reachable,
    /// so narrowing or moving the prefix is breaking and widening it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,

    /// The platform routes its modules may call through the shell, by method
    /// (specification HLIN-S-0007, *The request proxy*).
    ///
    /// One set per platform rather than one per module: the platform authorizes
    /// every request itself, and does not need protecting from its own module.
    /// What the prefixes protect is everything the platform did *not* mean to
    /// expose to a page — its manifest, its health endpoint, its admin routes.
    ///
    /// Contract, for the reason `assets` is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes: Option<Routes>,

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

/// The route prefixes a platform's modules may reach, split by what a request
/// may do.
///
/// `GET` and `HEAD` are reads and need a `read` prefix; `POST`, `PUT`, `PATCH`
/// and `DELETE` are writes and need a `write` prefix. They are declared apart
/// because a platform commonly lets a module read far more than it lets it
/// change, and the shell can then refuse a write before it costs the platform
/// anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Routes {
    /// Prefixes a module may read under: `/api/`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub read: Vec<String>,

    /// Prefixes a module may write under.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write: Vec<String>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl Routes {
    /// The prefixes declared for one kind of request.
    pub fn prefixes(&self, access: Access) -> &[String] {
        match access {
            Access::Read => &self.read,
            Access::Write => &self.write,
        }
    }
}

/// What a module's request may do to its platform, which decides the route
/// prefixes it must fall under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Access {
    /// `GET` and `HEAD`.
    Read,
    /// `POST`, `PUT`, `PATCH` and `DELETE`.
    Write,
}

impl Access {
    /// Both, in a stable order.
    pub const ALL: [Access; 2] = [Access::Read, Access::Write];

    /// The field name under `routes`.
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Read => "read",
            Access::Write => "write",
        }
    }
}

/// A UI module a platform ships for a panel or a navigation entry
/// (decision HLIN-A-0014).
///
/// The shell hosts it in a sandboxed frame and speaks to it over the bridge.
/// Nothing here is code: it says where the code is and which bridge it speaks,
/// and the shell decides whether and how to load it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleUi {
    /// The module's entry document, relative to the platform base and under
    /// the platform's `assets` prefix: `/ui/items/index.html`.
    pub entry: String,

    /// The major version of the bridge this module speaks. The shell refuses a
    /// major it does not support rather than mount a module it cannot talk to.
    pub bridge: u32,

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

    /// A module the shell hosts as this entry's page, instead of linking out
    /// to the platform's own frontend.
    ///
    /// An entry with a module keeps its `path`, which is still where the link
    /// goes when the module cannot be hosted. A module declared wrongly
    /// therefore costs the page, never the entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<ModuleUi>,

    /// Unknown fields, preserved.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn is_zero(weight: &i64) -> bool {
    *weight == 0
}

/// One panel a platform offers.
///
/// The contract content of a panel is `key`, `envelope`, `data`, `ui`,
/// `params` and `lifecycle`. `kind` is the platform's *default* rendering
/// rather than contract, because a user may switch a panel to any kind that
/// accepts its envelope (decision HLIN-A-0003).
///
/// A panel is drawn one of two ways, or offers both. The shell draws it from
/// `kind`, `envelope` and `data`; the platform's own module draws it from `ui`.
/// A panel declaring both is drawn by its module and falls back to the shell's
/// drawing when the module is unavailable (specification HLIN-S-0007). That is
/// why the data fields are optional in the type: they are required exactly
/// when `ui` is absent, which is a rule about one panel, so it is checked by
/// validation, where getting it wrong rejects that panel alone rather than
/// failing the parse of the whole manifest.
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
    /// Required unless the panel declares `ui`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// The module that draws this panel, when its platform ships one.
    /// Contract: removing it, moving its entry or changing its bridge major is
    /// breaking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<ModuleUi>,

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
    /// `kind` stays required wherever the shell draws, and stays a word Hlin
    /// knows, which is what makes this safe: a pack that does not recognise the component draws the kind
    /// instead, and validation has already proved the kind can draw the
    /// declared envelope. There is no way for naming a component to make a
    /// panel undrawable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,

    /// The envelope this panel's data endpoint returns. Contract. Required
    /// unless the panel declares `ui`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<String>,

    /// Data endpoint, relative to the platform base. Contract. Required unless
    /// the panel declares `ui`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,

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

/// What the shell needs to draw a panel itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataDecl<'a> {
    /// Default view kind.
    pub kind: &'a str,
    /// The envelope the data endpoint returns.
    pub envelope: &'a str,
    /// The data endpoint, relative to the platform base.
    pub data: &'a str,
}

impl Panel {
    /// The panel's data declaration, when it declares all of `kind`, `envelope`
    /// and `data` — which is to say, when the shell can draw it.
    ///
    /// A panel that validation accepted and that answers `None` here is drawn
    /// only by its module, and has nothing for the shell to fetch.
    pub fn drawn_by_shell(&self) -> Option<DataDecl<'_>> {
        Some(DataDecl {
            kind: self.kind.as_deref()?,
            envelope: self.envelope.as_deref()?,
            data: self.data.as_deref()?,
        })
    }
}
