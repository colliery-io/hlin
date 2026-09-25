// A platform's own module, in a sandboxed frame on a surface.
//
// The modules here are the sample platform's hand-written ones
// (crates/hlin-sample-platform/ui): plain HTML and JavaScript that speak the
// bridge's JSON directly, so what is being tested is the page against the
// specification, not the page against an SDK built from the same crate.
//
// What is claimed: a module mounts in a frame the page made, says `ready`, and
// can ask its own platform something through the shell as the person looking;
// a module that stops answering is dimmed, then given up on, and a panel that
// declares data is then drawn by Hlin instead; a module that never starts is
// given up on the same way; and a frame survives the gestures that move its
// panel, with nothing inside it able to take the pointer while they do.
//
// Pack-agnostic: every assertion is on the shell's own markup or inside the
// module's frame, so it runs whichever demo flavour is up.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface } = require('./helpers');

test.describe.configure({ mode: 'serial' });

/** The sample platform whose modules are used. Either would do. */
const PLATFORM = 'orebank';

test.describe('a platform module in a sandboxed frame', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    const catalog = await (await request.get('/api/panels')).json();
    const offered = catalog.find((platform) => platform.id === PLATFORM);
    const keys = (offered?.panels ?? []).filter((panel) => panel.ui).map((panel) => panel.key);
    expect(keys, 'the sample platform offers its module panels').toEqual(
      expect.arrayContaining(['module-probe', 'module-silent', 'module-only']),
    );

    layoutId = await freshLayout(request, 'Modules');
    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    current.panels = [
      { platform_id: PLATFORM, panel_key: 'module-probe', position: { x: 0, y: 0, w: 6, h: 4 } },
      { platform_id: PLATFORM, panel_key: 'module-silent', position: { x: 6, y: 0, w: 6, h: 4 } },
      { platform_id: PLATFORM, panel_key: 'module-only', position: { x: 0, y: 4, w: 6, h: 3 } },
    ];
    const written = await request.put(`/api/layouts/${layoutId}`, { data: current });
    expect(written.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  const probe = (page) => page.locator(`section.panel[data-panel="${PLATFORM}/module-probe"]`);
  const silent = (page) => page.locator(`section.panel[data-panel="${PLATFORM}/module-silent"]`);
  const only = (page) => page.locator(`section.panel[data-panel="${PLATFORM}/module-only"]`);
  const inside = (panel) => panel.frameLocator('iframe');

  test('mounts in a sandboxed frame and says it is ready', async ({ page }) => {
    await openSurface(page, layoutId);

    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(probe(page)).toHaveAttribute('data-state', 'ready');

    // The frame the specification describes, and nothing more permissive.
    const frame = probe(page).locator('iframe');
    await expect(frame).toHaveCount(1);
    await expect(frame).toHaveAttribute('sandbox', 'allow-scripts allow-forms');
    await expect(frame).toHaveAttribute('allow', '');
    await expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer');
    await expect(frame).toHaveAttribute('src', new RegExp(`^/m/${PLATFORM}/ui/probe/index\\.html#i=`));
    await expect(frame).toHaveAttribute('title', /Module probe — Orebank/);

    // And the module heard `init`: who is looking, and which instance it is.
    const { principal } = await (await page.request.get('/api/config')).json();
    await expect(inside(probe(page)).locator('#state')).toHaveText('ready');
    await expect(inside(probe(page)).locator('#viewer')).toHaveText(principal.name ?? principal.sub);
    const instance = await probe(page).getAttribute('data-instance');
    await expect(inside(probe(page)).locator('body')).toHaveAttribute('data-instance', instance);

    await shot(page, 60, 'module-ready');
  });

  test('asks its own platform through the shell, as the person looking', async ({ page }) => {
    await openSurface(page, layoutId);
    const { principal } = await (await page.request.get('/api/config')).json();

    // The probe's `fetch` goes to the page, the page to `/p/orebank/…`, the
    // shell to the platform with a token for this viewer, and the platform's
    // answer back the same way. What the platform says it saw is the proof.
    await expect(inside(probe(page)).locator('#whoami')).toHaveText(
      `${principal.sub} on ${PLATFORM} (200)`,
      { timeout: 20_000 },
    );
  });

  test('a panel only its module draws is placed and drawn by it', async ({ page }) => {
    await openSurface(page, layoutId);
    await expect(only(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(only(page)).toHaveAttribute('data-state', 'ready');
    await expect(inside(only(page)).locator('#state')).toHaveText('ready');
  });

  test('a frame asking for more than it may have in flight is refused in the page', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // Twenty at once, against a limit of eight in flight. Whichever are past
    // the limit are answered `too_many` by the page without a request; the
    // rest reach the platform.
    await inside(probe(page)).locator('#twenty').click();
    const flood = inside(probe(page)).locator('#flood');
    await expect(flood).toHaveText(/^\d+ answered, \d+ refused, 0 other$/, { timeout: 20_000 });
    await expect
      .poll(async () => {
        const [answered, refused] = (await flood.textContent()).match(/\d+/g).map(Number);
        return answered + refused;
      })
      .toBe(20);
    const [answered, refused] = (await flood.textContent()).match(/\d+/g).map(Number);
    expect(refused, 'some were past the limit').toBeGreaterThan(0);
    expect(answered, 'and at least the limit were carried').toBeGreaterThanOrEqual(8);
  });

  test('a frame survives its panel being dragged, and cannot take the pointer while it is', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    // Something only this document holds. A frame recreated by the drag would
    // be a new document without it, and a module reloaded mid-gesture.
    const frame = page.frames().find((candidate) => candidate.url().includes('/ui/probe/'));
    await frame.evaluate(() => {
      window.stillHere = 'yes';
    });

    await page.locator('button.mode').click();
    const grip = probe(page).locator('.grip');
    await grip.waitFor({ state: 'visible' });
    const box = await grip.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + 40, box.y + 60, { steps: 4 });

    // Mid-gesture: the shield is up, and it is what is under the pointer
    // wherever a frame is.
    await expect(page.locator('.drag-shield')).toHaveCount(1);
    const covered = await probe(page)
      .locator('iframe')
      .evaluate((node) => {
        const rect = node.getBoundingClientRect();
        const top = document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
        return top?.className;
      });
    expect(covered).toBe('drag-shield');

    await page.mouse.move(box.x + 700, box.y + 80, { steps: 6 });
    await page.mouse.up();
    await expect(page.locator('.drag-shield')).toHaveCount(0);
    await expect(page.locator('.saving')).toHaveCount(0);

    expect(await frame.evaluate(() => window.stillHere)).toBe('yes');
    await expect(probe(page)).toHaveAttribute('data-module', 'ready');

    // Put it back where the other tests expect it.
    const current = await (await page.request.get(`/api/layouts/${layoutId}`)).json();
    for (const panel of current.panels) {
      if (panel.panel_key === 'module-probe') {
        panel.position = { x: 0, y: 0, w: 6, h: 4 };
      }
    }
    await page.request.put(`/api/layouts/${layoutId}`, { data: current });
  });

  test('a module that stops answering goes stale, then unavailable, and its data is drawn instead', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });

    await inside(probe(page)).locator('#stop').click();

    // One heartbeat unanswered, about two seconds: dimmed, still there.
    await expect(probe(page)).toHaveAttribute('data-module', 'stale', { timeout: 6_000 });
    await expect(probe(page).locator('iframe')).toHaveCount(1);
    await shot(page, 61, 'module-stale');

    // Three in a row: given up on. The frame goes, and because the panel
    // declares data, Hlin draws that and says why.
    await expect(probe(page)).toHaveAttribute('data-module', 'fallback', { timeout: 10_000 });
    await expect(probe(page)).toHaveAttribute('data-module-cause', 'unreachable');
    await expect(probe(page).locator('iframe')).toHaveCount(0);
    await expect(probe(page).locator('.module-note')).toContainText('Drawn by Hlin');
    await expect(probe(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });
    await shot(page, 62, 'module-fallback');

    // A person can ask for the module back, which is the only way out of
    // `unavailable`: a new frame, a new handshake.
    await probe(page).locator('.retry-module').click();
    await expect(probe(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(probe(page).locator('iframe')).toHaveCount(1);
  });

  test('a module that never says ready is given up on, and its data is drawn instead', async ({
    page,
  }) => {
    await openSurface(page, layoutId);

    // Loaded, and silent: `loading` until the ready timeout says otherwise.
    await expect(silent(page)).toHaveAttribute('data-module', 'loading');
    await expect(silent(page).locator('iframe')).toHaveCount(1);
    await expect(inside(silent(page)).locator('body')).toContainText('never says it is ready');

    await expect(silent(page)).toHaveAttribute('data-module', 'fallback', { timeout: 20_000 });
    await expect(silent(page)).toHaveAttribute('data-module-cause', 'unreachable');
    await expect(silent(page).locator('iframe')).toHaveCount(0);
    await expect(silent(page)).toHaveAttribute('data-state', 'ready', { timeout: 30_000 });

    // The probe beside it was never affected.
    await expect(probe(page)).toHaveAttribute('data-module', 'ready');
  });
});
