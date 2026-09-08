//! The layout being edited.
//!
//! Every composition gesture ends here: adding a panel, removing one, dragging
//! it, resizing it, renaming it, switching how it is drawn, choosing a value
//! for one of its own parameters. Each is an ordinary method on ordinary data,
//! so the whole of composition can be exercised without a browser.
//!
//! The draft also knows whether it has changed since it was last written,
//! which is what lets the surface write once at the end of a gesture instead of
//! once per pointer move.

use std::collections::BTreeMap;

use hlin_stream::layout::{LayoutDocument, PanelInstanceDocument, Placement};

use crate::grid;

/// A layout, and whether it needs writing.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutDraft {
    document: LayoutDocument,
    dirty: bool,
}

impl LayoutDraft {
    /// A draft of a layout as the shell last described it. Clean, because
    /// nothing has been changed yet.
    pub fn of(document: LayoutDocument) -> Self {
        let mut draft = Self {
            document,
            dirty: false,
        };
        // A layout read back from the store could have been written by an older
        // shell, or by hand. Settling it on arrival means the rest of the code
        // never has to cope with an arrangement that overlaps itself.
        draft.arrange(None);
        draft.dirty = false;
        draft
    }

    /// A draft of nothing, for the moment before the layout has loaded.
    pub fn empty() -> Self {
        Self::of(LayoutDocument::empty("…"))
    }

    /// The layout, as it would be written.
    pub fn document(&self) -> &LayoutDocument {
        &self.document
    }

    /// The panels on it.
    pub fn panels(&self) -> &[PanelInstanceDocument] {
        &self.document.panels
    }

    /// What it is called.
    pub fn title(&self) -> &str {
        &self.document.title
    }

    /// Whether the principal looking at it may change it.
    pub fn editable(&self) -> bool {
        self.document.editable
    }

    /// The surface to subscribe to.
    pub fn surface_id(&self) -> Option<&str> {
        self.document.surface_id()
    }

    /// Whether there are changes the shell has not been told about.
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// The shell has accepted a write, and answered with what it stored.
    ///
    /// The shell's answer replaces the draft rather than merging into it,
    /// because the shell assigns panel identities and is the authority on what
    /// the layout now is.
    pub fn written(&mut self, stored: LayoutDocument) {
        self.document = stored;
        self.arrange(None);
        self.dirty = false;
    }

    /// How many rows the arrangement occupies, so the surface knows how tall
    /// to be.
    pub fn depth(&self) -> u32 {
        grid::depth(&self.placements())
    }

    /// Where a panel sits, by its instance id.
    pub fn placement_of(&self, instance: &str) -> Option<Placement> {
        self.index_of(instance)
            .map(|index| self.document.panels[index].position)
    }

    /// The panel with this instance id.
    pub fn panel(&self, instance: &str) -> Option<&PanelInstanceDocument> {
        self.index_of(instance)
            .map(|index| &self.document.panels[index])
    }

    // -- Composition ------------------------------------------------------

    /// Put a platform's panel on the surface, at the first free position.
    ///
    /// Returns nothing, because the panel has no identity until the shell has
    /// written it: the browser cannot name a thing it has just invented.
    pub fn add(&mut self, platform_id: &str, panel_key: &str) {
        let taken = self.placements();
        let (w, h) = grid::DEFAULT_SIZE;
        let position = grid::first_free(&taken, w, h);

        self.document
            .panels
            .push(PanelInstanceDocument::new(platform_id, panel_key, position));
        self.arrange(None);
        self.dirty = true;
    }

    /// Take a panel off the surface.
    ///
    /// What is left settles upwards, so removing a panel from the top does not
    /// leave a band of nothing where it was.
    pub fn remove(&mut self, instance: &str) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        self.document.panels.remove(index);
        self.arrange(None);
        self.dirty = true;
    }

    /// Move a panel to a column and row.
    pub fn move_to(&mut self, instance: &str, x: u32, y: u32) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        let mut placements = self.placements();
        grid::move_to(&mut placements, index, x, y);
        self.write_back(placements);
        self.dirty = true;
    }

    /// Resize a panel.
    pub fn resize(&mut self, instance: &str, w: u32, h: u32) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        let mut placements = self.placements();
        grid::resize(&mut placements, index, w, h);
        self.write_back(placements);
        self.dirty = true;
    }

    /// Give a panel a title of the viewer's own.
    ///
    /// An empty title is not a title: it clears the override, so the panel goes
    /// back to whatever its platform calls it. That is what a person emptying
    /// the box means, and it keeps a blank heading off the surface.
    pub fn rename(&mut self, instance: &str, title: &str) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        let trimmed = title.trim();
        self.document.panels[index].title_override = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        self.dirty = true;
    }

    /// Draw a panel a different way.
    ///
    /// `None` means "however its platform draws it", so a platform changing its
    /// default reaches everyone who did not choose otherwise.
    pub fn set_kind(&mut self, instance: &str, kind: Option<&str>) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        self.document.panels[index].kind_override = kind.map(str::to_string);
        self.dirty = true;
    }

    /// Choose a value for one of a panel's own parameters.
    ///
    /// Stored on the instance, and sent with the next parameter change, which
    /// is what makes a `select` on one panel independent of the same `select`
    /// on another.
    pub fn set_selection(&mut self, instance: &str, param: &str, values: Vec<String>) {
        let Some(index) = self.index_of(instance) else {
            return;
        };
        if values.is_empty() {
            self.document.panels[index].selections.remove(param);
        } else {
            self.document.panels[index]
                .selections
                .insert(param.to_string(), values);
        }
        self.dirty = true;
    }

    /// Rename the layout itself.
    pub fn retitle(&mut self, title: &str) {
        let trimmed = title.trim();
        if trimmed.is_empty() || trimmed == self.document.title {
            return;
        }
        self.document.title = trimmed.to_string();
        self.dirty = true;
    }

    /// Every panel's selections, keyed by instance, as the stream wants them.
    ///
    /// Panels the shell has not yet given an identity are skipped: there is
    /// nothing to name them by, and the shell will send their first frames once
    /// the layout has been written.
    pub fn selections(&self) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
        self.document
            .panels
            .iter()
            .filter_map(|panel| {
                let id = panel.id.clone()?;
                if panel.selections.is_empty() {
                    return None;
                }
                Some((id, panel.selections.clone()))
            })
            .collect()
    }

    // -- Inside -----------------------------------------------------------

    fn index_of(&self, instance: &str) -> Option<usize> {
        self.document
            .panels
            .iter()
            .position(|panel| panel.id.as_deref() == Some(instance))
    }

    fn placements(&self) -> Vec<Placement> {
        self.document
            .panels
            .iter()
            .map(|panel| panel.position)
            .collect()
    }

    fn write_back(&mut self, placements: Vec<Placement>) {
        for (panel, placement) in self.document.panels.iter_mut().zip(placements) {
            panel.position = placement;
        }
    }

    fn arrange(&mut self, priority: Option<usize>) {
        let mut placements = self.placements();
        grid::settle(&mut placements, priority);
        self.write_back(placements);
        self.dirty = true;
    }
}

impl Default for LayoutDraft {
    fn default() -> Self {
        Self::empty()
    }
}
