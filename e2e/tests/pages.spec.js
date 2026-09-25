// A platform's page, open at full width inside the shell (HLIN-S-0007, *Pages*).
//
// The pages are the sample platform's navigation entries: one whose module is
// a hand-written page of plain JavaScript (crates/hlin-sample-platform/ui/page)
// that shows what it was told, and one whose module never says `ready`. Its
// `Overview` entry has no module, and is only a link.
//
// What is claimed: an entry with a module is in the shell's navigation and
// opens in the same kind of sandboxed frame as a panel's, with `init.page`
// set, on an address of the shell's own that survives a reload; a module's
// `navigate` opens one too; a page whose module fails has nothing drawn in its
// place and says it is unavailable; and an entry without a module is still not
// something the shell opens.
//
// Pack-agnostic: every assertion is on the shell's own markup or inside the
// page's frame, so it runs whichever demo flavour is up.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORM = 'orebank';
const ADDRESS = `/page/${PLATFORM}/module-page`;

const link = (page, path) => page.locator(`nav.pages a.page-link[data-page="${PLATFORM}/${path}"]`);
const opened = (page, path) => page.locator(`section.page[data-page="${PLATFORM}/${path}"]`);
const probe = (page) => page.locator(`section.panel[data-panel="${PLATFORM}/module-probe"]`);
const inside = (locator) => locator.frameLocator('iframe');

test.describe("a platform's page inside the shell", () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    const catalog = await (await request.get('/api/panels')).json();
    const offered = catalog.find((platform) => platform.id === PLATFORM);
    const entries = Object.fromEntries(
      (offered?.navigation ?? []).map((entry) => [entry.path, entry.ui?.entry ?? null]),
    );
    expect(entries, 'the sample platform offers two pages and a plain link').toEqual({
      overview: null,
      'module-page': '/ui/page/index.html',
      'silent-page': '/ui/silent/index.html',
    });

    layoutId = await freshLayout(request, 'Pages');
    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    current.panels = [
      { platform_id: PLATFORM, panel_key: 'module-probe', position: { x: 0, y: 0, w: 12, h: 8 } },
    ];
    const written = await request.put(`/api/layouts/${layoutId}`, { data: current });
    expect(written.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('only an entry with a module is in the navigation', async ({ page }) => {
    await openSurface(page, layoutId);
    await expect(link(page, 'module-page')).toHaveText('Module page');
    await expect(link(page, 'module-page')).toHaveAttribute('href', ADDRESS);
    await expect(link(page, 'silent-page')).toBeVisible();
    await expect(link(page, 'overview')).toHaveCount(0);
    await expect(page.locator('nav.pages a.surface-link')).toHaveAttribute('aria-current', 'page');
  });

  test('opens from the navigation at full width, in a sandboxed frame told it is a page', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await link(page, 'module-page').click();

    await expect(page).toHaveURL(new RegExp(`${ADDRESS}$`));
    const shown = opened(page, 'module-page');
    await expect(shown).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(shown).toHaveAttribute('data-state', 'ready');
    await expect(link(page, 'module-page')).toHaveAttribute('aria-current', 'page');
    // The surface is out of the way, not gone.
    await expect(page.locator('.stage')).toBeHidden();
    await expect(probe(page)).toHaveCount(1);

    // The same frame a panel gets, and nothing more permissive.
    const frame = shown.locator('iframe');
    await expect(frame).toHaveCount(1);
    await expect(frame).toHaveAttribute('sandbox', 'allow-scripts allow-forms');
    await expect(frame).toHaveAttribute('allow', '');
    await expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer');
    await expect(frame).toHaveAttribute('src', new RegExp(`^/m/${PLATFORM}/ui/page/index\\.html#i=`));
    await expect(frame).toHaveAttribute('title', /Module page — Orebank/);

    // At full width: the page's frame spans the window, less the chrome's
    // margins, where a panel would have had its column.
    const width = (await frame.boundingBox()).width;
    expect(width).toBeGreaterThan(page.viewportSize().width * 0.9);

    // `init.page` set, the entry's path where a panel's key would be, and the
    // same bridge as any module: it asks its platform through the shell.
    const { principal } = await (await page.request.get('/api/config')).json();
    const body = inside(shown).locator('body');
    await expect(body).toHaveAttribute('data-page', 'true');
    await expect(body).toHaveAttribute('data-path', 'module-page');
    await expect(inside(shown).locator('#restored')).toHaveText('nothing');
    await expect(inside(shown).locator('#whoami')).toHaveText(
      `${principal.sub} on ${PLATFORM} (200)`,
      { timeout: 20_000 },
    );
    await shot(page, 70, 'page-open');

    // Back to the surface by the navigation, and to the page again by the
    // browser's own history.
    await page.locator('nav.pages a.surface-link').click();
    await expect(page).toHaveURL(new RegExp(`/s/${layoutId}$`));
    await expect(shown).toHaveCount(0);
    await expect(probe(page)).toBeVisible();
    await page.goBack();
    await expect(page).toHaveURL(new RegExp(`${ADDRESS}$`));
    await expect(opened(page, 'module-page')).toHaveAttribute('data-module', 'ready', {
      timeout: 20_000,
    });
  });

  test('its address can be sent to somebody, and reloaded', async ({ page }) => {
    await page.goto(ADDRESS);
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await expect(opened(page, 'module-page')).toHaveAttribute('data-module', 'ready', {
      timeout: 20_000,
    });

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await expect(page).toHaveURL(new RegExp(`${ADDRESS}$`));
    await expect(opened(page, 'module-page')).toHaveAttribute('data-module', 'ready', {
      timeout: 20_000,
    });
    await expect(inside(opened(page, 'module-page')).locator('body')).toHaveAttribute(
      'data-page',
      'true',
    );
  });

  test("a module's navigate opens a page, and a page's can go back to a panel", async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    await inside(probe(page)).locator('#to-page').click();
    await expect(page).toHaveURL(new RegExp(`${ADDRESS}$`));
    const shown = opened(page, 'module-page');
    await expect(shown).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    await inside(shown).locator('#to-panel').click();
    await expect(page).toHaveURL(new RegExp(`/s/${layoutId}$`));
    await expect(shown).toHaveCount(0);
    await expect(probe(page)).toBeInViewport();
  });

  test('a page whose module fails says it is unavailable, with nothing in its place', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await link(page, 'silent-page').click();
    await expect(page).toHaveURL(new RegExp(`/page/${PLATFORM}/silent-page$`));

    const shown = opened(page, 'silent-page');
    // Past the `ready` timeout: given up on, and torn down. A page has no data
    // to fall back to, so this is what it shows.
    await expect(shown).toHaveAttribute('data-state', 'unavailable', { timeout: 30_000 });
    await expect(shown).toHaveAttribute('data-module', 'unavailable');
    await expect(shown).toHaveAttribute('data-module-cause', 'unreachable');
    await expect(shown.locator('iframe')).toHaveCount(0);
    await expect(shown.locator('.module-note')).toContainText('not responding');
    await expect(shown.locator('.retry-module')).toBeVisible();
    await shot(page, 71, 'page-unavailable');

    // Trying again mounts it again: `loading`, with a frame.
    await shown.locator('.retry-module').click();
    await expect(shown).toHaveAttribute('data-module', 'loading');
    await expect(shown.locator('iframe')).toHaveCount(1);
  });

  test('an entry without a module is still not something the shell opens', async ({ page }) => {
    const warnings = [];
    page.on('console', (message) => warnings.push(message.text()));
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // Asked for by a module: ignored and logged, and the surface stays.
    await inside(probe(page)).locator('#to-link').click();
    await expect
      .poll(() => warnings.some((text) => text.includes('not a page this shell can open')))
      .toBe(true);
    await expect(page).toHaveURL(new RegExp(`/s/${layoutId}$`));
    await expect(page.locator('section.page')).toHaveCount(0);

    // Asked for by address: nothing mounted, and saying so.
    await page.goto(`/page/${PLATFORM}/overview`);
    const shown = opened(page, 'overview');
    await expect(shown).toHaveAttribute('data-state', 'unavailable');
    await expect(shown).toHaveAttribute('data-module-cause', 'unknown');
    await expect(shown.locator('iframe')).toHaveCount(0);
  });
});
