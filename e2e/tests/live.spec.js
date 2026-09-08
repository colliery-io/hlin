// Whether a surface configured for live data actually arrives live.
//
// The rest of the suite asserts that the right thing is on screen. This asserts
// that it keeps changing, which is a different property and the one a viewer
// judges a dashboard by: a panel that fetched once and stopped looks identical
// to one refreshing eight times a second in any single screenshot.
//
// Skipped unless the panels under test can actually deliver it. Since
// HLIN-A-0009 a panel declares its own cadence, so the ordinary demo draws these
// live without a special configuration — but a shell whose panels declare
// nothing still cannot redraw several times a second, and asserting that it does
// would be asserting something about the configuration rather than the code.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout } = require('./helpers');

/** How long to watch. Long enough for a slow refresh to be distinguishable. */
const WATCH_MS = 6_000;

/** Below this, whatever the shell is doing, a person would not call it live. */
const AT_LEAST_HZ = 3;

test.describe('a live surface', () => {
  let layout;

  test.afterEach(async ({ request }) => {
    await discardLayout(request, layout);
    layout = null;
  });

  test('keeps redrawing while nobody touches it', async ({ page, request }) => {
    layout = await freshLayout(request, 'Live, watched');

    const catalog = await (await request.get('/api/panels')).json();
    const live = catalog
      .flatMap((platform) =>
        platform.panels.map((panel) => ({
          platform: platform.id,
          key: panel.key,
          refresh_ms: panel.refresh_ms,
        })),
      )
      .filter((panel) => panel.key.startsWith('live-'));

    test.skip(live.length === 0, 'this shell offers no live panels');

    // The *effective* cadence, which is the panel's own where it declares one
    // and the shell's otherwise (HLIN-A-0009). Reading only the shell's would
    // now skip on a shell that draws these panels perfectly live — the whole
    // point of the panel-declared hint is that one number no longer describes
    // every panel.
    const { refresh_ms: shellRefresh } = await (await request.get('/api/config')).json();
    const declared = live
      .map((panel) => panel.refresh_ms)
      .filter((ms) => typeof ms === 'number');
    const refresh = declared.length ? Math.min(...declared) : (shellRefresh ?? 30_000);
    const capable = 1000 / Math.max(refresh, 1);
    test.skip(
      capable < AT_LEAST_HZ,
      `this shell refreshes every ${refresh}ms, so it cannot redraw ${AT_LEAST_HZ} times a second; ` +
        'run the demo with `--with live`',
    );

    await request.put(`/api/layouts/${layout}`, {
      data: {
        title: 'Live, watched',
        visibility: 'personal',
        panels: live.slice(0, 4).map((panel, index) => ({
          platform_id: panel.platform,
          panel_key: panel.key,
          selections: {},
          position: { x: (index % 2) * 6, y: Math.floor(index / 2) * 4, w: 6, h: 4 },
        })),
      },
    });

    await page.goto(`/s/${layout}`);
    await expect(page.locator('[data-state="ready"]').first()).toBeVisible();

    // Sample what the panels say, rather than trusting the network: the
    // question is whether new data reaches the DOM, and a frame the frontend
    // received but did not render is a failure this should catch.
    const readings = await page.evaluate(async (windowMs) => {
      const seen = new Set();
      const started = performance.now();
      let samples = 0;

      while (performance.now() - started < windowMs) {
        const text = Array.from(document.querySelectorAll('[data-panel]'))
          .map((panel) => panel.textContent)
          .join('|');
        seen.add(text);
        samples += 1;
        await new Promise((resume) => requestAnimationFrame(resume));
      }

      return { distinct: seen.size, samples, elapsed: performance.now() - started };
    }, WATCH_MS);

    // requestAnimationFrame samples at about 60Hz, comfortably above any
    // refresh rate worth calling live, so the count of distinct renderings is
    // a floor on how many times the panels actually changed.
    const hz = readings.distinct / (readings.elapsed / 1000);
    console.log(
      `${readings.distinct} distinct renderings from ${readings.samples} samples ` +
        `in ${Math.round(readings.elapsed)}ms — ${hz.toFixed(1)}Hz`,
    );

    expect(hz).toBeGreaterThan(AT_LEAST_HZ);
    await shot(page, 97, 'live-surface');
  });
});
