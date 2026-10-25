---
title: Surface JSON
description: The items a surface holds, and the pages a content component shows.
---

A surface is JSON with a `title` and a list of `items` (decision HLIN-A-0015).
The shell stores surfaces in this form, `/api/layouts` takes and gives it, and
the **Surface as JSON** box and `default_surface` use it.

```json
{ "title": "Operations", "items": [ … ] }
```

## The grid

Each item has a `position` on a grid of 12 columns. `x` and `y` are the column
and the row of its top left cell. `w` and `h` are its width and height in
cells. Items move up until something stops them, and two items never share a
cell. On a narrow screen, the grid wraps.

```json
"position": { "x": 0, "y": 1, "w": 6, "h": 4 }
```

## Items

Each item has a `type`. The shell refuses an item of a type that it does not
know, and two items with the same `id`.

### `panel`

A platform's panel.

| Field | Type | Notes |
|---|---|---|
| `id` | string | The shell gives it. Leave it out on a new panel |
| `platform_id` | string | The platform |
| `panel_key` | string | The panel, as the platform's manifest names it |
| `kind_override` | string | Optional. How to draw it, if not the platform's way |
| `title_override` | string | Optional. A name of your own |
| `selections` | object | Optional. Values for the panel's own parameters |
| `position` | object | Where it is |

### `bar`

The bar: the name, the time picker, whether the surface is live and saved,
Edit, and who is signed in.

| Field | Type | Notes |
|---|---|---|
| `id` | string | Required in a shell file |
| `position` | object | Where it is |

The bar of the shell at `/` has the time picker and Edit. A bar on any other
surface has neither: the outermost time picker sets the time range for every
surface on the screen.

### `page-selector`

A list of pages in groups. A person chooses a page, and the content component
`target` shows it.

| Field | Type | Notes |
|---|---|---|
| `id` | string | Required in a shell file |
| `style` | `"tabs"` or `"rail"` | Optional. `tabs` if not given |
| `target` | string | The `id` of a content component on the same surface |
| `groups` | list | Each `{ "label": "…", "pages": [ … ] }`. One level only |
| `position` | object | Where it is |

Each page in a group is a [page](#pages) with an optional `label`. Without a
`label`, a page is called what its target calls itself: a surface's title, the
platform's name for its page, or a view's id.

```json
{ "surface": "7f3c…", "label": "Team" }
```

### `content`

One page, in its cells.

| Field | Type | Notes |
|---|---|---|
| `id` | string | A page selector names it as `target`. Required in a shell file |
| `initial` | page | Optional. The page it shows before a person chooses one |
| `position` | object | Where it is |

## Pages

| Page | Address | Shows |
|---|---|---|
| `{ "surface": "<id>" }` | `/s/<id>` | A surface, with its own panels and items |
| `{ "page": { "platform": "<id>", "path": "<path>" } }` | `/page/<platform>/<path>` | A platform's page, drawn by its module |
| `{ "view": "<id>" }` | `/v/<id>` | A view that the front end registers in code |

A front end registers views with `App`'s `views` property:

```rust
let views = hlin_ui::Views::new().with("about", || view! { <p>"About"</p> });
view! { <App pack=MyPack views=views /> }
```

## What a content component cannot show

A content component that cannot show its page shows a placeholder with the
reason. The page stays in the surface. The placeholder has `data-cause`:

| `data-cause` | Why |
|---|---|
| `nothing-chosen` | No page is chosen, and no `initial` is given |
| `too-deep` | The surface would be more than two levels below the outermost |
| `cycle` | The surface is already open around this one |
| `no-such-page` | No platform offers this page with a module |
| `no-such-view` | The front end does not register this view |
| `unreachable` | The surface cannot be fetched: it is not there, or not yours to see |

## Who may see a surface

| `visibility` | Who sees it |
|---|---|
| `personal` | Its owner |
| `published` | Everyone who is signed in. Anyone signed in may publish |
| `public` | Everyone, anonymous viewers too. Only an admin sets it |
