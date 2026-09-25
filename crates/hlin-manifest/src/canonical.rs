//! Contract identity: what a manifest promises, reduced to a stable fingerprint.
//!
//! The hash answers one question only: *did the contract change*. It is
//! computed by the shell from the manifest it fetched, never declared by the
//! platform, so it cannot be forgotten or faked (decision HLIN-A-0002). The
//! platform's `contract_version` answers a different question, *was breakage
//! intended*, and the two together make the interesting case mechanically
//! detectable: contract changed, breakage not declared.
//!
//! Contract content is the `panels` array reduced to the fields a consumer can
//! pin to. Titles, descriptions, navigation and the health path are excluded
//! because changing them can break nobody. `kind` is excluded because it is the
//! platform's default rendering rather than a promise (decision HLIN-A-0003).
//! `contract_version` is excluded because bumping a version without changing
//! anything is not a change. `refresh_ms` and `component` are excluded for the
//! same reason as `kind`: they are what a platform suggests about presentation
//! and cadence, not what it promises about data (decisions HLIN-A-0003 and
//! HLIN-A-0009).
//!
//! Modules added four things a consumer can rely on: where a platform's module
//! code lives (`assets`), which routes its modules may call (`routes`), and for
//! each module its entry and the bridge major it speaks (`ui`). A layout
//! holding a module breaks if any of those move, so they are contract too. Each
//! enters the content only when declared, which is what keeps every manifest
//! written before modules existed at exactly the fingerprint it always had: a
//! shell upgrading past this change must not see every platform's contract
//! move at once.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::manifest::{Access, LifecycleStatus, Manifest, ModuleUi, Panel, Routes};

/// A manifest's contract fingerprint: SHA-256 over its canonicalised contract
/// content, hex encoded.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractHash(String);

impl ContractHash {
    /// The hash as a lowercase hex string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContractHash {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The contract content of a manifest: the structure the hash is taken over.
///
/// Panels are sorted by key and reduced to their contract fields, optional
/// fields are materialised to their defaults, parameter declarations are
/// normalised to object form and sorted, and unknown fields are dropped. Two
/// manifests that promise the same thing produce the same structure however
/// they were written.
pub fn contract_content(manifest: &Manifest) -> Value {
    let mut panels: Vec<(&str, Value)> = manifest
        .panels
        .iter()
        .map(|panel| (panel.key.as_str(), panel_contract(panel)))
        .collect();
    panels.sort_by(|left, right| left.0.cmp(right.0));

    let mut root = Map::new();
    root.insert(
        "panels".to_string(),
        Value::Array(panels.into_iter().map(|(_, value)| value).collect()),
    );
    if let Some(assets) = &manifest.assets {
        root.insert("assets".to_string(), Value::String(assets.clone()));
    }
    if let Some(routes) = &manifest.routes {
        root.insert("routes".to_string(), routes_contract(routes));
    }

    // Navigation is not contract, but a navigation entry's module is. Entries
    // have no key, so each module is identified by the path its entry links
    // to, and only entries that carry one appear. They are ordered by their
    // canonical bytes, which begin with the path, so that even two entries
    // sharing a path order the same however they were written.
    let mut navigation: Vec<(String, Value)> = manifest
        .navigation
        .iter()
        .filter_map(|entry| {
            let mut object = Map::new();
            object.insert("path".to_string(), Value::String(entry.path.clone()));
            object.insert("ui".to_string(), module_contract(entry.ui.as_ref()?));
            let value = Value::Object(object);
            Some((canonicalize(&value), value))
        })
        .collect();
    if !navigation.is_empty() {
        navigation.sort_by(|left, right| left.0.cmp(&right.0));
        root.insert(
            "navigation".to_string(),
            Value::Array(navigation.into_iter().map(|(_, value)| value).collect()),
        );
    }

    Value::Object(root)
}

fn panel_contract(panel: &Panel) -> Value {
    let mut object = Map::new();
    object.insert("key".to_string(), Value::String(panel.key.clone()));
    if let Some(envelope) = &panel.envelope {
        object.insert("envelope".to_string(), Value::String(envelope.clone()));
    }
    if let Some(data) = &panel.data {
        object.insert("data".to_string(), Value::String(data.clone()));
    }
    if let Some(ui) = &panel.ui {
        object.insert("ui".to_string(), module_contract(ui));
    }
    object.insert("params".to_string(), Value::Array(canonical_params(panel)));
    object.insert("lifecycle".to_string(), lifecycle_contract(panel));
    Value::Object(object)
}

/// A module's contract: its entry and its bridge major. Unknown fields are
/// dropped, as everywhere else.
fn module_contract(ui: &ModuleUi) -> Value {
    let mut object = Map::new();
    object.insert("entry".to_string(), Value::String(ui.entry.clone()));
    object.insert("bridge".to_string(), Value::from(ui.bridge));
    Value::Object(object)
}

/// Route prefixes with both lists materialised, each sorted and without
/// repeats. The order prefixes are written in, and writing one twice, promise
/// nothing.
fn routes_contract(routes: &Routes) -> Value {
    let mut object = Map::new();
    for access in Access::ALL {
        let mut prefixes: Vec<&String> = routes.prefixes(access).iter().collect();
        prefixes.sort();
        prefixes.dedup();
        object.insert(
            access.as_str().to_string(),
            Value::Array(
                prefixes
                    .into_iter()
                    .map(|prefix| Value::String(prefix.clone()))
                    .collect(),
            ),
        );
    }
    Value::Object(object)
}

/// Parameter declarations in object form, ordered by their canonical
/// serialisation.
///
/// Declaration order carries no meaning: two manifests that declare the same
/// parameters in different orders promise the same thing. Sorting by the
/// canonical bytes of each entry gives a total order that does not depend on
/// how the entry happened to be written. The comparison is over the JCS output,
/// which is UTF-8, so a byte-wise ordering is well defined.
fn canonical_params(panel: &Panel) -> Vec<Value> {
    let mut entries: Vec<(String, Value)> = panel
        .params
        .iter()
        .map(|declaration| {
            let value = declaration.to_object();
            let ordering_key = canonicalize(&value);
            (ordering_key, value)
        })
        .collect();
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries.into_iter().map(|(_, value)| value).collect()
}

fn lifecycle_contract(panel: &Panel) -> Value {
    let lifecycle = &panel.lifecycle;
    let mut object = Map::new();
    let status = match lifecycle.status {
        LifecycleStatus::Active => "active",
        LifecycleStatus::Deprecated => "deprecated",
    };
    object.insert("status".to_string(), Value::String(status.to_string()));
    if let Some(sunset) = lifecycle.sunset {
        object.insert("sunset".to_string(), Value::String(sunset.to_string()));
    }
    if let Some(successor) = &lifecycle.successor {
        object.insert("successor".to_string(), Value::String(successor.clone()));
    }
    Value::Object(object)
}

/// Serialise a value to RFC 8785 canonical JSON.
///
/// Contract content is built from strings, booleans, small integers, arrays and
/// objects, all of which canonicalise without ambiguity. The fallible paths in
/// RFC 8785 concern numbers that cannot be represented, which contract content
/// does not contain, so this cannot fail in practice; if it somehow did, falling back to ordinary
/// serialisation would silently change the fingerprint, so the panic is the
/// honest outcome.
pub fn canonicalize(value: &Value) -> String {
    serde_jcs::to_string(value).expect("contract content is canonicalisable JSON")
}

/// The contract hash of a manifest.
pub fn contract_hash(manifest: &Manifest) -> ContractHash {
    let canonical = canonicalize(&contract_content(manifest));
    let digest = Sha256::digest(canonical.as_bytes());
    ContractHash(hex(&digest))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}
