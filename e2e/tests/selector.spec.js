// The page selector (HLIN-T-0114, decision HLIN-A-0015).
//
// A selector lists pages in groups, as tabs or a rail, and opens the chosen
// one in the content component it targets. The address says which page each
// content component shows (`?main=/s/…`, and `main.inner=…` inside it), so a
// reload, a link and the back button all land on the same page.
//
// Pack-agnostic: every front end here registers the view `about`, and every
// assertion is on the shell's own markup.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface, asPanels } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const item = (page, id) => page.locator(`section.item[data-item="${id}"]`);

async function writeItems(request, id, items) {
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.items = items;
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok(), await written.text()).toBeTruthy();
}

test.describe('the page selector', () => {
  const made = [];
  let alpha;

  async function layout(request, title) {
    const id = await freshLayout(request, title);
    made.push(id);
    return id;
  }

  test.beforeAll(async ({ request }) => {
    alpha = await layout(request, 'Alpha');
    await writeItems(
      request,
      alpha,
      asPanels([
        { platform_id: 'orebank', panel_key: 'queue-depth', position: { x: 0, y: 0, w: 12, h: 3 } },
      ]),
    );
  });

  test.afterAll(async ({ request }) => {
    for (const id of made) {
      await discardLayout(request, id);
    }
  });

  /** A surface with a selector of `style` targeting `main`, which shows the view first. */
  async function shell(request, title, style) {
    const id = await layout(request, title);
    await writeItems(request, id, [
      {
        type: 'page-selector',
        id: 'nav',
        style,
        target: 'main',
        position: style === 'rail' ? { x: 0, y: 0, w: 3, h: 8 } : { x: 0, y: 0, w: 12, h: 1 },
        groups: [
          {
            label: 'Places',
            pages: [{ surface: alpha }, { page: { platform: 'orebank', path: 'module-page' } }],
          },
          { label: 'Code', pages: [{ view: 'about', label: 'About' }] },
        ],
      },
      {
        type: 'content',
        id: 'main',
        position: style === 'rail' ? { x: 3, y: 0, w: 9, h: 8 } : { x: 0, y: 1, w: 12, h: 8 },
        initial: { view: 'about' },
      },
    ]);
    return id;
  }

  test('a rail lists its groups, and a chosen page opens in its target, by address', async ({ page, request }) => {
    const outer = await shell(request, 'Railed', 'rail');
    await openSurface(page, outer);

    const rail = page.locator('nav.page-selector[data-style="rail"][data-target="main"]');
    await expect(rail).toBeVisible();
    await expect(rail.locator('.selector-group-label')).toHaveText(['Places', 'Code']);
    // Labels from what each page is: the surface's title, the platform's
    // label for its page, and one the layout gave.
    await expect(rail.locator('a.selector-page')).toHaveText(['Alpha', 'Module page', 'About']);

    // Before anything is chosen, the target's own first page is current.
    await expect(rail.locator('a.selector-page[aria-current="page"]')).toHaveText('About');
    await expect(item(page, 'main').locator('.demo-about')).toBeVisible();

    // Choose the surface.
    await rail.locator('a.selector-page', { hasText: 'Alpha' }).click();
    await expect(page).toHaveURL(new RegExp(`/s/${outer}\\?main=/s/${alpha}$`));
    await expect(rail.locator('a.selector-page[aria-current="page"]')).toHaveText('Alpha');
    const nested = item(page, 'main').locator('section.panel[data-panel="orebank/queue-depth"]');
    await expect(nested).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
    await shot(page, 1, 'selector-rail');

    // A reload lands on the same page.
    await page.reload();
    await expect(page.locator('a.selector-page[aria-current="page"]')).toHaveText('Alpha');
    await expect(nested).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });

    // Choose the platform's page, then go back.
    await page.locator('a.selector-page', { hasText: 'Module page' }).click();
    await expect(page).toHaveURL(/\?main=\/page\/orebank\/module-page$/);
    await expect(item(page, 'main').locator('section.page[data-page="orebank/module-page"] iframe')).toBeVisible({
      timeout: 30_000,
    });
    await page.goBack();
    await expect(page.locator('a.selector-page[aria-current="page"]')).toHaveText('Alpha');
    await expect(nested).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
  });

  test('tabs choose the same way', async ({ page, request }) => {
    const outer = await shell(request, 'Tabbed', 'tabs');
    await openSurface(page, outer);

    const tabs = page.locator('nav.page-selector[data-style="tabs"]');
    await expect(tabs).toBeVisible();
    await tabs.locator('a.selector-page', { hasText: 'Alpha' }).click();
    await expect(tabs.locator('a.selector-page[aria-current="page"]')).toHaveText('Alpha');
    await expect(item(page, 'main').locator('section.panel[data-panel="orebank/queue-depth"]')).toHaveCount(1);
    await shot(page, 2, 'selector-tabs');
  });

  test('a selector inside a surface opened in a content component has its own key in the address', async ({
    page,
    request,
  }) => {
    // Inner: its own selector, choosing for its own content component.
    const inner = await layout(request, 'Inner');
    await writeItems(request, inner, [
      {
        type: 'page-selector',
        id: 'tabs',
        style: 'tabs',
        target: 'inner',
        position: { x: 0, y: 0, w: 12, h: 1 },
        groups: [{ label: 'Inside', pages: [{ surface: alpha }, { view: 'about', label: 'About' }] }],
      },
      { type: 'content', id: 'inner', position: { x: 0, y: 1, w: 12, h: 6 }, initial: { surface: alpha } },
    ]);
    const outer = await layout(request, 'Around');
    await writeItems(request, outer, [
      { type: 'content', id: 'main', position: { x: 0, y: 0, w: 12, h: 9 }, initial: { surface: inner } },
    ]);

    await openSurface(page, outer);
    const tabs = item(page, 'main').locator('nav.page-selector[data-target="inner"]');
    await expect(tabs.locator('a.selector-page[aria-current="page"]')).toHaveText('Alpha');

    await tabs.locator('a.selector-page', { hasText: 'About' }).click();
    await expect(page).toHaveURL(/\?main\.inner=\/v\/about$/);
    await expect(item(page, 'main').locator('.demo-about')).toBeVisible();

    await page.reload();
    await expect(
      item(page, 'main').locator('nav.page-selector a.selector-page[aria-current="page"]'),
    ).toHaveText('About');
    await expect(item(page, 'main').locator('.demo-about')).toBeVisible();
  });
});
