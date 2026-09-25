// A module built with the SDK, on the sample platform (HLIN-T-0071).
//
// The annotations panel is drawn by a small Leptos app on `hlin-module`
// (crates/hlin-sample-platform/module), which the sample platforms serve and
// `angreal demo up` builds. Everything else a module does in these tests is a
// hand-written probe speaking the wire directly; this is the other half of the
// claim: that a module a platform team would actually write, with the SDK and
// no bridge code of its own (NFR-1.2), draws, follows the surface's time
// range, reads and writes its platform as the person looking, and that a
// write in one browser reaches the same panel in another.
//
// Pack-agnostic: every assertion is on the shell's own markup or inside the
// module's frame, so it runs whichever demo flavour is up.

const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, openSurface, shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORM = 'orebank';

const annotations = (page) =>
  page.locator(`section.panel[data-panel="${PLATFORM}/annotations"]`);
const inside = (panel) => panel.frameLocator('iframe');

/** A layout holding only the annotations panel. */
async function layoutOf(request, title) {
  const id = await freshLayout(request, title);
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.panels = [
    { platform_id: PLATFORM, panel_key: 'annotations', position: { x: 0, y: 0, w: 6, h: 8 } },
  ];
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok()).toBeTruthy();
  return id;
}

test.describe('a module built with the SDK', () => {
  let mine;
  let theirs;

  test.beforeAll(async ({ request }) => {
    const catalog = await (await request.get('/api/panels')).json();
    const offered = catalog.find((platform) => platform.id === PLATFORM);
    const panel = (offered?.panels ?? []).find((one) => one.key === 'annotations');
    expect(panel?.ui, 'the sample platform offers its SDK module').toBeTruthy();

    mine = await layoutOf(request, 'Annotations, here');
    theirs = await layoutOf(request, 'Annotations, elsewhere');
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, mine);
    await discardLayout(request, theirs);
  });

  test('draws, and reads its platform as the person looking', async ({ page }) => {
    await openSurface(page, mine);
    const panel = annotations(page);
    await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 30_000 });

    // The platform's answer names who it thinks asked: the viewer, carried
    // by the shell's token, not the shell and not the module.
    const frame = inside(panel);
    await expect(frame.locator('#viewer')).toHaveText('Development User');
    await expect(frame.locator('#range')).toHaveAttribute('data-minutes', '60');
    await shot(page, 80, 'sdk-module');
  });

  test('follows the time picker', async ({ page }) => {
    await openSurface(page, mine);
    const panel = annotations(page);
    await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 30_000 });
    const frame = inside(panel);

    await page.locator('.bar .picker button', { hasText: '15m' }).click();
    await expect(frame.locator('#range')).toHaveAttribute('data-minutes', '15');
    await expect(frame.locator('#range')).toHaveText('The last 15 minutes');

    await page.locator('.bar .picker button', { hasText: '24h' }).click();
    await expect(frame.locator('#range')).toHaveAttribute('data-minutes', '1440');
  });

  test('a write by one viewer reaches the same module in another browser', async ({ browser }) => {
    const baseURL = test.info().project.use.baseURL;
    const one = await browser.newContext({ baseURL });
    const two = await browser.newContext({ baseURL });
    const here = await one.newPage();
    const there = await two.newPage();

    await openSurface(here, mine);
    await openSurface(there, theirs);
    const writer = annotations(here);
    const reader = annotations(there);
    for (const panel of [writer, reader]) {
      await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 30_000 });
      await expect(inside(panel).locator('#viewer')).toHaveText('Development User');
    }

    const text = `Deployed ${Date.now()}`;
    await inside(writer).locator('#note').fill(text);
    await inside(writer).locator('#pin').click();

    // Pinned by the viewer, as the platform recorded it, in the writer's
    // panel once the platform kept it...
    const written = inside(writer).locator('#notes li', { hasText: text });
    await expect(written).toHaveCount(1);
    await expect(written.locator('.who')).toContainText('Development User');
    await expect(inside(writer).locator('#problem')).toHaveCount(0);

    // ...and in the other browser's, which heard `changed` through the shell
    // and read again. Nobody reloaded it.
    await expect(inside(reader).locator('#notes li', { hasText: text })).toHaveCount(1, {
      timeout: 10_000,
    });

    // A note the platform refuses is refused in its words, and not drawn.
    await inside(writer).locator('#note').fill('x'.repeat(280));
    await inside(writer).locator('#note').evaluate((input) => {
      input.removeAttribute('maxlength');
      input.value += 'x';
      input.dispatchEvent(new Event('input', { bubbles: true }));
    });
    await inside(writer).locator('#pin').click();
    await expect(inside(writer).locator('#problem')).toHaveText('A note is at most 280 characters.');

    await shot(here, 81, 'sdk-module-written');
    await one.close();
    await two.close();
  });
});
