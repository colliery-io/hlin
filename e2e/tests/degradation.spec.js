// What a person sees when something goes wrong.
//
// The walkthrough already asserts that the shell reaches the right states. This
// asserts that those states reach a person's eyes: that a panel says what
// happened to it, in words, rather than sitting there blank or showing a number
// from twenty minutes ago.
//
// The stream is severed by refusing the request rather than by killing a
// platform, so a demo somebody is looking at in another window is undisturbed.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface, drewSomething } = require("./helpers");

test.describe.configure({ mode: 'serial' });

test.describe('a surface degrades where a person can see it', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    layoutId = await freshLayout(request, 'Degradation test');

    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    current.panels = [
      {
        platform_id: 'orebank',
        panel_key: 'records-per-second',
        position: { x: 0, y: 0, w: 6, h: 3 },
      },
      {
        platform_id: 'stampmill',
        panel_key: 'throughput',
        position: { x: 6, y: 0, w: 6, h: 3 },
      },
      {
        platform_id: 'orebank',
        panel_key: 'queue-depth',
        position: { x: 0, y: 3, w: 6, h: 3 },
      },
    ];
    const written = await request.put(`/api/layouts/${layoutId}`, { data: current });
    expect(written.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('panels from two platforms all reach a drawn state', async ({ page }) => {
    await openSurface(page, layoutId);
    await expect(page.locator('section.panel')).toHaveCount(3);

    // Ready, not merely "not waiting". A panel stuck in `loading` would satisfy
    // a weaker assertion while showing a person nothing at all.
    await expect(page.locator('section.panel[data-state="ready"]')).toHaveCount(3, {
      timeout: 90_000,
    });

    // And each one drew something rather than being an empty box, asked in a
    // way that does not care which design pack is drawing.
    for (const one of await page.locator('section.panel').all()) {
      await expect.poll(() => drewSomething(one), { timeout: 30_000 }).toBe(true);
    }

    // Nothing is left saying it is applying a change nobody made.
    await expect(page.locator('.bar .applied')).toHaveText('applied', { timeout: 30_000 });

    await shot(page, 20, 'three-panels-drawn');
  });

  test('a stream that will not open is reported as the shell, not a platform', async ({
    page,
    context,
    request,
  }) => {
    // How long the browser waits before calling the shell unreachable is the
    // shell's own setting now, derived from its staleness. Asking beats
    // guessing: a hardcoded ninety seconds raced a ninety-second grace and
    // could never win, and any future tuning would have broken it again.
    const { stream_loss_grace_seconds: grace } = await (await request.get('/api/config')).json();
    const past_the_grace = (grace + 30) * 1000;
    // Refused before the page loads, so the `EventSource` fails on connect.
    // Blocking an already-open stream does not sever it: Chromium leaves an
    // established connection alone, which is why this reloads into the refusal
    // rather than pulling the rug out from under a live page.
    await context.route('**/api/stream/**', (route) => route.abort());

    await openSurface(page, layoutId);
    await expect(page.locator('section.panel')).toHaveCount(3);

    await expect(page.locator('.bar .link')).toHaveText('reconnecting', { timeout: 30_000 });

    await shot(page, 21, 'stream-lost');

    // Past the grace interval the panels are not merely aged: nothing is
    // coming. The unreachable party is Hlin itself, and a person must not be
    // sent to go and look at a platform that is answering perfectly well.
    await expect(page.locator('section.panel[data-state="unavailable"]')).toHaveCount(3, {
      timeout: past_the_grace,
    });
    // The pack drew a placeholder rather than leaving the panel blank. What
    // that looks like is the pack's business, so this asks whether anything is
    // there rather than naming a class.
    await expect.poll(() => drewSomething(page.locator("section.panel").first()), { timeout: 30_000 }).toBe(true);

    // And the party named is Hlin. This line is the shell's own markup, not a
    // pack's, which is why it can be asserted on by text: a pack is handed a
    // cause and cannot know who failed, so it must never be the one naming a
    // platform here.
    const said = await page.locator('section.panel .detail').first().textContent();
    expect(said).toMatch(/Hlin is not responding/i);

    await shot(page, 22, 'stream-lost-unreachable');

    await context.unroute('**/api/stream/**');
  });

  test('the panels come back once the stream can open again', async ({ page }) => {
    await openSurface(page, layoutId);

    await expect(page.locator('.bar .link')).toHaveText('live', { timeout: 60_000 });
    await expect(page.locator('section.panel[data-state="ready"]')).toHaveCount(3, {
      timeout: 90_000,
    });

    await shot(page, 23, 'stream-restored');
  });
});
