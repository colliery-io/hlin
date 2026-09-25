// How long a module takes to load (HLIN-S-0007, NFR-1.1).
//
// Two numbers, each the median of three runs:
//
// - **A cached module is `ready` within 1 second**: the annotations module
//   (Leptos, built with the SDK), its files already in the browser's cache,
//   from the moment the page puts its frame in the document to the moment the
//   page hears `ready`, and to the module's first content: the platform's
//   answer, drawn.
// - **Six modules from three platforms are interactive within 3 seconds
//   cold**: a fresh browser with nothing cached, from navigation to the last
//   of six modules drawing its first content. Two annotations panels from each
//   of the three sample platforms, so three separate module downloads, each
//   compiled and instantiated twice.
//
// Times are the epoch clock, which a frame and its page share: the page
// records when each frame went in and when each panel said `ready`, and the
// module stamps `data-drawn-at` on its body when it first draws an answer.
//
// The limits are asserted whichever build is up: a debug build meets them
// too, with room, on the machine they were first measured on. What a
// deployment serves is `angreal demo up --release`, which records that it was
// a release build in demo/state/build.json, and each number is printed with
// the build it was measured on. The numbers recorded against the requirement
// are the release build's (HLIN-T-0071).

const fs = require('fs');
const path = require('path');
const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, openSurface } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORMS = ['orebank', 'stampmill', 'smelter'];
const RUNS = 3;

/** Whether the demo under test was built optimised. */
function releaseBuild() {
  try {
    const build = JSON.parse(
      fs.readFileSync(path.join(__dirname, '..', '..', 'demo', 'state', 'build.json'), 'utf8'),
    );
    return build.release === true;
  } catch {
    return false;
  }
}

/** Record, on the page, when each frame went in and each panel said `ready`. */
function recordTimes() {
  if (window !== window.top) {
    return;
  }
  const times = { inserted: {}, ready: {} };
  window.__hlinTimes = times;
  new MutationObserver((records) => {
    const at = Date.now();
    for (const record of records) {
      if (record.type === 'childList') {
        for (const node of record.addedNodes) {
          if (node.nodeName === 'IFRAME') {
            const panel = node.closest('section.panel');
            const key = panel ? panel.dataset.instance : node.src;
            times.inserted[key] ??= at;
          }
        }
      } else if (record.target.dataset.module === 'ready') {
        times.ready[record.target.dataset.instance] ??= at;
      }
    }
  }).observe(document, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ['data-module'],
  });
}

/** Every module panel's times, once every one has drawn its first content. */
async function measure(page, count) {
  const panels = page.locator('section.panel[data-module]');
  await expect(panels).toHaveCount(count, { timeout: 30_000 });
  const drawn = [];
  for (let i = 0; i < count; i += 1) {
    const panel = panels.nth(i);
    await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 30_000 });
    const body = panel.frameLocator('iframe').locator('body[data-drawn-at]');
    await expect(body).toHaveCount(1, { timeout: 30_000 });
    drawn.push({
      instance: await panel.getAttribute('data-instance'),
      drawn: Number(await body.getAttribute('data-drawn-at')),
    });
  }
  const { times, origin } = await page.evaluate(() => ({
    times: window.__hlinTimes,
    origin: performance.timeOrigin,
  }));
  return drawn.map(({ instance, drawn: at }) => ({
    inserted: times.inserted[instance] - origin,
    ready: times.ready[instance] - origin,
    drawn: at - origin,
  }));
}

const median = (values) => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const round = (value) => Math.round(value);

async function layoutOf(request, title, panels) {
  const id = await freshLayout(request, title);
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.panels = panels.map(([platform_id, panel_key], i) => ({
    platform_id,
    panel_key,
    position: { x: (i % 3) * 4, y: Math.floor(i / 3) * 5, w: 4, h: 5 },
  }));
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok()).toBeTruthy();
  return id;
}

test.describe('how long modules take to load', () => {
  const release = releaseBuild();
  let single;
  let six;

  test.beforeAll(async ({ request }) => {
    single = await layoutOf(request, 'One cached module', [['orebank', 'annotations']]);
    six = await layoutOf(
      request,
      'Six modules, three platforms',
      PLATFORMS.flatMap((platform) => [
        [platform, 'annotations'],
        [platform, 'annotations'],
      ]),
    );
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, single);
    await discardLayout(request, six);
  });

  test('a cached module says ready, and draws, within a second', async ({ browser }) => {
    const baseURL = test.info().project.use.baseURL;
    const context = await browser.newContext({ baseURL });
    await context.addInitScript(recordTimes);
    const page = await context.newPage();
    // Once to fill the cache; then the runs, each a reload.
    await openSurface(page, single);
    await measure(page, 1);

    const runs = [];
    for (let run = 0; run < RUNS; run += 1) {
      await page.reload();
      await page.locator('header.bar').waitFor();
      const [one] = await measure(page, 1);
      runs.push({ ready: one.ready - one.inserted, drawn: one.drawn - one.inserted });
    }
    await context.close();

    const ready = median(runs.map((run) => run.ready));
    const drawn = median(runs.map((run) => run.drawn));
    const said =
      `cached module (${release ? 'release' : 'debug'}): ready ${round(ready)} ms, ` +
      `first content ${round(drawn)} ms after its frame went in ` +
      `(runs: ${runs.map((run) => `${round(run.ready)}/${round(run.drawn)}`).join(', ')})`;
    console.log(said);
    test.info().annotations.push({ type: 'NFR-1.1', description: said });

    expect(ready).toBeLessThan(1_000);
    expect(drawn).toBeLessThan(1_000);
  });

  test('six modules from three platforms are interactive within three seconds, cold', async ({
    browser,
  }) => {
    const baseURL = test.info().project.use.baseURL;
    const runs = [];
    for (let run = 0; run < RUNS; run += 1) {
      // A fresh browser context each time: nothing cached.
      const context = await browser.newContext({ baseURL });
      await context.addInitScript(recordTimes);
      const page = await context.newPage();
      await openSurface(page, six);
      const panels = await measure(page, 6);
      await context.close();
      runs.push({
        ready: Math.max(...panels.map((one) => one.ready)),
        drawn: Math.max(...panels.map((one) => one.drawn)),
      });
    }

    const ready = median(runs.map((run) => run.ready));
    const drawn = median(runs.map((run) => run.drawn));
    const said =
      `six modules, three platforms, cold (${release ? 'release' : 'debug'}): last ready ` +
      `${round(ready)} ms, last first content ${round(drawn)} ms after navigation ` +
      `(runs: ${runs.map((run) => `${round(run.ready)}/${round(run.drawn)}`).join(', ')})`;
    console.log(said);
    test.info().annotations.push({ type: 'NFR-1.1', description: said });

    expect(drawn).toBeLessThan(3_000);
  });
});
