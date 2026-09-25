// What a module and the surface around it tell each other, once it is mounted.
//
// The module is the sample platform's hand-written probe
// (crates/hlin-sample-platform/ui/probe), which shows on its own page
// everything the shell's page tells it, and has a button for everything it can
// ask. So each claim here is read off the module's side of the bridge: the
// context it was told, the parameter it set surviving a reload, the notice it
// asked for drawn as its platform's, the budget taking its frame and handing
// back what it kept, and a change it reported reaching the same module in
// another person's browser.
//
// Pack-agnostic: every assertion is on the shell's own markup or inside the
// module's frame, so it runs whichever demo flavour is up.

const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, openSurface, settled, shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORM = 'orebank';

/** A layout of these panels, stacked or placed as given. */
async function layoutOf(request, title, panels) {
  const id = await freshLayout(request, title);
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.panels = panels.map(([key, position]) => ({
    platform_id: PLATFORM,
    panel_key: key,
    position,
  }));
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok()).toBeTruthy();
  return id;
}

const byKey = (page, key) => page.locator(`section.panel[data-panel="${PLATFORM}/${key}"]`);
const inside = (panel) => panel.frameLocator('iframe');

test.describe('a module told where it is, asking for things', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    layoutId = await layoutOf(request, 'Module context', [
      ['module-context', { x: 0, y: 0, w: 6, h: 14 }],
      ['module-only', { x: 6, y: 0, w: 6, h: 14 }],
      // Below both, off the first screen, so going to it is a visible move.
      ['module-probe', { x: 0, y: 14, w: 6, h: 6 }],
    ]);
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('context follows the time picker into a module, with only its declared parameters', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // The picker's default hour, from `init`.
    await expect(inside(context).locator('#range')).toHaveText('60 minutes');
    await expect(inside(context).locator('#params')).toHaveText('{}');
    const before = Number(await inside(context).locator('#generation').textContent());

    // Moved in the chrome: told as `context`, with the generation that asked.
    await page.locator('.bar .picker button', { hasText: '15m' }).click();
    await expect(inside(context).locator('#range')).toHaveText('15 minutes');
    await expect
      .poll(async () => Number(await inside(context).locator('#generation').textContent()))
      .toBeGreaterThan(before);

    await page.locator('.bar .picker button', { hasText: '24h' }).click();
    await expect(inside(context).locator('#range')).toHaveText('1440 minutes');
  });

  test('a module is sent the theme the page is drawn in', async ({ page }) => {
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // The chrome's eleven colour roles, as the mounted pack filled them, and
    // the scheme the page actually reads as.
    await expect(inside(context).locator('#theme')).toHaveText(/^(light|dark), 11 tokens$/);
    const accent = await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue('--hlin-accent').trim(),
    );
    await expect(inside(context).locator('#theme')).toHaveAttribute('data-accent', accent);
  });

  test('set-param from a module is stored with the layout, and sticks after a reload', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    await inside(context).locator('#choose').click();
    const chosen = JSON.stringify({ cluster: [`${PLATFORM}-lab`] });
    await expect(inside(context).locator('#params')).toHaveText(chosen);
    await settled(page);

    // Where the chrome's own control would have put it.
    const stored = await (await page.request.get(`/api/layouts/${layoutId}`)).json();
    const panel = stored.panels.find((p) => p.panel_key === 'module-context');
    expect(panel.selections).toEqual({ cluster: [`${PLATFORM}-lab`] });

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await expect(byKey(page, 'module-context')).toHaveAttribute('data-module', 'ready', {
      timeout: 20_000,
    });
    await expect(inside(byKey(page, 'module-context')).locator('#params')).toHaveText(chosen);

    // A panel that declares no such parameter cannot be given one.
    const only = byKey(page, 'module-only');
    await expect(only).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await inside(only).locator('#choose').click();
    await page.waitForTimeout(500);
    await settled(page);
    const after = await (await page.request.get(`/api/layouts/${layoutId}`)).json();
    expect(after.panels.find((p) => p.panel_key === 'module-only').selections ?? {}).toEqual({});
  });

  test('set-range from a module moves the whole surface, picker included', async ({ page }) => {
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await page.locator('.bar .picker button', { hasText: '6h' }).click();
    await expect(inside(context).locator('#range')).toHaveText('360 minutes');

    await inside(byKey(page, 'module-only')).locator('#fifteen').click();
    await expect(inside(context).locator('#range')).toHaveText('15 minutes');
    // The bar no longer claims a preset it is not showing.
    await expect(page.locator('.bar .picker button.active')).toHaveCount(0);
  });

  test('a notice is plain text, cut to length, labelled as the platform’s, in its own panel only', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    await inside(context).locator('#notice').click();
    const notice = context.locator('.module-notice');
    await expect(notice).toHaveAttribute('data-level', 'warning');
    await expect(notice.locator('.module-notice-from')).toHaveText('Orebank says');
    const text = await notice.locator('.module-notice-text').textContent();
    expect(text.startsWith('Sync paused. <b>Not markup</b> and on')).toBeTruthy();
    expect([...text].length).toBeLessThanOrEqual(140);
    expect(text.endsWith('…')).toBeTruthy();
    await expect(notice.locator('b')).toHaveCount(0);
    await expect(page.locator('.module-notice')).toHaveCount(1);
    await shot(page, 63, 'module-notice');
  });

  test('navigate brings a panel into view, and an unknown target is ignored and logged', async ({
    page,
  }) => {
    const warnings = [];
    page.on('console', (message) => warnings.push(message.text()));
    await openSurface(page, layoutId);
    const context = byKey(page, 'module-context');
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    const far = byKey(page, 'module-probe');
    await expect(far).not.toBeInViewport();
    await inside(context).locator('#goto').click();
    await expect(far).toBeInViewport();

    await page.evaluate(() => window.scrollTo(0, 0));
    await inside(context).locator('#nowhere').click();
    await expect.poll(() => warnings.some((text) => text.includes('no platform offers'))).toBe(true);
    await expect(page).toHaveURL(new RegExp(`/s/${layoutId}$`));
  });
});

test.describe('the frame budget', () => {
  let layoutId;
  const COUNT = 13;

  test.beforeAll(async ({ request }) => {
    layoutId = await layoutOf(
      request,
      'Thirteen modules',
      Array.from({ length: COUNT }, (_, index) => [
        'module-only',
        { x: 0, y: index * 4, w: 12, h: 4 },
      ]),
    );
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('past twelve, the least recently seen frame out of view is unmounted and hands back its state', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const panels = page.locator('section.panel');
    await expect(panels).toHaveCount(COUNT);
    const first = panels.nth(0);
    await expect(first).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await inside(first).locator('#note').fill('kept across the budget');

    // Down the surface a step at a time, so each panel comes near and mounts,
    // and the first leaves view before any other.
    const height = await page.evaluate(() => document.body.scrollHeight);
    for (let y = 0; y <= height; y += 250) {
      await page.evaluate((to) => window.scrollTo(0, to), y);
      await page.waitForTimeout(120);
    }
    const last = panels.nth(COUNT - 1);
    await expect(last).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // Twelve mounted, and the one gone is the first: seen longest ago.
    await expect(first.locator('iframe')).toHaveCount(0, { timeout: 5_000 });
    await expect(page.locator('section.panel iframe')).toHaveCount(12);
    await expect(last).toBeInViewport();
    await expect(last.locator('iframe')).toHaveCount(1);
    // One still mounted, scrolled out of view, was told so.
    await expect(inside(panels.nth(3)).locator('#visible')).toHaveText('no');
    await expect(inside(last).locator('#visible')).toHaveText('yes');

    // Back to the top: mounted again, a new handshake, and what it kept.
    for (let y = height; y >= 0; y -= 250) {
      await page.evaluate((to) => window.scrollTo(0, to), y);
      await page.waitForTimeout(60);
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    await expect(first.locator('iframe')).toHaveCount(1, { timeout: 10_000 });
    await expect(first).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(inside(first).locator('#restored')).toHaveText('kept across the budget');
    await expect(inside(first).locator('#note')).toHaveValue('kept across the budget');
    await expect(page.locator('section.panel iframe')).toHaveCount(12);
  });
});

test.describe('changes relayed to a platform’s modules', () => {
  let mine;
  let theirs;

  test.beforeAll(async ({ request }) => {
    mine = await layoutOf(request, 'Relay, mine', [
      ['module-probe', { x: 0, y: 0, w: 6, h: 6 }],
      ['module-only', { x: 6, y: 0, w: 6, h: 14 }],
    ]);
    theirs = await layoutOf(request, 'Relay, theirs', [
      ['module-probe', { x: 0, y: 0, w: 6, h: 6 }],
    ]);
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, mine);
    await discardLayout(request, theirs);
  });

  test('a module’s change reaches the same platform’s module in another browser, which refetches', async ({
    browser,
  }) => {
    const baseURL = test.info().project.use.baseURL;
    const one = await browser.newContext({ baseURL });
    const two = await browser.newContext({ baseURL });
    const here = await one.newPage();
    const there = await two.newPage();

    await openSurface(here, mine);
    await openSurface(there, theirs);
    const writer = byKey(here, 'module-probe');
    const beside = byKey(here, 'module-only');
    const reader = byKey(there, 'module-probe');
    for (const panel of [writer, beside, reader]) {
      await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    }
    await expect(inside(reader).locator('#whoami')).toHaveAttribute('data-asked', '1');

    await inside(writer).locator('#wrote').click();

    // The other browser, on another surface: through the shell, and it asks
    // its platform again.
    const heard = inside(reader).locator('#changes');
    await expect(heard).toHaveAttribute('data-module', 'module-probe', { timeout: 10_000 });
    await expect(inside(reader).locator('#whoami')).toHaveAttribute('data-asked', '2');
    await expect(inside(reader).locator('#whoami')).toHaveText(/ on orebank \(200\)$/);

    // The module beside the writer heard it at once, from its own page, and
    // the writer was not told its own news.
    await expect(inside(beside).locator('#changes')).toHaveAttribute('data-module', 'module-probe');
    await expect(inside(writer).locator('#whoami')).toHaveAttribute('data-asked', '1');
    const writerHeard = await inside(writer).locator('#changes').getAttribute('data-module');
    expect(writerHeard).toBeNull();

    await one.close();
    await two.close();
  });

  test('a platform’s own event stream reaches its modules as a change', async ({ page }) => {
    await openSurface(page, theirs);
    const reader = byKey(page, 'module-probe');
    await expect(reader).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    // The sample platform moves a pushed panel every few seconds.
    await expect(inside(reader).locator('#changes')).toHaveAttribute('data-platform', /.+/, {
      timeout: 30_000,
    });
  });
});
