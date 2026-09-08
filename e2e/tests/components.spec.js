// A panel drawn by a component Hlin has no word for.
//
// The claim under test is not "Aurora draws a graph" — that is Aurora's
// business and Aurora's to test. It is that the *seam* works: a platform names
// a component in its manifest, the shell forwards a string it never reads, and
// the pack either draws it or declines without the panel suffering for it.
//
// So both halves are asserted, and the second is the important one. A feature
// that only works when the pack cooperates would be a feature no platform could
// safely use, because a platform cannot know which design system a shell it
// registers against is running.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, drewSomething } = require('./helpers');

/** A panel the sample platforms declare a component for, and its fallback. */
const ASKING = { key: 'pipeline', kind: 'table' };

test.describe('a panel that asks for a component by name', () => {
  let layout;

  /** The component name the platform actually declared, read from the catalog. */
  let component;

  test.beforeEach(async ({ request }) => {
    layout = await freshLayout(request, 'Asking for a component');

    const catalog = await (await request.get('/api/panels')).json();
    const asking = catalog
      .flatMap((platform) => platform.panels.map((panel) => ({ ...panel, platform: platform.id })))
      .find((panel) => panel.key === ASKING.key && panel.component);

    test.skip(!asking, 'no platform here declares a component');

    // The manifest's word reaches the browser intact. If this drifts, the rest
    // of the test would still pass while forwarding nothing.
    expect(asking.component).toBeTruthy();
    expect(asking.kind).toBe(ASKING.kind);
    component = asking.component;

    await request.put(`/api/layouts/${layout}`, {
      data: {
        title: 'Asking for a component',
        visibility: 'personal',
        panels: [
          {
            platform_id: asking.platform,
            panel_key: asking.key,
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

  test('is drawn by the pack that offers it', async ({ page }) => {
    await page.goto(`/s/${layout}?pack=aurora`);

    // `?pack=` is honoured only by a frontend built with more than one pack.
    // Where the shell serves a single-pack frontend the address does nothing,
    // the panel falls back to its declared kind, and this would fail as though
    // the seam were broken — which is a statement about which demo is running,
    // not about the code. The mounted pack says what it answers to, so ask it.
    const offered = await page.locator('main.grid').getAttribute('data-offers');
    test.skip(
      !offered.split(' ').includes(component),
      `this frontend's pack does not offer \`${component}\`, so nothing here can draw it; ` +
        'run the demo with `--with gallery`',
    );

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    // Asserted structurally rather than by class name: the point is that a
    // component the vocabulary cannot express got drawn, and an SVG with edges
    // in it is not something any of the six kinds produces for records.v1.
    await expect(panel.locator('svg')).toBeVisible();

    await shot(page, 98, 'component-drawn');
  });

  test('falls back to its declared kind where the pack has no such component', async ({ page }) => {
    await page.goto(`/s/${layout}?pack=demo`);

    // The mirror of the guard above: on a frontend built only from a pack that
    // *does* offer the component there is no declining pack to reach, and this
    // would fail for the same irrelevant reason.
    const offered = await page.locator('main.grid').getAttribute('data-offers');
    test.skip(
      offered.split(' ').includes(component),
      `every pack in this frontend offers \`${component}\`, so none of them can decline it; ` +
        'run the demo with `--with gallery`',
    );

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready');
    await drewSomething(panel);

    // The declared kind is `table`, and a table is what a pack without the
    // component must produce. Nothing on screen should suggest a failure: this
    // is the ordinary panel the platform also promised.
    await expect(panel.locator('table')).toBeVisible();
    await expect(panel).not.toContainText(/could not|unavailable|unknown/i);

    await shot(page, 99, 'component-declined');
  });
});
