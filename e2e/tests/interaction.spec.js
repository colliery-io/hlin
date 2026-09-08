// A component that talks back.
//
// The other suites assert what reaches the browser. This asserts the return
// path: a person clicks something a *design pack* drew, and the shell asks a
// platform a different question because of it.
//
// The claim being tested is a round trip, so a round trip is what is checked.
// Asserting that a click emitted an intent would pass with the shell ignoring
// it; asserting that the drawn series changed is the only version that cannot.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, drewSomething, settled } = require('./helpers');

/** The panel that asks a pack to draw its own filter. */
const FACETED = 'throughput-faceted';

test.describe('a component that talks back', () => {
  let layout;
  let platform;

  test.beforeEach(async ({ request }) => {
    const catalog = await (await request.get('/api/panels')).json();
    const found = catalog
      .flatMap((entry) => entry.panels.map((panel) => ({ ...panel, platform: entry.id })))
      .find((panel) => panel.key === FACETED);

    test.skip(!found, 'no platform here offers a faceted panel');
    platform = found.platform;

    layout = await freshLayout(request, 'Filtered in place');
    await request.put(`/api/layouts/${layout}`, {
      data: {
        title: 'Filtered in place',
        visibility: 'personal',
        panels: [
          {
            platform_id: platform,
            panel_key: FACETED,
            selections: {},
            position: { x: 0, y: 0, w: 8, h: 5 },
          },
        ],
      },
    });
  });

  test.afterEach(async ({ request }) => {
    await discardLayout(request, layout);
    layout = null;
  });

  test('the shell fetches what a platform will accept, so the pack can offer it', async ({
    request,
  }) => {
    // The half that flows in. Before this route existed the control was a text
    // box, because nothing had ever read the options endpoint a platform
    // declared.
    const answer = await request.get(`/api/options/${platform}/${FACETED}/cluster`);
    expect(answer.ok()).toBeTruthy();

    const { choices } = await answer.json();
    expect(choices.length).toBeGreaterThan(1);
    for (const choice of choices) {
      expect(choice.value).toBeTruthy();
    }
  });

  test('will not fetch a URL it was handed', async ({ request }) => {
    // The route takes a platform, a panel and a control, and looks the address
    // up in the manifest it already holds. Naming a panel that does not exist
    // is refused rather than answered, which is what keeps this from being a
    // proxy for the shell's own credential.
    const invented = await request.get(`/api/options/${platform}/not-a-panel/cluster`);
    expect(invented.status()).toBe(404);

    const nowhere = await request.get('/api/options/not-a-platform/x/y');
    expect(nowhere.status()).toBe(404);
  });

  test('clicking the filter a pack drew changes what the platform is asked', async ({ page }) => {
    await page.goto(`/s/${layout}?pack=aurora`);

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    // Drawn by the pack, not the chrome: these are inside the panel body, and
    // the chrome's control for this panel only appears in edit mode.
    const facets = panel.locator('.cl-hlin-facet');
    await expect(facets.first()).toBeVisible();
    expect(await facets.count()).toBeGreaterThan(1);

    // What is on screen now, so the change can be seen rather than assumed.
    const before = await panel.locator('svg').innerHTML();

    await shot(page, 90, 'facets-before');

    // Deliberately not the first choice. A platform given no selection applies
    // its own default, and this one's default is its first choice — so clicking
    // that asks for what is already on screen, and an assertion that the drawing
    // changed can only pass by accident of the time window having moved. Picking
    // a different one is what makes this test about the round trip.
    await facets.nth(1).click();

    // The round trip: the click emits, the shell writes the selection, sends
    // new parameters, the platform answers a different question and the frame
    // redraws. Polled because every step of that is asynchronous.
    await expect
      .poll(async () => panel.locator('svg').innerHTML(), { timeout: 20_000 })
      .not.toBe(before);

    // And the pack shows which one is set, from the state the shell sent back
    // rather than from anything it remembered locally.
    await expect(panel.locator('.cl-hlin-facet--on')).toHaveCount(1);

    await shot(page, 91, 'facets-after');
  });

  test('survives a reload, because the choice was stored and not merely drawn', async ({ page }) => {
    await page.goto(`/s/${layout}?pack=aurora`);

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    // Not the first, for the reason in the test above.
    const facets = panel.locator('.cl-hlin-facet');
    await facets.nth(1).click();
    await expect(panel.locator('.cl-hlin-facet--on')).toHaveCount(1);

    const chosen = await panel.locator('.cl-hlin-facet--on').textContent();

    // The choice is a layout write, and reloading through one cancels it.
    await settled(page);

    await page.reload();
    const again = page.locator('[data-panel]').first();
    await expect(again).toHaveAttribute('data-state', 'ready');
    await expect(again.locator('.cl-hlin-facet--on')).toHaveText(chosen);
  });
});

test.describe('a component that moves the whole surface', () => {
  let layout;
  let platform;

  test.beforeEach(async ({ request }) => {
    const catalog = await (await request.get('/api/panels')).json();
    const found = catalog
      .flatMap((entry) => entry.panels.map((panel) => ({ ...panel, platform: entry.id })))
      .find((panel) => panel.component === 'aurora.brush');

    test.skip(!found, 'no platform here offers a brushable panel');
    platform = found.platform;

    layout = await freshLayout(request, 'Brushed');
    await request.put(`/api/layouts/${layout}`, {
      data: {
        title: 'Brushed',
        visibility: 'personal',
        panels: [
          {
            platform_id: platform,
            panel_key: found.key,
            selections: {},
            position: { x: 0, y: 0, w: 8, h: 5 },
          },
        ],
      },
    });
  });

  test.afterEach(async ({ request }) => {
    await discardLayout(request, layout);
    layout = null;
  });

  test('dragging across a chart moves the surface to that window', async ({ page }) => {
    // `Intent::Select` changes one panel. `Intent::Range` changes the surface,
    // which is the half that had no component emitting it and therefore no
    // browser ever exercising the handler.
    await page.goto(`/s/${layout}?pack=aurora`);

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    const brush = panel.locator('.hlin-brush');
    await expect(brush).toBeVisible();

    // A preset is in force to begin with. That is what a brush has to displace,
    // and asserting it first is what makes the assertion afterwards mean
    // something.
    await expect(page.locator('.bar button.active')).toHaveCount(1);

    const box = await brush.boundingBox();
    expect(box, 'the chart must be on screen to drag across').toBeTruthy();

    const y = box.y + box.height / 2;
    await page.mouse.move(box.x + box.width * 0.25, y);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.5, y, { steps: 8 });
    await page.mouse.move(box.x + box.width * 0.75, y, { steps: 8 });
    await page.mouse.up();

    // No preset is in force any more: the surface is on a range somebody
    // dragged, and the picker says so rather than still claiming an hour.
    await expect(page.locator('.bar button.active')).toHaveCount(0);

    // And the panel is still drawing, on the new window. A range that emptied
    // the surface would be worse than one that never applied.
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    await shot(page, 92, 'brushed-range');
  });

  test('a click is not a brush', async ({ page }) => {
    // Below a small fraction of the width this is somebody tapping the chart,
    // and moving the whole surface for that would be a surprise rather than a
    // feature.
    await page.goto(`/s/${layout}?pack=aurora`);

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    const brush = panel.locator('.hlin-brush');
    await expect(brush).toBeVisible();

    await expect(page.locator('.bar button.active')).toHaveCount(1);

    await brush.click();

    // Still exactly where it was.
    await expect(page.locator('.bar button.active')).toHaveCount(1);
  });
});
