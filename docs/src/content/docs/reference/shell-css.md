---
title: Shell class names
description: The class names and attributes an operator's stylesheet may style.
---

Hlin draws the shell's own chrome: the bar, the page selectors, the content
components, and the frames around panels. A design pack draws what is in a
panel. To change how the chrome looks, write a stylesheet that selects these
names (decision HLIN-A-0015).

These names and attributes are a contract. They do not change within a major
version. Other names in the page can change at any time.

## Put your stylesheet on the page

Name it in `[brand]` in `hlin.toml`. The shell puts it on the page after the
pack's stylesheet, so your rules win.

```toml
[brand]
stylesheet = "/etc/hlin-brand/brand.css"
```

```css
.page-selector[data-style="rail"] .selector-page[aria-current="page"] {
  background: #003d5b;
  color: white;
}
```

## The bar

| Name | Is |
|---|---|
| `header.bar` | The bar |
| `.brand`, `.brand-logo`, `.brand-name` | The name and logo |
| `.tagline` | The title of the surface it speaks for |
| `.picker` | The time picker. `button.active` is the range in force |
| `.custom` | The boxes for a range of your own |
| `.applied`, `.applied.pending` | Whether the shell has taken the last change |
| `.saving` | A write is in flight |
| `.link`, `.link.down` | The stream: live, or reconnecting |
| `button.mode`, `button.mode.active` | Edit, and Done while composing |
| `.viewer` | Who is signed in |
| `.sign-out` | Sign out |
| `.extra` | The front end's own controls |

## A surface

| Name | Is |
|---|---|
| `.stage`, `.stage.composing` | A surface, and the same while composing |
| `.catalog` | The panels to add, while composing |
| `.grid` | The grid of items |
| `.empty` | Said when a surface holds nothing |
| `section.panel` | A panel. `data-state`: `loading`, `ready`, `stale`, `unavailable` |
| `section.item` | A shell component. `data-type`: `bar`, `page-selector`, `content`. `data-item`: its id |
| `.trouble` | What went wrong, under the bar |

## A page selector

| Name | Is |
|---|---|
| `nav.page-selector` | A page selector. `data-style`: `tabs` or `rail`. `data-target`: its content component |
| `.selector-group` | One group |
| `.selector-group-label` | A group's name |
| `a.selector-page` | One page. `aria-current="page"` on the page shown. `data-address`: its address |

While its surface is composed, a page selector is `.selector-editor`, with
`.group-edit`, `.group-label`, `.page-edit`, `.page-label`, `.add-page`,
`.add-group` and `.new-group`.

## A content component and a page

| Name | Is |
|---|---|
| `.content-missing` | A placeholder. `data-cause` says why (see [Surface JSON](../surface-json/)) |
| `section.page` | A platform's page. `data-page`: `platform/path` |
| `.page-head`, `.page-platform` | A page's heading, and its platform |
| `a.page-back` | The way back from a page at full width |

## Surface as JSON

| Name | Is |
|---|---|
| `.surface-json` | The section in the catalogue |
| `.json-text` | The text |
| `.json-show`, `.json-apply`, `.json-download`, `.json-load` | Its controls |
| `.json-refused` | Why the text was refused |
