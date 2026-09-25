---
id: module-bridge
level: specification
title: "Module Bridge"
short_code: "HLIN-S-0007"
created_at: 2026-09-24T23:21:07.381422+00:00
updated_at: 2026-09-24T23:21:07.381422+00:00
parent: HLIN-I-0011
blocked_by: []
archived: false

tags:
  - "#specification"
  - "#phase/discovery"


exit_criteria_met: false
initiative_id: NULL
---

# Module Bridge

## Overview

A platform ships its own UI as a module, and the shell runs every module in a
sandboxed frame ([[HLIN-A-0014]]). This specification is the contract between
the two: how a frame is made, what it may load, every message that crosses
between a module and the shell's page, how a module's requests reach its
platform ([[HLIN-A-0013]]), and what the shell shows when a module misbehaves.

It is a public API in the same sense as the manifest. A dozen platforms will
build against it on their own schedules, through the SDK or by hand, and the
shell must be able to tell a module that speaks it from one that does not.

The property everything here protects is the one the decision rests on: **a
module holds nothing and reaches nothing.** It has no credential, no cookie,
no network, and no view of the shell's page. Everything it gets, it asks for
over the bridge, and the shell's page decides who is asking from which frame
sent the message, which a module cannot forge.

## System Context

### Actors
- **Module**: a platform's UI, running in a sandboxed frame with an opaque
  origin. Usually a Leptos application built with the SDK.
- **Shell page**: the shell's own frontend (`hlin-ui`). Creates frames, owns
  the parent end of the bridge, draws the frame and state around each panel.
- **Shell**: serves module assets (`/m/`), carries module requests (`/p/`),
  relays platform events.
- **Platform**: serves its manifest, its module's assets, and the endpoints
  its module calls. Authorizes every request from the identity the shell
  binds to it.

### Boundaries
Inside: frame creation and sandbox policy, asset serving and the module's CSP,
the message envelope and every message, the request proxy, timeouts and
heartbeat, the mapping of failures onto panel states, fallback, and
versioning. Outside: the manifest fields themselves (amended in
[[HLIN-S-0001]]), the token's claims (amended in [[HLIN-S-0004]]), the SDK's
API, and anything a module does inside its own frame.

## Requirements

### Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| REQ-1.1 | Every module runs in an `iframe` sandboxed with `allow-scripts allow-forms` and never `allow-same-origin`, `allow-top-navigation*`, `allow-popups` or `allow-modals` | An opaque origin is what keeps a module away from the shell's page, cookies and storage |
| REQ-1.2 | Module assets are served only from the shell's origin under `/m/{platform}/`, fetched from the platform's declared asset prefix | A platform never serves code directly into the page, and the shell controls the headers it arrives with |
| REQ-1.3 | Every module document is served with the module CSP in this specification, and the shell page's own CSP confines frames to `/m/` | The module cannot reach the network; the frame cannot be navigated anywhere the shell did not serve |
| REQ-2.1 | The shell page accepts a message only when `event.source` is a frame it created and has not torn down, and attributes it to that frame's platform, panel and instance | Identity rests on something a module cannot forge |
| REQ-2.2 | Nothing the shell page sends a module is secret beyond what that module's platform could tell it anyway | Messages to an opaque origin must use target origin `*` |
| REQ-2.3 | Unknown message types and unknown fields are ignored in both directions | Additive evolution without coordinated releases |
| REQ-3.1 | A module's `fetch` is carried to `/p/{platform}/{path}` for the platform owning the frame, never one the message names | A module can reach only its own platform |
| REQ-3.2 | The request proxy refuses any path outside the platform's declared prefixes, and any method outside the rules below | The shell calls only what a platform declared |
| REQ-3.3 | The request proxy accepts only same-origin requests from the shell page | A module, or anything else, cannot call it directly |
| REQ-3.4 | Writes require `Author`, carry an `Idempotency-Key`, carry a request-bound token, and are never retried by the shell | [[HLIN-A-0013]] |
| REQ-3.5 | Platform answers are passed back to the module unchanged, within limits. Refusals the shell makes itself are marked as the shell's | The module shows its platform's words; the shell never shows them as its own |
| REQ-3.6 | A streamed response flows only as fast as the module grants credit, and open streams are capped per frame and per page | Nothing buffers without bound, and module streams cannot starve the shell's own stream of connections |
| REQ-4.1 | Every mounted panel is in exactly one of `loading`, `ready`, `stale`, `unavailable (cause)` at all times, whatever the module does | Rendering stays total |
| REQ-4.2 | A panel declaring both a module and data falls back to shell-drawn data when its module is unavailable for a reason other than `unknown` or `deprecated` | The fallback is the reason to declare both |
| REQ-4.3 | A frame in view is never unmounted to meet the budget, and a frame unmounted out of view is offered `suspend` first | A person never loses what they are looking at, and a module can keep what it held |
| REQ-5.1 | The bridge has a major and a minor version. A module declares its major in the manifest and repeats it in `ready`; a mismatch is `unavailable (malformed)` | The shell must be able to tell a module that speaks the bridge from one that does not |

### Non-Functional Requirements

| ID | Requirement | Rationale |
|----|-------------|-----------|
| NFR-1.1 | A cached module mounts and says `ready` within 1 second on a desktop browser; a surface of six modules from three platforms is interactive within 3 seconds cold | Weight is the first risk in the new bet; it needs a number to be watched against |
| NFR-1.2 | A module built with the SDK needs no bridge code of its own | Twelve teams implement this; the SDK is how they get it right |
| NFR-1.3 | Containment is proven by browser tests, not argued | Every isolation property here fails silently if it fails |

## Frames

The shell page creates one frame per mounted panel instance, and one per open
page:

```html
<iframe
  src="/m/checklist/ui/items/index.html#i=7f3c…"
  sandbox="allow-scripts allow-forms"
  allow=""
  referrerpolicy="no-referrer"
  title="To do — Checklist"
  loading="lazy">
</iframe>
```

- `allow=""` grants no permissions-policy features: no camera, microphone,
  geolocation, clipboard or fullscreen.
- `title` is the panel's title and platform, for assistive technology.
- The fragment carries the instance id, so a module can log which instance it
  is before `init` arrives. It is not a secret and nothing relies on it.
- `allow-forms` lets a module use native form validation and submit events.
  Native submission goes nowhere, because the module CSP's `form-action` is
  `'none'`.

The shell page keeps a registry from each frame's `contentWindow` to its
platform, panel key, instance id and mount time. A frame leaving the surface
is removed from the registry before it is removed from the document, so a
message already in flight from it is dropped.

**Mounting.** A frame is mounted when its panel comes within 200 px of the
viewport. Frames out of view are unmounted to keep the surface within its
budget (see *Budget*), and remounted when their panel comes back.

**The drag shield.** While a panel is dragged or resized, the shell lays a
transparent element over every frame, so pointer events stay with the grid.

**Pages.** A navigation entry that declares a module opens it at full width in
the same kind of frame, with `init.page` set. A page is on an address of the
shell's own, `/page/{platform}/{path}`, so it can be shared and reloaded. It
has no shell-drawn fallback, and is not counted against, or unmounted by, the
budget.

## Assets

`GET /m/{platform}/{path}` serves the platform's module assets from the shell's
origin.

1. `{path}` must fall under the platform's declared `assets` prefix, by
   segment: `/ui/` matches `/ui/items/app.wasm`, not `/uix/`. Anything else,
   any `..`, any encoded separator, is 404.
2. The shell fetches the asset from the platform **as itself**, with no viewer
   identity, as it fetches a manifest ([[HLIN-S-0004]] REQ-3.1). Code is the
   same for every viewer; a module that must differ per person asks its
   platform over the bridge.
3. Sizes are bounded by `entry_bytes` and `asset_bytes` (see *Limits*).
4. `Content-Type` is taken from the platform, except that `.wasm` is always
   served as `application/wasm` and `.html` as `text/html`, since streaming
   compilation and the CSP both depend on them.
5. **Caching.** The entry document is revalidated on every mount. Other assets
   are cached immutably when the platform marks them so (`Cache-Control:
   immutable`, as hashed build output such as Trunk's is), and revalidated
   otherwise.

### The module CSP

Every response under `/m/` carries:

```
Content-Security-Policy:
  default-src 'none';
  script-src {shell}/m/{platform}/ 'wasm-unsafe-eval';
  style-src {shell}/m/{platform}/ 'unsafe-inline';
  img-src {shell}/m/{platform}/ data: blob:;
  font-src {shell}/m/{platform}/;
  connect-src {shell}/m/{platform}/;
  form-action 'none';
  base-uri 'none';
  frame-ancestors {shell}
```

`connect-src` allows the module to fetch its own assets (a `.wasm` file is
fetched), and nothing else. `'unsafe-inline'` for styles is there because
Leptos and most component kits inject style elements. Scripts stay confined.

Sources are written with the shell's explicit origin rather than `'self'`,
because what `'self'` means inside an opaque origin varies across browsers.
The containment tests assert the result in each browser the shell supports.

### The shell page's CSP

The shell page carries `frame-src {shell}/m/`. A frame's own navigations are
checked against the embedder's policy, so a module that navigates its frame
anywhere other than module assets is blocked. Without this, a frame navigated
to a hostile page would still be the same `contentWindow`, and would inherit
the bridge.

### The shell's origin

`{shell}` in both policies, and the shell's own `Origin` in the request
proxy's first check, are one value, resolved once per request so they cannot
disagree:

- The configured origin, where there is one: the shell's `public_url`, else
  `oidc`'s. Every request, whatever its `Host`, gets that origin.
- Otherwise the origin the request arrived on: its `Host`, over `http`, since
  the shell serves plain http itself. `X-Forwarded-Proto` and the like are not
  read, because with nothing configured nothing says which proxy may be
  believed; a shell behind TLS sets `public_url`. A `Host` that is not plainly
  a host and port is not believed, since it would be written into a header,
  and the request gets `http://localhost:{port}`.

The fallback is for a developer's shell, opened as `localhost` or `127.0.0.1`
or any other name. A `Host` is the caller's to choose, and choosing it buys
nothing: it is read only when nothing is configured; a policy built from it is
sent only to the caller who chose it; and every check it feeds also requires
`Sec-Fetch-Site: same-origin`, which a browser sets from the page it is on and
page script cannot, so a browser request passes only from a page the browser
already treats as the shell's. Anything that is not a browser could always
send whichever `Origin` passed. A shell reachable from other machines sets
`public_url`, which also closes the fallback to a host name somebody else has
pointed at it.

## Containment

Every isolation property above fails silently if it fails, so each is proven
in a real browser (NFR-1.3) rather than argued: `e2e/tests/containment.spec.js`
mounts the sample platform's hostile module (`ui/hostile`, plain JavaScript
under exactly the frame and policy every module gets), which tries each escape
when asked and reports what stopped it. What a frame cannot report about
itself (a navigation that ended its document, a window that did or did not
open, a request that did or did not leave the browser) is judged from outside,
by the page and by the test harness. The matrix, settled for `[1, 0]`:

| Escape attempted | Stopped by |
|---|---|
| Read the parent: its `document`, `location`, `localStorage`, a global, `top.document` | The opaque origin: every read throws `SecurityError` |
| Read cookies, `localStorage`, `sessionStorage`, IndexedDB, Cache Storage, having set some on the shell's origin first | The opaque origin: each throws or fails |
| Reach the network: `fetch` (the platform directly, the shell's API, another platform's `/m/`, anywhere), XHR, WebSocket, EventSource, an image, `sendBeacon` | The module CSP (`connect-src`, `img-src`); no request leaves the browser |
| Run script from elsewhere (the platform directly, another platform's `/m/`, `import()`), inline, `eval`, `new Function`, a `javascript:` URL, a worker from a blob, a frame of its own | The module CSP (`script-src`, and `default-src` for workers and frames) |
| Navigate its own frame off `/m/`: to its platform directly, the shell's pages, `/p/`, a `data:` document | The shell page's `frame-src`, reported on the page; nothing is fetched |
| Open a window, a link to `_blank`, navigate the top, submit a form to `_top`, navigate a sibling frame, open a dialog | The sandbox's flags (no `allow-popups`, `allow-top-navigation*`, `allow-modals`; a sandboxed frame may navigate only itself), and `form-action 'none'` |
| Pose as the shell to a sibling module (a forged `context`) | The SDK, and the hand-written modules, hear only `window.parent` |
| Use a powerful feature: geolocation, fullscreen, the clipboard | The frame's empty `allow`, where the browser governs the feature by permissions policy (see below) |
| Call `/p/` directly | The module CSP first. With it taken away (a test-only switch, where the harness has one): 403 from the proxy, since the frame's request is cross-site and carries no session; and the platform asked directly answers 401, having no token from the shell |
| Reach another platform through the bridge: a `platform` field, `..` and `%2e%2e` climbs, the proxy's own address, absolute and scheme-relative URLs | The page routes by frame, never by message: the field is ignored and answered by the frame's own platform; every climb is refused `outside_prefix` in the page |
| Announce a change as another platform | The page relays a module's `changed` only to its own platform's modules |
| Flood: 200 messages then a `fetch`; 30 `fetch`es at once | `messages_per_second` (the `fetch` is refused `too_many`); `fetches_in_flight` (8 answered, the rest `too_many`) |
| Starve the page: two frames each holding streams open unpulled | `streams` per frame (the third is `too_many`) and the page-wide cap over HTTP/1.1 (another module's stream is `too_many`), while its whole requests and the shell's own stream carry on |
| Throw, synchronously and as a rejected promise | Nothing to stop: the module's error is its own, and the page and panel are unmoved |

**Browsers.** Chromium runs the matrix on every `angreal e2e test`;
`HLIN_BROWSERS=chromium,chromium-full,firefox,webkit` runs it in the full
Chromium browser and in Firefox and WebKit as well. As first run (Chromium
153, Firefox 155, WebKit 26.6), every row holds in all four, with these
differences, which are the browsers' and not the shell's:

- **A module that spins holds the page still, except in Chromium.** A
  sandboxed frame served from the page's own site runs in the page's process
  unless the browser isolates sandboxed frames. Full Chromium does
  (`IsolateSandboxedIframes`): while a module busy-looped for five seconds the
  page kept drawing (longest frame gap 17 ms), and the heartbeat marked the
  panel `stale`, then `ready` when it stopped. Firefox, WebKit and Chromium's
  headless shell do not: the page drew nothing for the whole five seconds, and
  because the page's own clock was held too, not even `stale` could be shown
  until it was over. Containment of *what a module can reach* holds
  everywhere; containment of *its CPU* is Chromium's alone (see *Open
  Questions*).
- **The clipboard.** Chromium withholds clipboard writes from a frame whose
  `allow` does not grant them. Firefox and WebKit do not govern clipboard
  writes by permissions policy, and let a frame write (never read) the
  clipboard on a person's click; nothing the shell sends can withhold it there.
- **A blocked navigation** leaves an error page in the frame in Chromium and
  Firefox, which answers nothing, so the module is given up on like any that
  stops answering; WebKit cancels the navigation and leaves the module where
  it was.
- The request-proxy row's second half needs the CSP taken away, which
  Playwright can do in Chromium and Firefox and not in WebKit, where the CSP
  stops it first.

**Recorded, not prevented.** `frame-src` confines a frame to `/m/`, not to its
own platform's part of it: a module can load another platform's module
document into its own frame. That gives nothing away. The document runs in the
same sandbox under its own platform's CSP, and the page still attributes the
frame to the platform that owns the panel: it is never sent `init` again, and
anything it asked would be carried to the first platform, not its own. And a
cross-origin window always exposes a few properties (`length`, frames by
index, `postMessage`), which let a module find and post to its siblings; that
is the bridge's own means, and a module hears only its parent.

## Messages

### Envelope

Every message in both directions is a JSON-compatible object:

```json
{ "bridge": [1, 0], "id": "m-17", "type": "fetch", "data": { } }
```

| Field | Meaning |
|---|---|
| `bridge` | `[major, minor]` the sender speaks |
| `id` | Unique per sender for the life of the frame |
| `re` | Present on a reply: the `id` it answers |
| `type` | One of the types below |
| `data` | The type's fields |

Request and response bodies travel as `ArrayBuffer`s, transferred rather than
copied. Everything else is plain data. A message that does not have this shape
is dropped without reply.

### Shell to module

**`init`**, after the frame loads, and again every 250 ms until the module
answers `ready` or the `ready` timeout passes. A module's code starts
asynchronously (a Trunk-built module compiles its WASM after the load event),
so an `init` sent once can arrive before anything is listening and be lost,
which would look exactly like a module that never answers. A module acts on
the first `init` it receives and ignores the rest:

```json
{ "bridge": [1, 0], "id": "s-1", "type": "init", "data": {
  "platform": "checklist", "panel": "items", "instance": "7f3c…",
  "page": false,
  "context": {
    "time_range": { "from_millis": 1790200000000, "to_millis": 1790286400000 },
    "params": { "list": ["team"] },
    "generation": 4
  },
  "theme": { "scheme": "dark", "tokens": { "--hlin-surface": "#0f1115", "--hlin-accent": "#7aa2f7" } },
  "viewer": { "name": "Alice" },
  "read_only": false,
  "limits": { "request_bytes": 1048576, "response_bytes": 4194304,
              "fetches_in_flight": 8, "streams": 2 },
  "restored": null
} }
```

`params` carries only parameters the panel declares ([[HLIN-T-0028]]).
`viewer` is for display. It carries a name and nothing a platform would
authorize on: the platform learns who is asking from the token on every
request, not from its module. `read_only` says writes will be refused, so a
module can stop offering them ([[HLIN-A-0012]]). `limits` are the values in
force for this platform (see *Limits*). `restored` is what the module handed
back at its last `suspend` on this page, or `null` (see *Budget*).

**`context`**, whenever the time range or a parameter changes, with the same
fields as `init.context`. A module refetches what depends on it.

**`theme`**, whenever the scheme or tokens change, with the same fields as
`init.theme`.

`scheme` is `light` or `dark`. `tokens` are the mounted design pack's chrome
colours, and the set is closed so a module can be written against it:

| Token | Is |
|---|---|
| `--hlin-surface` | The page behind everything |
| `--hlin-raised` | A panel, card or control on it |
| `--hlin-border` | Lines between things |
| `--hlin-text` | Body text |
| `--hlin-dim` | Secondary text |
| `--hlin-faint` | Tertiary text, placeholders |
| `--hlin-accent` | The pack's accent: links, primary actions |
| `--hlin-on-accent` | Text drawn on the accent |
| `--hlin-good` | Success |
| `--hlin-warn` | Caution |
| `--hlin-bad` | Failure, refusal |

A pack may send more; a module should fall back to its own colours for any
token it does not receive.

**`changed`**, when the module's platform reports that something changed, or
when another module of the same platform says it wrote something:

```json
{ "bridge": [1, 0], "id": "s-40", "type": "changed", "data": {
  "panel": "items", "selections": { "list": ["team"] }, "from": "platform" } }
```

`from` is `platform` (its event stream, [[HLIN-A-0011]]) or `module`. A
module refetches if it cares.

**`visibility`**: `{ "visible": false }` when the panel scrolls out of view or
the tab is hidden, so a module can stop timers and animation.

**`response`**, answering a `fetch`:

```json
{ "bridge": [1, 0], "id": "s-41", "re": "m-17", "type": "response", "data": {
  "status": 403,
  "headers": { "content-type": "application/json" },
  "body": "<ArrayBuffer>",
  "refusal": null } }
```

`refusal` is `null` when the platform answered. When the shell refused the
request itself, it names why (see *Refusals*), and `status` and `body` are the
shell's. For a streamed request, `response` carries the status and headers,
`"streaming": true` and no body; the body follows as `chunk` messages.

**`heartbeat`**: `{ "n": 12 }`, every 2 seconds while the frame is visible,
and not at all while it is hidden. Browsers throttle timers in hidden
documents, so a hidden module cannot be judged by its answers; the first
heartbeat after `visibility: true` judges it instead.

**`chunk`**, **`end`**: parts of a streamed response (see *Streaming*).

**`suspend`**: `{ "deadline_ms": 500 }`, before the shell unmounts an
out-of-view frame (see *Budget*).

### Module to shell

**`ready`**, once, when the module can draw:

```json
{ "bridge": [1, 0], "id": "m-1", "type": "ready", "data": { "kit": "aurora@0.2.1" } }
```

`kit` is optional, and names the shared kit the module was built against.
The shell logs it on the operator channel and does nothing else with it (see
*Open questions*).

**`fetch`**, a request to the module's own platform:

```json
{ "bridge": [1, 0], "id": "m-17", "type": "fetch", "data": {
  "method": "POST", "path": "/api/lists/team/items",
  "query": "", "headers": { "content-type": "application/json" },
  "body": "<ArrayBuffer>", "idempotency_key": "01J8Z…" } }
```

`"stream": true` asks for the response body as it arrives rather than
whole (see *Streaming*). `path` is relative to the platform's base. `headers`
may carry only
`content-type`, `accept`, `if-match` and `if-none-match`; anything else is
dropped. `idempotency_key` is required on writes. The SDK mints one per
attempt and reuses it when a person retries.

**`set-param`**: `{ "id": "list", "values": ["team"] }`, for a parameter the
panel declares. Same effect as the chrome's control and `Intent::Select`
([[HLIN-I-0005]]): stored with the layout, sent to every module and panel that
declares it.

**`set-range`**: `{ "from_millis": …, "to_millis": … }`, the surface's time
range, as `Intent::Range`.

**`navigate`**: `{ "to": { "platform": "checklist", "page": "lists" } }`, open
a page or panel in the shell. It may name another platform: navigation is the
shell's, and opening a page grants nothing. An unknown target is ignored and
logged.

**`changed`**: `{ "panel": "items", "selections": { "list": ["team"] } }`, after
a write. The shell relays it as `changed` with `from: "module"` to that
platform's other mounted modules on every surface it serves. The shell never
infers it from a write.

**`notice`**: `{ "level": "warning", "text": "Sync paused" }`. Shown in the
panel's frame, attributed to the platform, as plain text of at most 140
characters. `level` is `info`, `warning` or `error`. This is a deliberate,
bounded exception to [[HLIN-S-0003]]'s rule that the shell never shows a
platform's words: it is confined to that platform's own panel and labelled as
theirs.

**`heartbeat`**: `{ "n": 12 }`, echoing the shell's. The SDK answers
automatically.

**`pull`**, **`cancel`**: credit for, or an end to, a streamed response (see
*Streaming*).

**`state`**: `{ "blob": "<ArrayBuffer>" }`, answering `suspend` (see *Budget*).
`{ "blob": null }`, or no `blob`, keeps nothing and lets the frame go at once.

### Limits

Every limit is operator configuration: a `[modules.limits]` table in the
shell's configuration sets the defaults, and a platform's entry may override
any of them (`[[platforms]] modules.limits = { … }`). The shell sends the
values in force in `init.limits`. The defaults:

| Limit | Default | Applies to |
|---|---|---|
| `entry_bytes` | 256 KiB | The entry document |
| `asset_bytes` | 16 MiB | Any other asset |
| `request_bytes` | 1 MiB | A `fetch` body |
| `response_bytes` | 4 MiB | A whole (not streamed) response body |
| `fetches_in_flight` | 8 | Per frame, streams included |
| `messages_per_second` | 50 | Per frame, `chunk` credit messages excluded |
| `streams` | 2 | Open streamed responses per frame |
| `stream_bytes_per_second` | 1 MiB | Per stream, the platform's pace, enforced by the shell (see *Streaming*) |
| `stream_idle_seconds` | 60 | A stream with no bytes for this long is ended |
| `state_bytes` | 64 KiB | A `state` blob |

A `fetch` past a count limit is answered with the refusal `too_many`; other
messages past the rate are dropped. A configured value is checked at startup
like the rest of the configuration: zero, or a response limit below the
request limit's floor of 1 KiB, stops the shell with the platform named.

### Streaming

A module may ask for a response body as it arrives: a log tail, a
server-sent event feed of its own, a long export. It is a read only. `stream`
on a write is refused with `method`, because a write's answer is a decision,
not a feed.

1. The module sends `fetch` with `"stream": true`.
2. The page makes the request and answers with `response`, carrying the status
   and headers and `"streaming": true`.
3. The body follows as `chunk` messages, in order:
   `{ "re": "m-17", "seq": 0, "body": "<ArrayBuffer>" }`.
4. **Credit.** The page sends only as many bytes as the module has asked for.
   The module sends `pull { "re": "m-17", "bytes": 262144 }`, and the SDK does
   so automatically as it consumes. A module that stops pulling stops the
   page reading, and the browser's own backpressure reaches the shell and then
   the platform. Nothing is buffered without bound anywhere.
5. The stream ends with `end { "re": "m-17" }`, or
   `end { "re": "m-17", "error": "idle" | "rate" | "unreachable" | "cancelled" | "unmounted" }`.
6. The module ends it early with `cancel { "re": "m-17" }`. The page aborts
   the request, and the shell drops the upstream connection.

Streams stay open while a frame is hidden; a module that wants otherwise
cancels on `visibility: false`. Unmounting a frame ends its streams with
`unmounted`.

**Connections.** Every stream is a request from the shell page to the shell's
own origin, beside the shell's own event stream ([[HLIN-S-0003]]). Over
HTTP/1.1 a browser opens about six connections to one origin, and a surface of
modules each holding a stream would exhaust them and stall everything else,
including the shell's stream. So the page counts open module streams across
the whole surface: at most 4 when the page was served over HTTP/1.1, and 32
over HTTP/2 or later, read from the navigation's `nextHopProtocol`. A stream
past the page's cap is refused with `too_many`. Serving the shell over HTTP/2
is therefore what makes streaming modules practical, and the operator guide
should say so.

**Liveness is unchanged.** A platform's own event stream is still the shell's
one subscription ([[HLIN-A-0011]]), relayed as `changed`. Streaming is for a
module's own data, not a second route for the platform's events.

At the proxy, a streamed response is passed through as it arrives: the
upstream timeout applies until the status and headers, `response_bytes` does
not apply, and `stream_bytes_per_second` and `stream_idle_seconds` do.

`stream_bytes_per_second` measures how fast the platform sends, not how fast
the shell reads: it is there to stop a platform flooding the page. The shell
holds a second's allowance and refills it at the rate, so a platform cannot
save up while it is being read. But while a module holds its stream the
platform goes on sending into the connections between until they fill, and
when the module pulls again the shell reads that backlog at once, seconds of
sending in milliseconds. So the time a stream is held earns its allowance
beyond the second, which a platform within its rate cannot have outsent; and
once the shell has had to wait for the platform's next piece the backlog is
gone, and so is what was saved for it. A platform sending faster than the
rate is still ended with `rate`: however a stream is held, no more is read
from the platform than the rate for the time the stream has been open, and a
second's allowance.

The page asks for a streamed answer with `X-Hlin-Stream`, and the shell
answers one with the same header and a framed body (data frames as they
arrive, then an end frame naming `idle`, `rate` or `unreachable`, or none),
because an HTTP body cannot otherwise say why it stopped and the module's
`end` must. A body that stops without an end frame is `unreachable`. The
format is the shell's and its page's (`hlin_stream::streamed`); a module
never sees it.

## The request proxy

The shell page sends a module's `fetch` to:

```
{METHOD} /p/{platform}/{path}?{query}
X-Hlin-Instance: 7f3c…
Idempotency-Key: 01J8Z…          (writes)
```

with the session cookie, as any request from the shell page. The shell:

1. **Checks the caller.** `Sec-Fetch-Site: same-origin` is required on every
   request. `Origin` must equal the shell's own (*The shell's origin*)
   wherever it is present, and is
   required on writes. Browsers send no `Origin` on a same-origin `GET` or
   `HEAD`, so requiring it on reads would refuse every read the page makes.
   Anything else is 403 `not_from_shell`. The session cookie's `SameSite`
   setting is not relied on, because it is configurable.
2. **Checks the person.** No session is 401 `not_signed_in`.
3. **Checks the path.** Percent-decoded once, then refused if it contains `..`,
   an empty segment, a backslash, a scheme or a host. It must fall, by segment,
   under one of the platform's declared route prefixes for its method.
4. **Checks the method.** `GET` and `HEAD` are reads and need a read prefix.
   `POST`, `PUT`, `PATCH` and `DELETE` are writes and need a write prefix.
   Anything else is 405 `method`.
5. **Checks writes.** A write needs `Author`, so a read-only shell answers 403
   `read_only`. A platform whose credential strategy collapses every viewer
   into one caller answers 409 `no_identity`. A write without
   `Idempotency-Key` is 400 `no_idempotency_key`.
6. **Mints identity.** The token [[HLIN-S-0004]] specifies. On a write it is
   bound to the request: `htm` is the method and `htu` the path without its
   query, relative to the platform's base, with a 30-second lifetime.
7. **Calls the platform** at `{base}{path}?{query}` with the body, the allowed
   headers, the token and `Idempotency-Key`. The upstream timeout applies.
   Bodies are bounded by `request_bytes` and `response_bytes`, except that a
   streamed response is passed through under the streaming limits instead.
8. **Answers.** A platform's redirect is passed back as its answer, without
   `Location`, and never followed: following it would carry the viewer's
   identity to an address the platform did not declare, and passing
   `Location` on would let the page follow it with the viewer's session. The
   platform's status and body pass back unchanged, with only
   `content-type`, `etag`, `last-modified` and `cache-control` from its
   headers. A 401 from a platform also goes to the operator channel: the
   shell's token was refused, which is a configuration fault, not the
   viewer's.

The shell retries nothing here. A module may retry a read; the SDK retries a
write only when a person asks, with the same key.

### Refusals

The shell's own refusals carry `X-Hlin-Refusal: {code}` and a short reason
the shell wrote, and arrive at the module with `refusal` set:

| Code | Status | When |
|---|---|---|
| `not_from_shell` | 403 | Step 1 |
| `not_signed_in` | 401 | Step 2 |
| `outside_prefix` | 404 | Step 3 |
| `method` | 405 | Step 4 |
| `read_only` | 403 | Step 5 |
| `no_identity` | 409 | Step 5 |
| `no_idempotency_key` | 400 | Step 5 |
| `too_large` | 413 | Either body over its limit |
| `unreachable` | 502 | Connection refused or reset |
| `timeout` | 504 | Upstream timeout |
| `too_many` | 429 | The frame's rate limit, refused in the page without a request |

Everything else is the platform's answer, and the module shows it as its own.

## Panel states

The shell page derives a module panel's state from the bridge. It never
derives it from what a module draws.

| What happened | State |
|---|---|
| Frame created, no `ready` yet | `loading` |
| `ready` received, bridge major matches | `ready` |
| One heartbeat not echoed within 2 seconds | `stale`: the frame stays, dimmed |
| Three heartbeats in a row not echoed (about 6 seconds) | `unavailable (unreachable)` |
| No `ready` within 10 seconds of the frame loading | `unavailable (unreachable)` |
| Entry document or asset unreachable, 5xx, or timed out | `unavailable (unreachable)` |
| Entry or asset over its limit, a 4xx from the asset prefix, or not HTML | `unavailable (malformed)` |
| Manifest declares a bridge major the shell does not speak, or `ready` names a different major | `unavailable (malformed)` |
| Panel or platform no longer in the registry | `unavailable (unknown)`; the frame is torn down |
| Past the panel's sunset | `unavailable (deprecated)`; never mounted |
| Unmounted out of view to stay within the budget | Its last state, held; `loading` again when it remounts |

`stale` recovers to `ready` on the next echoed heartbeat. Two seconds is
tight enough that a module blocking its main thread for a long task will show
`stale` briefly. That is accepted: `stale` only dims the frame, and a module
that blocks for seconds is one a person would notice anyway. `unreachable`
recovers only by remounting, which the shell does on the next platform
`changed` event or when a person asks, not in a loop.

`forbidden` is not derived here. A platform that refuses a viewer refuses
their module's requests, and the module shows it. `forbidden` still applies
to a panel's shell-drawn data.

### Fallback

A panel that declares both `ui` and `data` falls back to shell-drawn data when
its module is `unavailable (unreachable)` or `unavailable (malformed)`. The
frame is torn down, the data is fetched and drawn through the existing path
([[HLIN-S-0003]]), and the panel's frame says it is drawn by Hlin because the
module is unavailable. The panel's state is then the data path's state.
`unknown` and `deprecated` do not fall back, because they are about the panel,
not the module.

### Budget

At most 12 frames are mounted per surface. When mounting another would pass
that, the shell unmounts the out-of-view frame seen least recently. A frame in
view is never unmounted, so a surface with more than 12 panels in view at once
runs over the budget rather than blanking what a person is looking at.

A frame counts against the budget from the moment it is put in the document
until it has left it, including while it is being suspended. So room is made
before a frame is mounted, not after: the page suspends the frame to go, and
mounts the newcomer once that one has left the document. The count in the
document never passes 12 on the way, only when more than 12 are in view at
once ([[HLIN-T-0085]]).

Unmounting loses whatever a module held only in memory: a half-typed entry, a
scroll position, an expanded row. So before unmounting, the page sends
`suspend`, and the module may answer `state` with a blob of at most
`state_bytes` within the deadline. The page keeps it in memory for the life of
the page, never on the server and never sent to the platform, and returns it
as `init.restored` when the frame remounts. The SDK exposes this as a hook, and
answers `suspend` at once with an empty `state` when the module registered
none or the hook keeps nothing, so a module that keeps nothing costs no wait.
A module that ignores `suspend` simply starts fresh, after the page has waited
out the deadline. Reloading the page forgets every blob.

## Versioning

- The bridge is `[major, minor]`, starting at `[1, 0]`.
- A **minor** change is additive: new message types, new optional fields. Both
  sides ignore what they do not recognise (REQ-2.3), so any minor of a major
  works with any other.
- The shell sends its own minor in every message. A module must not depend on
  a message newer than that minor.
- A **major** change is anything else. A manifest names the major its module
  speaks (`ui.bridge`). The shell supports a set of majors and deprecates one
  with a window and a named successor, as it does panels. A module declaring
  an unsupported major is `unavailable (malformed)` without being mounted.
- The bridge major is contract in the manifest ([[HLIN-S-0001]]), so changing
  it without a major contract version is a violation the shell flags.

## Manifest fields this relies on

Specified in the [[HLIN-S-0001]] amendment; summarised here:

```json
{
  "assets": "/ui/",
  "routes": { "read": ["/api/"], "write": ["/api/"] },
  "navigation": [ { "label": "Lists", "path": "lists", "ui": { "entry": "/ui/lists/index.html", "bridge": 1 } } ],
  "panels": [ {
    "key": "items", "title": "To do",
    "ui": { "entry": "/ui/items/index.html", "bridge": 1 },
    "kind": "table", "envelope": "records.v1", "data": "/panels/items",
    "params": [ { "param": "select", "id": "list", "label": "List", "options": "/options/lists" } ]
  } ]
}
```

One `assets` prefix and one set of `routes` per platform. A module can reach
any route its platform declared, which is enough: the platform authorizes
every request, and a platform does not need protecting from its own module.

## Worked example

Alice opens a surface with the checklist's `items` panel.

1. **Mount.** The panel is in view. The shell page creates the frame at
   `/m/checklist/ui/items/index.html`. The shell fetches the entry from the
   checklist as itself, serves it with the module CSP, and the module's
   `.wasm` loads from its own asset path. The panel is `loading`.
2. **Handshake.** The page sends `init`. The module answers `ready` with major
   1. The panel is `ready`.
3. **A read.** The module sends `fetch GET /api/lists/team/items`. The page
   calls `/p/checklist/api/lists/team/items`; the shell checks the caller, the
   prefix and the method, mints a token for Alice addressed to `checklist`,
   and passes the platform's 200 back. The module draws the list.
4. **A write.** Alice ticks an item. The module sends `fetch POST
   /api/lists/team/items/i1/toggle` with an idempotency key. The shell checks
   `Author`, mints a token bound to `POST` and that path, and calls the
   platform. It answers 200. The module redraws and sends `changed`.
5. **Everyone else.** The shell relays `changed` to the checklist's other
   mounted modules, including Bob's. Bob's module refetches. The checklist
   also announces the change on its event stream, which reaches any shell
   this one does not know about.
6. **A refusal.** Bob edits Alice's item. The platform answers 403 with its
   own message. The shell passes it back with `refusal: null`, and Bob's
   module shows the checklist's words.
7. **A hang.** The module wedges in a loop. One heartbeat goes unanswered and
   the panel is `stale`; after three it is `unavailable (unreachable)`, the
   frame is torn down, and because the panel declares `data`, the shell draws
   the list as a table.

## Decision Log

| ADR | Title | Status | Summary |
|-----|-------|--------|---------|
| [[HLIN-A-0014]] | Platforms ship UI modules, sandboxed per frame | decided | The frame, the bridge, and the shell owning the page, the person and the wire |
| [[HLIN-A-0013]] | Requests under declared prefixes, identity bound to each | decided | The request proxy, bound tokens, no retries, the platform's own words |
| [[HLIN-A-0011]] | Platforms offer event streams | decided | Relayed to modules as `changed` |
| [[HLIN-A-0012]] | `anonymous` is read-only | decided | `read_only` in `init`; writes refused |
| [[HLIN-A-0004]] | Authentication hoisted, identity forwarded | decided | The token on every request; the platform decides |

## Decided in design

Confirmed by the owner on 2026-09-24, for [[HLIN-I-0011]]:

| Question | Decision |
|---|---|
| Prefix granularity | One `assets` prefix and one set of read and write `routes` per platform |
| Streaming | **In `[1, 0]`**: streamed reads with credit-based flow control, per-frame and per-page caps (*Streaming*) |
| Kit drift | `ready` may name the kit; logged on the operator channel, never refused |
| Frame budget | 12 mounted per surface; the least recently seen out-of-view frame is unmounted, with `suspend`/`state` so a module can keep what it held (*Budget*) |
| Timing | `ready` within 10 seconds; heartbeat every 2 seconds while visible, none while hidden; `stale` after one miss, `unavailable` after three |
| Limits | Operator configuration, per shell with per-platform overrides, defaulting to the values in *Limits* |
| `notice` | Plain text, at most 140 characters, labelled as the platform's, confined to its panel |
| `viewer` in `init` | Display name only |
| Performance | A cached module `ready` within 1 second; six modules interactive within 3 seconds cold (NFR-1.1) |

Three details were filled in while writing these down, and are worth a look:
`suspend`/`state` (so unmounting does not silently lose a person's work), no
heartbeat while hidden (browsers throttle hidden timers), and the page-wide
stream cap tied to HTTP/2 (so streams cannot starve the shell's own stream).

## Open Questions

- **Platforms' own frontends.** Nothing here depends on the answer.
- **A module's CPU, outside Chromium.** In Firefox and WebKit a module frame
  shares the page's process, so a module that spins freezes the whole
  surface, heartbeat included (*Containment*). Serving `/m/` from a site of
  its own (a module origin beside the shell's) would put frames in another
  process wherever the browser isolates by site, which Firefox does and WebKit
  does not; it would also change *The shell's origin*, the module CSP and the
  request proxy's first check. Not needed for `[1, 0]`; worth deciding before
  a platform ships a heavy module to people on Firefox.

The containment matrix, an open question until HLIN-T-0071, is settled and
written down in *Containment*.
