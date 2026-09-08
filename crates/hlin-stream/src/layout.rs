//! What the shell and the browser say to each other about layouts.
//!
//! The store's own [`Layout`](../../hlin/store/types/struct.Layout.html) type
//! lives in the shell, which does not build for `wasm32`. These are the shapes
//! that cross the wire, and the shell converts at its edge.
//!
//! Two deliberate differences from the stored shape. Identifiers travel as
//! strings, so the browser needs no `uuid` dependency to read a layout it will
//! only ever hand back. And `id` is optional on a panel instance: a panel the
//! viewer has just added has no identity yet, and the shell assigns one when
//! the layout is written. That keeps identifier generation on the side that
//! owns the store, which is where an identifier means something.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How many columns a surface is divided into.
///
/// Twelve, which is the usual choice for the usual reason: it divides by two,
/// three, four and six, so halves, thirds and quarters are all expressible
/// without fractions.
pub const COLUMNS: u32 = 12;

/// Where a panel sits on the grid.
///
/// Column and row units, not pixels. The browser multiplies by whatever a cell
/// currently measures, so the same layout is the same arrangement on any
/// screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    /// Its leftmost column, counting from zero.
    pub x: u32,
    /// Its topmost row, counting from zero.
    pub y: u32,
    /// How many columns wide.
    pub w: u32,
    /// How many rows tall.
    pub h: u32,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        }
    }
}

impl Placement {
    /// A placement, with every field forced into the grid.
    ///
    /// Nothing downstream should have to wonder whether a placement is
    /// sensible, so every entry point runs through here: a width of zero
    /// becomes one, a width past the edge is trimmed, and an `x` that would
    /// push the panel off the right is pulled back.
    pub fn clamped(self) -> Self {
        let w = self.w.clamp(1, COLUMNS);
        let h = self.h.max(1);
        let x = self.x.min(COLUMNS - w);
        Self { x, y: self.y, w, h }
    }

    /// One past its rightmost column.
    pub fn right(&self) -> u32 {
        self.x + self.w
    }

    /// One past its bottom row.
    pub fn bottom(&self) -> u32 {
        self.y + self.h
    }

    /// Whether these two cover any cell in common.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

/// A surface someone composed, as it travels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutDocument {
    /// Its identifier. Absent only on a layout that has never been stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// What the viewer called it.
    pub title: String,

    /// `personal` or `published`.
    pub visibility: String,

    /// The principal who owns it.
    #[serde(default)]
    pub owner: String,

    /// Whether the principal asking may change it.
    ///
    /// Computed by the shell rather than inferred by the browser, because the
    /// answer is the shell's to give and a browser that guessed would offer an
    /// edit that fails (decision HLIN-A-0007).
    #[serde(default)]
    pub editable: bool,

    /// The panels on it.
    #[serde(default)]
    pub panels: Vec<PanelInstanceDocument>,
}

impl LayoutDocument {
    /// An empty layout with this title, not yet stored.
    pub fn empty(title: impl Into<String>) -> Self {
        Self {
            id: None,
            title: title.into(),
            visibility: "personal".to_string(),
            owner: String::new(),
            editable: true,
            panels: Vec::new(),
        }
    }

    /// The surface id to subscribe to, which is the layout's own id.
    pub fn surface_id(&self) -> Option<&str> {
        self.id.as_deref()
    }
}

/// One panel on one surface, as it travels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelInstanceDocument {
    /// Its identifier within the surface, which the stream names in every
    /// frame. Absent on a panel the viewer has just added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// The platform the panel comes from.
    pub platform_id: String,

    /// The panel's key in that platform's manifest.
    pub panel_key: String,

    /// The kind the viewer chose, where it differs from the manifest default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind_override: Option<String>,

    /// The title the viewer chose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_override: Option<String>,

    /// Values the viewer chose for this instance's own parameters, by
    /// parameter id.
    #[serde(default)]
    pub selections: BTreeMap<String, Vec<String>>,

    /// Where it sits.
    #[serde(default)]
    pub position: Placement,
}

impl PanelInstanceDocument {
    /// A new instance of a catalogue panel, at this placement.
    pub fn new(
        platform_id: impl Into<String>,
        panel_key: impl Into<String>,
        position: Placement,
    ) -> Self {
        Self {
            id: None,
            platform_id: platform_id.into(),
            panel_key: panel_key.into(),
            kind_override: None,
            title_override: None,
            selections: BTreeMap::new(),
            position: position.clamped(),
        }
    }

    /// The manifest reference this points at, as `platform/key`.
    pub fn reference(&self) -> String {
        format!("{}/{}", self.platform_id, self.panel_key)
    }
}

/// Enough of a layout to list it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSummary {
    /// Its identifier.
    pub id: String,
    /// What the viewer called it.
    pub title: String,
    /// `personal` or `published`.
    pub visibility: String,
    /// Who owns it.
    pub owner: String,
    /// How many panels are on it.
    pub panel_count: usize,
}

// -- The picker's source ---------------------------------------------------

/// One platform's offering, as the picker needs it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogPlatform {
    /// Its id, as shell configuration names it.
    pub id: String,
    /// Its display name, where a manifest has been read.
    pub name: Option<String>,
    /// Whether it answered the last time it was asked.
    ///
    /// An unreachable platform's panels are still listed, because the layout a
    /// person is composing outlives one bad poll, and a panel that vanished
    /// from the picker while its platform restarted would be worse than one
    /// that is briefly unavailable.
    pub reachable: bool,
    /// The panels a viewer may use.
    pub panels: Vec<CatalogPanel>,
}

/// One panel a platform offers.
///
/// Shared by the picker and by `/api/platforms`, so an operator's view of a
/// panel and a composer's view of it cannot drift apart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogPanel {
    /// Its key within the platform.
    pub key: String,
    /// The reference a layout stores, as `platform/key`.
    pub reference: String,
    /// Its default title.
    pub title: String,
    /// What it is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The platform's default rendering.
    pub kind: String,
    /// How often this panel's platform says its data is worth refetching.
    ///
    /// Absent for the many panels content with the shell's own interval.
    /// Carried because a caller cannot otherwise tell how live a panel is: the
    /// shell's configured cadence is a default and a floor, not what any
    /// particular panel gets (decision HLIN-A-0009).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_ms: Option<u64>,

    /// A design-system component the platform asked for by name.
    ///
    /// Never interpreted by the shell or the frontend: it is forwarded to
    /// whichever design pack is mounted, which draws it or declines. Absent for
    /// the overwhelming majority of panels, which want the vocabulary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,

    /// That this panel's platform reports when its data changes.
    ///
    /// Carried for the same reason `refresh_ms` is: how live a panel is cannot
    /// be read off the shell's own settings, and this is now half the answer
    /// (specification HLIN-S-0006). A pushed panel is polled at a relaxed
    /// interval and refetched on notice, so its cadence hint stops describing
    /// how often it actually arrives.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pushed: bool,

    /// What its data endpoint returns.
    pub envelope: String,
    /// The controls it responds to, by vocabulary name.
    #[serde(default)]
    pub params: Vec<String>,
    /// The controls a viewer sets on this panel alone, with enough of each
    /// declaration to draw it.
    ///
    /// Shell-level controls, such as the time range, are not here: they are
    /// driven by one picker for the whole surface and are not a per-panel
    /// choice.
    #[serde(default)]
    pub controls: Vec<PanelControl>,
    /// Every kind that can draw this envelope, so a viewer can switch.
    #[serde(default)]
    pub available_kinds: Vec<String>,
    /// Whether it is on its way out.
    #[serde(default)]
    pub deprecated: bool,
    /// What replaces it, where the platform said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor: Option<String>,
}

/// One control a viewer sets on one panel.
///
/// Taken from the panel's parameter declaration in the manifest, which is where
/// the platform said what it responds to and what to call it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelControl {
    /// The vocabulary name, such as `select`.
    pub param: String,
    /// The query key the chosen value is sent under, and the key it is stored
    /// against in the instance's selections.
    pub id: String,
    /// What to call it on screen.
    pub label: String,
    /// Where the platform lists the values it will accept, where it said.
    ///
    /// Not yet used: reaching it means the shell fetching on the viewer's
    /// behalf, because the browser cannot call a platform directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<String>,
}

/// What the browser knows about this shell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientConfig {
    /// The stream protocol the shell speaks.
    pub protocol_version: u32,
    /// How long a lost stream is merely stale before it is unreachable.
    ///
    /// Derived from the shell's own staleness setting rather than fixed, so a
    /// person watching a shell tuned for live data does not see one clock for a
    /// slow platform and a quite different one for a missing shell, with
    /// nothing on screen explaining the difference.
    pub stream_loss_grace_seconds: i64,

    /// How often the shell refetches a panel on a watched surface.
    ///
    /// Reported so the browser — and anything testing it — can tell what
    /// cadence this shell is configured for rather than assuming one. A suite
    /// that asserts a panel redraws several times a second is asserting
    /// something only some configurations can deliver, and without this it has
    /// no way to know which it is talking to.
    #[serde(default)]
    pub refresh_ms: u64,

    /// Who the browser is, as far as this shell is concerned.
    pub principal: PrincipalSummary,
}

/// The viewer, named.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrincipalSummary {
    /// The identifier platforms are told.
    pub sub: String,
    /// What to call them on screen.
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placement_is_pulled_back_inside_the_grid() {
        let hanging = Placement {
            x: 10,
            y: 0,
            w: 8,
            h: 2,
        }
        .clamped();
        assert_eq!(hanging.w, 8, "a width that fits is kept");
        assert_eq!(
            hanging.x, 4,
            "and the panel is pulled left until it fits, rather than narrowed"
        );

        let too_wide = Placement {
            x: 3,
            y: 0,
            w: 20,
            h: 2,
        }
        .clamped();
        assert_eq!(
            (too_wide.x, too_wide.w),
            (0, COLUMNS),
            "a width past the whole grid is the whole grid, and there is one place it can sit"
        );

        let zero = Placement {
            x: 3,
            y: 1,
            w: 0,
            h: 0,
        }
        .clamped();
        assert_eq!((zero.w, zero.h), (1, 1), "nothing is allowed to be nothing");
    }

    #[test]
    fn touching_is_not_overlapping() {
        let left = Placement {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let right = Placement {
            x: 4,
            y: 0,
            w: 4,
            h: 4,
        };
        assert!(!left.overlaps(&right), "sharing an edge is sharing no cell");

        let below = Placement {
            x: 0,
            y: 4,
            w: 4,
            h: 4,
        };
        assert!(!left.overlaps(&below));

        let straddling = Placement {
            x: 3,
            y: 3,
            w: 4,
            h: 4,
        };
        assert!(left.overlaps(&straddling));
    }

    #[test]
    fn a_new_panel_carries_no_identity() {
        let panel = PanelInstanceDocument::new("orders", "queue-depth", Placement::default());
        assert!(
            panel.id.is_none(),
            "identity is the store's to assign, not the browser's"
        );
        assert_eq!(panel.reference(), "orders/queue-depth");
    }
}
