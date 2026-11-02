// The shell, end to end (HLIN-T-0118, initiative COLLIERY-I-0654).
//
// The default shell at `/`; a page chosen in its rail; a surface opened in it
// with a selector of its own; a link to the inner page, sent to somebody else
// and opened in their browser; and an operator's stylesheet restyling the
// selector by its class names.
//
// The first runs on every flavour but `shell`, whose shell is the operator's;
// the stylesheet test on `angreal demo up --with brand`, whose demo/brand/
// brand.css outlines the shown page in Acme's magenta.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, asPanels } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const item = (page, id) => page.locator(`section.item[data-item="${id}"]`);

async function writeItems(request, id, items) {
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.items = items;
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok(), await written.text()).toBeTruthy();
}

test.describe('the shell, end to end', () => {
  const made = [];

  test.afterAll(async ({ request }) => {
    for (const id of made) {
      await discardLayout(request, id);
    }
  });

  test('a page chosen in the rail, a surface inside it, and a link to the inner page', async ({
    page,
    browser,
    baseURL,
    request,
  }) => {
    const shell = await (await request.get('/api/shell')).json();
    test.skip(shell.title === 'Operator shell', "this shell is the operator's");
    const config = await (await request.get('/api/config')).json();
    test.skip(config.read_only, 'an open shell lists no surfaces of anybody');

    const alpha = await freshLayout(request, 'Proof alpha');
    const outer = await freshLayout(request, 'Proof outer');
    made.push(alpha, outer);
    await writeItems(
      request,
      alpha,
      asPanels([
        { platform_id: 'orebank', panel_key: 'queue-depth', position: { x: 0, y: 0, w: 12, h: 3 } },
      ]),
    );
    await writeItems(request, outer, [
      {
        type: 'page-selector',
        id: 'tabs',
        style: 'tabs',
        target: 'inner',
        position: { x: 0, y: 0, w: 12, h: 1 },
        groups: [{ label: 'Inside', pages: [{ view: 'about', label: 'About' }, { surface: alpha }] }],
      },
      { type: 'content', id: 'inner', position: { x: 0, y: 1, w: 12, h: 6 }, initial: { view: 'about' } },
    ]);

    // The default shell, and the rail's own surfaces.
    await page.goto('/');
    const rail = page.locator('nav.page-selector[data-style="rail"][data-target="main"]');
    await rail.locator(`a.selector-page[data-address="/s/${outer}"]`).click();
    await expect(page).toHaveURL(new RegExp(`/\\?main=/s/${outer}$`));
    await expect(rail.locator('a.selector-page[aria-current="page"]')).toHaveText('Proof outer');

    // The surface in `main` has its own selector, for its own content.
    const tabs = item(page, 'main').locator('nav.page-selector[data-target="inner"]');
    await expect(tabs.locator('a.selector-page[aria-current="page"]')).toHaveText('About');
    await tabs.locator('a.selector-page', { hasText: 'Proof alpha' }).click();
    const address = new RegExp(`/\\?main=/s/${outer}&main\\.inner=/s/${alpha}$`);
    await expect(page).toHaveURL(address);
    const panel = item(page, 'main').locator('section.panel[data-panel="orebank/queue-depth"]');
    await expect(panel).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
    await shot(page, 1, 'proof-nested');

    // The link, sent to somebody: a fresh browser lands on the inner page.
    const link = page.url();
    const elsewhere = await browser.newContext({ baseURL });
    const theirs = await elsewhere.newPage();
    await theirs.goto(link);
    await expect(theirs).toHaveURL(address);
    await expect(
      item(theirs, 'main').locator('nav.page-selector[data-target="inner"] a.selector-page[aria-current="page"]'),
    ).toHaveText('Proof alpha');
    await expect(
      item(theirs, 'main').locator('section.panel[data-panel="orebank/queue-depth"]'),
    ).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
    await elsewhere.close();
  });

  test("an operator's stylesheet restyles the selector by its class names", async ({ page, request }) => {
    const brand = (await (await request.get('/api/config')).json()).brand;
    test.skip(brand.name !== 'Acme Operations', 'run with `angreal demo up --with brand`');

    await page.goto('/');
    const shown = page.locator('nav.page-selector a.selector-page[aria-current="page"]');
    await expect(shown).toHaveCount(1);
    await expect(shown).toHaveCSS('outline-color', 'rgb(176, 19, 95)');
    await expect(shown).toHaveCSS('outline-style', 'solid');
    await shot(page, 2, 'proof-brand');
  });
});
