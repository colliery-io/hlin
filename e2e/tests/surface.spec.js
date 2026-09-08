// Composing a surface, in a browser.
//
// The claim the product exists to make is that a person picks panels from
// platforms that know nothing about each other and arranges them into a view
// they authored. Everything else in this repository tests a piece of that. This
// tests the claim.
//
// Runs against a demo started with `angreal demo up`, and works in a layout of
// its own so it neither depends on nor disturbs whatever is already there.

const { test, expect } = require('@playwright/test');
const {
  shot,
  freshLayout,
  discardLayout,
  openSurface,
  panel,
  placementOf,
  dragBy,
  drewSomething,
  settled,
} = require("./helpers");

test.describe.configure({ mode: 'serial' });

test.describe('a person composes a surface', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    layoutId = await freshLayout(request, 'Browser test');
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('the surface opens, empty, and says so', async ({ page }) => {
    await openSurface(page, layoutId);

    await expect(page.locator('header.bar strong')).toHaveText('Hlin');
    await expect(page.locator('.bar .tagline')).toHaveText('Browser test');

    // The stream connects on its own; until it does the shell is not live, and
    // saying "reconnecting" forever would be a real defect.
    await expect(page.locator('.bar .link')).toHaveText('live');

    await expect(page.locator('.empty')).toContainText('Nothing on this surface yet');
    await expect(page.locator('section.panel')).toHaveCount(0);

    await shot(page, 1, 'empty-surface');
  });

  test('edit mode opens a picker listing both platforms', async ({ page, request }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    const catalog = page.locator('aside.catalog');
    await expect(catalog).toBeVisible();

    // Two platforms that know nothing about each other, offered side by side.
    // One would demonstrate nothing.
    const platforms = catalog.locator('section.platform h3');
    await expect(platforms).toHaveCount(2);
    await expect(platforms.nth(0)).toContainText('Orebank');
    await expect(platforms.nth(1)).toContainText('Stampmill');

    // Every panel both platforms offer, and no more. Counted from the catalog
    // rather than written down here: a hardcoded number asserts nothing about
    // the picker beyond how many panels the sample platform happened to have
    // on the day it was written, and fails the moment one is added.
    const catalogued = await (await request.get('/api/panels')).json();
    const offered = catalogued.reduce((total, platform) => total + platform.panels.length, 0);
    await expect(catalog.locator('button.offer')).toHaveCount(offered);

    await shot(page, 2, 'picker-open');
  });

  test('the search box narrows the picker to what was typed', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    const before = await page.locator('.catalog button.offer').count();
    await page.locator('.catalog .search').fill('throughput');

    // What narrowing means, rather than how many panels happen to match today:
    // fewer than everything, more than nothing, and every one of them matching
    // what was typed. The last of those is the assertion that would catch a
    // search box that had stopped filtering.
    const offers = page.locator('.catalog button.offer');
    await expect(offers).not.toHaveCount(before);
    expect(await offers.count()).toBeGreaterThan(0);

    for (const text of await offers.allTextContents()) {
      expect(text.toLowerCase()).toContain('throughput');
    }

    // A search that matches nothing should say nothing, not everything.
    await page.locator('.catalog .search').fill('nothing named this');
    await expect(offers).toHaveCount(0);
    await page.locator('.catalog .search').fill('throughput');

    await shot(page, 3, 'picker-searched');
  });

  test('adding panels from both platforms puts them on the grid, drawn', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    // One from each platform, so what is on screen could not have come from a
    // single manifest.
    await page.locator('.catalog section.platform', { hasText: 'Orebank' })
      .locator('button.offer', { hasText: 'Throughput' }).first().click();
    await expect(page.locator('section.panel')).toHaveCount(1);

    await page.locator('.catalog section.platform', { hasText: 'Stampmill' })
      .locator('button.offer', { hasText: 'Queue depth' }).first().click();
    await expect(page.locator('section.panel')).toHaveCount(2);

    // Each panel gets an identity from the shell and then a frame from its
    // platform. Waiting for the drawn state is waiting for the whole path:
    // layout written, surface rebuilt, platform fetched, frame delivered.
    await expect(page.locator('section.panel[data-state="ready"]')).toHaveCount(2, {
      timeout: 60_000,
    });

    // Drawn, not merely present: an empty box would satisfy a count.
    await expect.poll(() => drewSomething(page.locator("section.panel").first()), { timeout: 30_000 }).toBe(true);

    const panels = page.locator('section.panel');
    expect(await placementOf(panels.nth(0))).toMatchObject({ x: 0, y: 0 });
    expect(await placementOf(panels.nth(1))).toMatchObject({ x: 4, y: 0 });

    await settled(page);
    await shot(page, 4, 'two-panels-drawn');
  });

  test('dragging a panel by its handle moves it', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();
    await expect(page.locator('section.panel')).toHaveCount(2);

    const second = page.locator('section.panel').nth(1);
    const instance = await second.getAttribute('data-instance');
    const before = await placementOf(second);

    // Far enough right to cross several columns, and down a row.
    await dragBy(page, second.locator('.grip'), 320, 90);

    const moved = panel(page, instance);
    await expect
      .poll(async () => (await placementOf(moved)).x, { timeout: 20_000 })
      .not.toBe(before.x);

    const after = await placementOf(moved);
    expect(after.x).toBeGreaterThan(before.x);

    // And it never stopped showing its data. A write used to drop the surface,
    // so every panel refetched from nothing and the whole grid blanked for a
    // move that changed no data at all. The shell now reconciles in place, so
    // the states seen during and after a drag are `ready` throughout.
    await expect(moved).toHaveAttribute("data-state", "ready");
    await drewSomething(moved);

    await settled(page);
    await shot(page, 5, 'panel-dragged');
  });

  test('resizing from the corner changes the panel span', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    const first = page.locator('section.panel').first();
    const instance = await first.getAttribute('data-instance');
    const before = await placementOf(first);

    await dragBy(page, first.locator('.corner'), 240, 120);

    const resized = panel(page, instance);
    await expect
      .poll(async () => (await placementOf(resized)).w, { timeout: 20_000 })
      .toBeGreaterThan(before.w);

    const after = await placementOf(resized);
    expect(after.h).toBeGreaterThan(before.h);
    expect(after.x + after.w).toBeLessThanOrEqual(12);

    await settled(page);
    await shot(page, 6, 'panel-resized');
  });

  test('the arrangement survives a reload', async ({ page }) => {
    await openSurface(page, layoutId);
    const before = [];
    for (const one of await page.locator('section.panel').all()) {
      before.push(await placementOf(one));
    }
    expect(before.length).toBe(2);

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await expect(page.locator('section.panel')).toHaveCount(2);

    const after = [];
    for (const one of await page.locator('section.panel').all()) {
      after.push(await placementOf(one));
    }

    // The whole point of writing the layout: what a person arranged is what
    // they come back to.
    expect(after).toEqual(before);

    await expect(page.locator('section.panel[data-state="ready"]')).toHaveCount(2, {
      timeout: 60_000,
    });
    await shot(page, 7, 'reloaded');
  });

  test('renaming a panel keeps the name, and clearing it gives the platform back', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    const first = page.locator('section.panel').first();
    const rename = first.locator('input.rename');
    const original = await rename.inputValue();

    // The write is what makes a rename survive, and it is in flight when the
    // box loses focus. Reloading without waiting for it tests how fast this
    // machine is, not whether the shell stored anything.
    const written = () =>
      page.waitForResponse(
        (answer) =>
          answer.url().includes('/api/layouts/') && answer.request().method() === 'PUT',
      );

    await rename.fill('Overnight ingest');
    await Promise.all([written(), rename.blur()]);

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await page.locator('button.mode').click();
    await expect(page.locator('section.panel').first().locator('input.rename'))
      .toHaveValue('Overnight ingest');

    await shot(page, 8, 'panel-renamed');

    const cleared = page.locator('section.panel').first().locator('input.rename');
    await cleared.fill('');
    await Promise.all([written(), cleared.blur()]);

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await page.locator('button.mode').click();
    await expect(page.locator('section.panel').first().locator('input.rename'))
      .toHaveValue(original, {
        // Back to whatever its platform calls it, not to a blank heading.
      });
  });

  test('switching a kind changes how the panel is drawn', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();

    // The timeseries panel: its envelope accepts more than one rendering, which
    // is the whole reason a viewer is offered the choice.
    const series = page.locator('section.panel[data-panel*="throughput"]').first();
    await expect(series).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });

    const choices = series.locator('select.kind');
    const options = await choices.locator('option').allTextContents();
    expect(options.length).toBeGreaterThan(2);

    // A table is a `<table>` in any pack worth the name, so this one selector
    // is safe to assert on. What a sparkline looks like is entirely the pack's
    // business, so that half is asserted as "it changed" instead.
    await choices.selectOption('table');
    await expect(series.locator('table')).toBeVisible({ timeout: 30_000 });

    await shot(page, 9, 'kind-switched-to-table');

    const drawn = await series.innerHTML();

    await choices.selectOption('sparkline');
    await expect(series.locator('table')).toHaveCount(0, { timeout: 30_000 });
    await expect.poll(async () => (await series.innerHTML()) !== drawn).toBe(true);

    // Changing a kind writes the layout, and every write drops the surface, so
    // the panel is briefly a skeleton while the shell rebuilds and refetches.
    // Waiting for `ready` is waiting for that to settle; asserting before it
    // does is asserting about a loading state.
    await expect(series).toHaveAttribute('data-state', 'ready', { timeout: 60_000 });
    await expect.poll(() => drewSomething(series), { timeout: 30_000 }).toBe(true);

    await settled(page);
    await shot(page, 10, 'kind-switched-to-sparkline');
  });

  test('the time picker marks a preset and settles', async ({ page }) => {
    await openSurface(page, layoutId);

    const presets = page.locator('.picker > button');
    await expect(presets).toHaveCount(4);

    await presets.filter({ hasText: '6h' }).click();
    await expect(presets.filter({ hasText: '6h' })).toHaveClass(/active/);

    // The indicator says "applying" until the shell acknowledges the
    // generation. That it settles is what proves the acknowledgement arrives;
    // a browser that never cleared it would look permanently behind.
    await expect(page.locator('.bar .applied')).toHaveText('applied', { timeout: 30_000 });

    await expect(page.locator('section.panel[data-state="ready"]')).toHaveCount(2, {
      timeout: 60_000,
    });

    await settled(page);
    await shot(page, 11, 'time-range-applied');
  });

  test('a custom range is refused until it reads as a range', async ({ page }) => {
    await openSurface(page, layoutId);

    const apply = page.locator('.custom button');
    await expect(apply).toBeDisabled();

    const [from, to] = await page.locator('.custom input').all();

    // Backwards on purpose: a range that ends before it starts is not a range,
    // and the shell should never be sent one.
    await from.fill('2026-09-07T12:00');
    await to.fill('2026-09-07T06:00');
    await expect(apply).toBeDisabled();

    await to.fill('2026-09-07T18:00');
    await expect(apply).toBeEnabled();
    await apply.click();

    await expect(page.locator('.bar .applied')).toHaveText('applied', { timeout: 30_000 });
    await settled(page);
    await shot(page, 12, 'custom-range');
  });

  test('removing a panel takes it off the grid for good', async ({ page }) => {
    await openSurface(page, layoutId);
    await page.locator('button.mode').click();
    await expect(page.locator('section.panel')).toHaveCount(2);

    await page.locator('section.panel').first().locator('button.drop').click();
    await expect(page.locator('section.panel')).toHaveCount(1);

    // The count above is the draft, which changes the instant the button is
    // pressed; the write follows. Reloading through it cancels it, and the
    // panel comes back — which is what "for good" is here to catch, and what
    // this test used to do to itself under a busy shell.
    await settled(page);

    await page.reload();
    await page.locator('header.bar').waitFor({ state: 'visible' });
    await expect(page.locator('section.panel')).toHaveCount(1);

    // What is left rises, rather than leaving a band of nothing where the
    // removed panel was.
    expect((await placementOf(page.locator('section.panel').first())).y).toBe(0);

    await shot(page, 13, 'panel-removed');
  });
});
