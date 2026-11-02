---
title: Build the shell at /
description: Give people a shell of shell components, change it, and name the admins.
---

The shell at `/` is a surface of shell components (decision HLIN-A-0015). It
holds a bar, page selectors and content components. A page selector lists
pages. A content component shows the page that a person chooses.

## Use the default shell

Set nothing. Hlin draws a shell for each person:

- A bar across the top.
- A rail down the side. It lists the person's own surfaces, and each
  platform's pages that a module draws.
- A content component, `main`. It shows the person's home surface first.

Edit changes the surface that `main` shows.

## Give everyone your own shell

1. Write the shell as JSON. Use the item types in
   [Surface JSON](../../reference/surface-json/).

   ```json
   {
     "title": "Operations",
     "items": [
       { "type": "bar", "id": "bar", "position": { "x": 0, "y": 0, "w": 12, "h": 1 } },
       {
         "type": "page-selector", "id": "tabs", "style": "tabs", "target": "main",
         "position": { "x": 0, "y": 1, "w": 12, "h": 1 },
         "groups": [
           { "label": "Pages", "pages": [
             { "page": { "platform": "orebank", "path": "runs" } },
             { "surface": "7f3c0e2a-0000-0000-0000-000000000000", "label": "Team" }
           ] }
         ]
       },
       {
         "type": "content", "id": "main",
         "position": { "x": 0, "y": 2, "w": 12, "h": 10 },
         "initial": { "page": { "platform": "orebank", "path": "runs" } }
       }
     ]
   }
   ```

2. Name the file in `hlin.toml`:

   ```toml
   default_surface = "/etc/hlin-shell/shell.json"
   ```

3. Start the shell. The shell reads the file at startup. It does not start if
   the file cannot be read, or if the file is not a shell.

Obey these rules in the file:

- Give every item an `id`. A page selector names its content component by id.
- Do not put panels in the shell. Put panels on a surface, and open the
  surface in a content component.
- Do not give two items the same id.

In the chart, set `config.defaultSurface.json` to the JSON, or set
`config.defaultSurface.existingConfigMap` to a ConfigMap that holds it.

## Link to a page

The address names the page that each content component shows. There is one
query parameter for each content component:

| Address | Shows |
|---|---|
| `/` | The shell, with each content component on its first page |
| `/?main=/s/{id}` | Surface `{id}` in `main` |
| `/?main=/page/{platform}/{path}` | A platform's page in `main` |
| `/?main=/v/{id}` | A view that the front end registers, in `main` |
| `/?main=/s/{id}&main.inner=/s/{other}` | Surface `{other}` in the content component `inner` on surface `{id}` |
| `/s/{id}` | Surface `{id}` alone, without the shell |
| `/page/{platform}/{path}` | A platform's page at full width |

Reload, a link that you send, and the back button all open the same pages.

## Change a surface's pages

1. Open the surface, and select **Edit**.
2. On each page selector, use the controls:
   - **Add a page…** adds one of your surfaces, a platform's page, or a view.
   - Type in a page's box to give it a name. Clear the box to use the page's
     own name.
   - **↑** and **↓** move a page or a group. **✕** removes it.
   - **Add group** adds a group.
3. Select **Done**.

Each change is saved at once.

## Change a surface as JSON

1. Open the surface, and select **Edit**.
2. In **Surface as JSON**, select **Show current**.
3. Change the text, or select **Load a file**.
4. Select **Apply**.

If the text is not a surface, the shell shows why and does not change the
surface. Select **Download** to keep a copy.

## Name the admins

An admin sets the org default surface, makes a surface public for anonymous
viewers, and deletes a surface that another person published (decision
HLIN-A-0016). Name admins by principal, by group or claim, or both:

```toml
[admin]
principals = ["robin"]

[auth]
strategy = "trusted-header"
groups_header = "x-forwarded-groups"
admin_groups = ["hlin-admins"]
acknowledge_proxy_required = true
```

Under `oidc`, `admin_claim` names the claim and `admin_values` the values that
make an admin. Under `anonymous`, nobody is an admin. Under `dev`, a principal
is an admin only if `[admin] principals` names it.

To set the org default, send `PUT /api/layouts/default` with
`{ "id": "<surface id>" }` as an admin. The surface must be published or public.
A person with no surface of their own opens on the org default.

An anonymous viewer sees only public surfaces. An existing anonymous shell
shows nothing until an admin makes a surface public.
