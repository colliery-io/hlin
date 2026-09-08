---
id: composition-picker-drag-and-drop
level: task
title: "Composition: picker, drag-and-drop grid and layout API"
short_code: "HLIN-T-0015"
created_at: 2026-09-07T14:04:26.118328+00:00
updated_at: 2026-09-07T16:58:31.658500+00:00
parent: HLIN-I-0002
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/completed"


exit_criteria_met: false
initiative_id: HLIN-I-0002
---

# Composition: picker, drag-and-drop grid and layout API

## Parent Initiative

[[HLIN-I-0002]]

## Objective

Let a person compose the surface. A picker lists every panel the registry accepted; adding one puts it on a drag-and-drop, resizable grid; the arrangement persists through the store and survives a reload. This is the vision's central claim made operable, and by a wide margin the most expensive slice in the initiative.

## Acceptance Criteria

## Acceptance Criteria

- [x] Layout API on the shell: `GET /api/layouts` (the principal's own), `POST /api/layouts`, `GET`/`PUT`/`DELETE /api/layouts/{id}`, all scoped to the development principal as owner; `PUT` replaces the whole layout per the store's contract; a layout that is not the principal's is read-only, returning 403 on write. `POST /api/layouts/{id}/fork` added, because a 403 with no way forward is a dead end
- [x] `GET /api/panels`: the picker's source, built from the registry's current view: accepted panels only, grouped by platform, with key, title, description, default kind, envelope, and the kinds that accept that envelope. Also each panel's own controls, which the criterion did not ask for and the `select` requirement below needs
- [x] On first visit the development principal gets a default empty layout and is sent to it; the surface route from [[HLIN-T-0014]] gains an edit mode
- [x] A picker panel: browse by platform, search by title, add a panel, which appends a `PanelInstance` at the first free grid position and re-subscribes the stream
- [x] A 12-column grid: drag to move by a handle, resize from a corner, both with pointer events; live preview during the gesture; collisions push other panels down; positions stored as `{x, y, w, h}` in the instance's `position`
- [~] Per-instance controls: remove; rename (title override); switch kind among `Kind::accepting(envelope)`; set a `select` parameter's value, stored in `selections` and sent with the next parameter change. All four work; the `select` is a text box rather than a list of the platform's valid values, because reaching an options endpoint needs the shell to proxy it. See the status update
- [x] Every edit issues one `PUT` with the whole layout, debounced during a gesture so a drag does not write on every pointer move; the surface re-subscribes after the write lands
- [x] The grid algorithm (place, move, resize, collide, compact) lives in a pure module with no DOM and is unit-tested: overlap resolution, compaction upward, resize clamping to the grid, and that no two instances ever overlap after any sequence of operations
- [~] The whole flow in a browser: add three panels from two platforms, drag them into an arrangement, resize one, switch one to a sparkline, reload, everything is where it was — every part of this was driven over HTTP against a running shell and two platforms, and **none of it in a browser**. See the status update

## Implementation Notes

### Technical Approach
The grid is written to be replaced by a real design pack's grid one day; keep it in its own module in `hlin-ui` with a small interface (a list of instances with positions in, the same out) rather than threading it through the surface. Pointer capture on the handle is what makes drag reliable across the panel's own content. Compaction should be a pure function of the instance list so it is trivially testable.

### Dependencies
[[HLIN-T-0014]] for the surface; the store and layout types from [[HLIN-T-0007]]; the registry view from [[HLIN-T-0011]] for the picker.

### Risk Considerations
This is the slice most likely to overrun. If it does, the fallback that keeps the demo honest is: land the picker with add and remove and auto-flow first, and treat drag and resize as the second half. Do not let the grid delay the picker.

## Status Updates

### 2026-09-07 — plan, and two corrections owed to T-0014

Reading the code before writing any found two acceptance criteria on
[[HLIN-T-0014]] that were ticked and should not have been. Both are the
frontend half of the time picker: there is no custom absolute range (only the
four presets), and there is no "applied" indicator, because `stream.rs`
registers a `surface` event listener whose body discards the payload. Both are
in the path of this task, so both get built here, and the T-0014 record gets
corrected rather than quietly fixed.

Order of work, five stages:

1. **Wire types.** `hlin-stream` gains a `layout` module: `Placement`,
   `LayoutDocument`, `PanelInstanceDocument`, and the picker's catalogue. Ids
   travel as strings so the browser needs no `uuid` dependency, and a new panel
   arrives with no id for the shell to assign. `PanelReport` moves out of
   `server.rs` into this crate so the picker and `/api/platforms` read one
   definition.
2. **The shell.** `AppState` gains the store, which it did not have.
   `layouts.rs` holds the API. `surfaces.rs` learns to resolve a layout id into
   instances instead of only knowing `all`. A layout panel whose platform no
   longer offers it becomes a retired instance reporting
   `unavailable(unknown)`, which needs a `retired` flag on `Instance` so `due()`
   does not fetch an endpoint that is not there.
3. **The grid, pure.** `grid.rs` in `hlin-ui`'s library half: 12 columns,
   `first_free`, `settle` (push down, then float up, in `(y, x)` order with the
   moved panel winning ties). Unit-tested for the invariant that matters — no
   two panels overlap after any sequence.
4. **The draft, pure.** `layout.rs` wraps a `LayoutDocument` with add, remove,
   move, resize, rename, kind and selection, each settling the grid.
5. **The surface.** `app.rs` gains edit mode, the picker, pointer-event drag
   and resize, per-instance controls, the debounced whole-layout `PUT`, and a
   re-subscribe once the write lands.

The task's own fallback (picker first, grid second) is noted and not taken
unless stage 3 overruns.

### 2026-09-07 — done; the fallback was not needed

The grid did not overrun, so the picker and the drag-and-drop landed together.
What was built, and the decisions that are worth knowing about later.

**The shell had no store.** `AppState` carried the config, the registry, the
issuer and the running surfaces, and the registry kept the store to itself.
Composition needs it, so it was hoisted. That was the only structural change to
the shell; everything else went into `layouts.rs`.

**Two rules, in one place.** `layouts.rs` is where one-owner and
whole-layout-replacement are enforced, so there is a single file to read for
decision HLIN-A-0007. Two status codes there are deliberate and easy to get
backwards. A layout the principal can see but not write answers **403**, not
404: hiding it would make the fork path senseless. A personal layout belonging
to somebody else answers **404**: knowing the link is the permission, and this
principal does not have it.

**Identity is the store's to assign.** A panel arrives from the browser with no
id; the shell gives it one. An id the browser invented is not honoured, because
a browser that could name a panel could collide two of them. A panel already on
the layout keeps the id it had, so the open stream goes on naming the same
thing across a drag.

**A layout outlives its panels.** When a platform stops offering a panel that a
layout names, the panel does not vanish from the surface. It becomes a *retired*
instance: born `unavailable(unknown)`, never fetched, drawn in its place with
"this panel is no longer offered". That needed a `retired` flag on `Instance`
and one guard in `due()`, because an instance with no endpoint would otherwise
be asked for a URL its platform never promised, and a 404 would report the
panel as `malformed` rather than gone.

**A defect the live run found.** Stored selections were never applied. The
surface was built from the layout's panels but not from their choices, so
somebody who picked a cluster and reloaded got the platform's default with
nothing on screen saying why, until they touched a control. Fixed with
`Surface::restore_selections`, which sets them before anything is fetched and
deliberately does not move the generation: these are not a change anyone just
made. The browser now also applies its current range once the layout is known,
so the first fetch carries a window rather than none.

**The grid.** One rule rather than two: gravity. A panel floats up until
something stops it, and a panel dropped onto another pushes it down. `settle`
does push-down-then-float-up in `(y, x)` order with the moved panel winning
ties, which is what makes "drop it on top and the other gives way" and "remove
one and the gap closes" the same code path. Twelve tests, including a scripted
walk through ten moves and resizes that checks after every one that no two
panels overlap and nothing has run off the grid. Written to be replaced: a real
design pack will want its own grid, and the interface is a list of placements in
and the same list out.

**Debouncing.** The layout is written when a gesture ends, not while it is in
progress, so a drag across the screen is one `PUT` rather than one per pointer
move. `LayoutDraft` tracks whether it is dirty, so a gesture that changed
nothing writes nothing, and opening a surface does not write it straight back.

**Correcting T-0014.** Two things that task claimed were not there: the custom
absolute range and the "applied" indicator. Both are built here, and the T-0014
record now says so rather than being quietly fixed. The indicator was not merely
missing — the browser's `surface` event listener parsed nothing and dropped the
payload, so the shell's acknowledgements were being thrown away.

**One thing narrower than the criterion.** A panel's `select` parameter is a
text box carrying the manifest's label, not a list of the platform's valid
values. The value is stored in `selections` and sent with the next parameter
change, which is what the criterion asks for, but a person has to know what to
type. Offering the list means the shell proxying the platform's options endpoint
with the viewer's credential, since the browser cannot call a platform directly.
That is a small route and it is not built.

**What was verified, against a running system.** Postgres, the shell and two
sample platforms, everything driven over HTTP:

- The catalogue lists both platforms and carries the `select` control for the
  parameterised panel, with its label and options endpoint.
- A first visit creates a layout; a second visit returns the same one.
- A layout of five panels from two platforms was written, and every panel
  reached `ready`.
- The arrangement survived a shell restart, unchanged.
- Writing a layout dropped the surface that was running for it (`dropped=1`), so
  the next subscription rebuilt from what was stored.
- A layout naming a panel the platform does not offer produced
  `unavailable`/`unknown` with "this panel is no longer offered", while the
  panel beside it stayed `ready`.
- A stored cluster selection reached the *first* fetch: the same panel returned
  different data for `orebank-lab` than for `orebank-us-east`, and no selection
  fell back to the platform's default.
- `/s/{layout}` serves the frontend, and the address bar ends up naming the
  surface on screen so the link can be sent to somebody.

Checks clean, 259 tests passing.

**The gap, again.** No browser has rendered any of this. The Chrome extension is
not connected in this environment, so drag, resize, the picker, the corner
handle and the pointer-capture behaviour have never been exercised by a pointer.
The arithmetic underneath them is unit-tested and the API they drive is verified,
but "a person drags a panel and it goes where they meant" is not something an
HTTP request can establish. It carries into [[HLIN-T-0016]] as the first step of
the walkthrough.
