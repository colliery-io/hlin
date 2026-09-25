---
id: open-a-platform-s-page-inside-the
level: task
title: "Open a platform's page inside the shell"
short_code: "HLIN-T-0070"
created_at: 2026-09-25T00:01:09.187717+00:00
updated_at: 2026-09-25T02:41:00.000000+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#task"
  - "#phase/active"


exit_criteria_met: true
initiative_id: HLIN-I-0011
---

# Open a platform's page inside the shell

## Parent Initiative

[[HLIN-I-0011]]

## Objective

Slice 10 of [[HLIN-I-0011]]. [[HLIN-S-0007]] *Pages*: a navigation entry that
declares `ui` opens at full width in the same kind of frame, with
`init.page` set.

## Acceptance Criteria

## Acceptance Criteria

- [x] Navigation entries with `ui` appear in the shell's navigation and open a
      full-width frame on a route of the shell's own (so a page has a URL a
      person can share)
- [x] Same frame rules, bridge, states and fallback-free behaviour as panels
      (a page has no shell-drawn fallback; it is `unavailable` and says so)
- [x] `navigate` from any module can open a page
- [x] Entries without `ui` keep behaving as today
- [x] Browser test: open a page from navigation and from `navigate`
- [x] `angreal check all`, `angreal test all`, `angreal e2e test` pass

## Implementation Notes

- Depends on [[HLIN-T-0066]] and [[HLIN-T-0067]].
- `crates/hlin-ui/src/app.rs` already reads navigation.

## Status Updates

### 2026-09-24

Created when [[HLIN-I-0011]] was decomposed. Not started.

### 2026-09-25 (implemented)

**What.**

- *Catalogue.* `CatalogPlatform.navigation` (`CatalogNavigation`: label, path,
  icon, weight, `ui`), from `PlatformView::accepted_navigation`, which drops
  the module of an entry validation rejected and keeps the entry.
- *Address.* `/page/{platform}/{path}`, each segment percent-encoded
  (`hlin-ui/src/route.rs`, host-tested, with `pages`/`page_entry`: only
  entries with a module, a platform's together, by weight then label). The
  shell's frontend fallback already answers it with `index.html` and the page
  CSP; a test in `tests/assets.rs` holds that.
- *Navigation.* A row under the bar (`nav.pages`): "Surface", then each
  platform's name and its pages, as links to their addresses (a plain click is
  handled in the page; modified clicks go to the browser). `pushState` on
  open and on going back; `popstate` reopens whatever the address names. On a
  page address the home layout is still loaded behind it, without rewriting
  the address to `/s/{id}`.
- *Page.* `PageView` in `app.rs`: a `Mount` with `page: true`,
  `declares_data: false`, the entry's path as `panel`, a fresh
  `page-{random}` instance per opening. Same markup attributes as a panel
  (`data-state`, `data-module`, `data-module-cause`), the notice, and
  `ModuleUnavailable` (no fallback) with its retry. An address naming no page
  (unknown platform, unknown path, or an entry without a module) is
  `unavailable (unknown)` once the catalogue is known, with nothing mounted.
- *Frame host.* `init.page` from the mount; `restored` never for a page; page
  frames excluded from the budget's count and choice.
- *`navigate`.* A `page` target opens the page if the catalogue offers it as
  one, else it is ignored and logged. A panel target from a page goes back to
  the surface and brings the panel into view on the next turn.
  `app.rs::navigate` split into `panel_to_open` and `bring_into_view`.
- *Sample platform.* Navigation gains `module-page` (`ui/page/`, a hand-written
  page module showing `init.page`, its path, the viewer and a `whoami`, with a
  button that navigates back to `module-probe`) and `silent-page` (the silent
  module). `Overview` stays a plain link. The probe gains `#to-page` and
  `#to-link`.
- *Browser tests.* `e2e/tests/pages.spec.js`, six: only entries with a module
  in the navigation; open from it (sandbox attributes, full width,
  `init.page`, a `whoami` through the proxy, back via "Surface" and forward
  via history); load and reload the address; open from the probe's
  `navigate` and back to the panel from the page's; the silent page is
  `unavailable (unreachable)` with no frame and a retry that remounts it; the
  plain link ignored and logged from `navigate`, and `unknown` by address.

**Decisions.**

- *`/page/` rather than `/p-page/`.* Axum matches whole segments, so it
  cannot collide with `/p/` or `/m/`, and it reads as what it is. Recorded in
  HLIN-S-0007 *Frames, Pages*.
- *Entries without a module are not drawn.* The shell never drew navigation
  before, and it has no public address for a platform's own frontend to link
  to (`base_url` is where the shell reaches it, not where a person does). The
  catalogue still lists them, so a later task can link out.
- *The surface is hidden, not torn down,* while a page is open, so its
  modules keep their state; hidden panels count as out of view, as they are.
- *Composition is hidden while a page is open*, and opening a page leaves
  composing.
- *Pages are exempt from the budget*, per the spec's note on HLIN-T-0067.

**Checks.** `angreal check all` clean; `angreal test all` 723 passed, 0
failed, 3 ignored; `angreal ui build` ok. Against `demo up --with aurora`,
`angreal e2e test`: 45 passed, 15 skipped, 0 failed (`pages.spec.js` 6 of
them). Against `--with collab`, `angreal e2e signin`: 9 passed.
