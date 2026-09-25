// A module reading a response as it arrives, as fast as it asks and no faster
// (HLIN-S-0007, *Streaming*).
//
// The module is the sample platform's hand-written probe
// (crates/hlin-sample-platform/ui/probe), and what it streams is its
// platform's feed (crates/hlin-sample-platform/src/feed.rs): numbered ticks as
// server-sent events, at a pace the test names. The probe keeps everything a
// stream heard — each `chunk`'s `seq` and size, the `end` and why — and lets a
// test grant credit by hand, so a test can hold the module still and see the
// page hold still too. The feed keeps count of what it wrote and whether its
// connection is still open, so the far end can be asked as well: that a
// module which stopped pulling stopped the platform, and that a stream
// cancelled or unmounted reached it as a dropped connection.
//
// Pack-agnostic: every assertion is on the shell's own markup or inside the
// module's frame, so it runs whichever demo flavour is up.

const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, openSurface, settled, shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORM = 'orebank';

const byKey = (page, key) => page.locator(`section.panel[data-panel="${PLATFORM}/${key}"]`);

/** The probe's document in a panel, once it has said `ready`. */
async function probeIn(page, key) {
  const panel = byKey(page, key);
  await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
  const handle = await panel.locator('iframe').elementHandle();
  return handle.contentFrame();
}

/** A name for one feed, so its counts are this test's own. */
let feeds = 0;
const feedName = (what) => `${what}-${Date.now()}-${(feeds += 1)}`;

const open = (frame, query, options) =>
  frame.evaluate(([q, o]) => window.probeStreams.open(q, o), [query, options]);
const stream = (frame, id) => frame.evaluate((re) => window.probeStreams.get(re), id);
const pull = (frame, id, bytes) =>
  frame.evaluate(([re, n]) => window.probeStreams.pull(re, n), [id, bytes]);
const cancel = (frame, id) => frame.evaluate((re) => window.probeStreams.cancel(re), id);
const counts = (frame, name) => frame.evaluate((feed) => window.probeStreams.counts(feed), name);

test.describe('a module streaming a response through the shell', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    layoutId = await freshLayout(request, 'Module streams');
    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    // Three probes side by side, all in view, for the page-wide cap.
    current.panels = ['module-probe', 'module-only', 'module-context'].map((key, i) => ({
      platform_id: PLATFORM,
      panel_key: key,
      position: { x: i * 4, y: 0, w: 4, h: 10 },
    }));
    const written = await request.put(`/api/layouts/${layoutId}`, { data: current });
    expect(written.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('a streamed read arrives in order and then ends; a streamed write is refused', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const probe = await probeIn(page, 'module-probe');

    const id = await open(probe, `id=${feedName('ordered')}&every_ms=20&count=15`, {
      credit: 1 << 20,
      auto: true,
    });
    await expect.poll(async () => (await stream(probe, id)).ended, { timeout: 20_000 }).toBe(true);

    const heard = await stream(probe, id);
    expect(heard.status).toBe(200);
    expect(heard.streaming).toBe(true);
    expect(heard.refusal).toBeNull();
    expect(heard.error).toBeNull();
    // Every `chunk` in order, none missing, and the platform's ticks in order
    // within them.
    expect(heard.seqs).toEqual(heard.seqs.map((_, i) => i));
    const ticks = heard.text.match(/tick \d+/g).map((tick) => Number(tick.slice(5)));
    expect(ticks).toEqual([...Array(15).keys()]);

    // A write's answer is a decision, not a feed.
    const write = await open(probe, '', { method: 'POST' });
    await expect.poll(async () => (await stream(probe, write)).refusal).toBe('method');
    expect((await stream(probe, write)).streaming).toBe(false);
  });

  test('a module that stops pulling stops the flow, all the way back to the platform', async ({
    page,
  }) => {
    await openSurface(page, layoutId);
    const probe = await probeIn(page, 'module-probe');
    const name = feedName('held');

    // Heavy ticks, quickly: about 400 KB a second, far more than the module
    // will ask for, and well within the default rate. The backlog that builds
    // while the module holds the stream is read at once when it pulls again,
    // and the shell counts that against the time it was held, not as the
    // platform going over `stream_bytes_per_second` (HLIN-T-0089).
    const credit = 4096;
    const id = await open(probe, `id=${name}&every_ms=10&bytes=4096`, { credit, auto: false });

    // Exactly what was granted arrives, and not a byte more, however long
    // the platform goes on.
    await expect.poll(async () => (await stream(probe, id)).bytes).toBe(credit);
    await page.waitForTimeout(2_000);
    const held = await stream(probe, id);
    expect(held.bytes).toBe(credit);
    expect(held.pulled).toBe(credit);
    expect(held.ended).toBe(false);

    // And the platform is held too: once the connections between have filled,
    // it can write nothing more. Unbounded buffering anywhere would show here
    // as a count that kept climbing.
    let before = await counts(probe, name);
    let still = false;
    for (let look = 0; look < 30 && !still; look += 1) {
      await page.waitForTimeout(1_500);
      const now = await counts(probe, name);
      still = now.bytes === before.bytes;
      before = now;
    }
    expect(still, `the platform went on writing: ${JSON.stringify(before)}`).toBe(true);
    expect(before.open).toBe(true);
    console.log(
      `held: the module took ${held.bytes} bytes; the platform stopped at ${before.bytes} bytes (${before.ticks} ticks)`,
    );
    // Far less than it would have written in the time, had anything read it.
    expect(before.bytes).toBeLessThan(64 * 1024 * 1024);

    // Credit again, as a module consuming it would, and it flows again, all
    // the way back. A little at a time until the platform writes: how much of
    // what the connections hold must drain before the platform's socket can
    // be written again is the operating system's buffering, not the shell's
    // (a single 64 KiB was enough once, and on another machine was not).
    let flowing = false;
    for (let step = 0; step < 80 && !flowing; step += 1) {
      await pull(probe, id, 192 * 1024);
      await page.waitForTimeout(300);
      flowing = (await counts(probe, name)).bytes > before.bytes;
    }
    expect(flowing, 'the platform wrote again once the module read again').toBe(true);
    const resumed = await stream(probe, id);
    expect(resumed.bytes).toBeGreaterThan(credit);
    expect(resumed.bytes).toBeLessThanOrEqual(resumed.pulled);
    expect(resumed.ended).toBe(false);
  });

  test('cancel ends a stream, and the platform sees its connection dropped', async ({ page }) => {
    await openSurface(page, layoutId);
    const probe = await probeIn(page, 'module-probe');
    const name = feedName('cancelled');

    const id = await open(probe, `id=${name}&every_ms=50`, { credit: 1 << 20, auto: true });
    await expect.poll(async () => (await stream(probe, id)).seqs.length).toBeGreaterThan(2);
    expect((await counts(probe, name)).open).toBe(true);

    await cancel(probe, id);
    await expect.poll(async () => (await stream(probe, id)).error).toBe('cancelled');
    await expect.poll(async () => (await counts(probe, name)).open).toBe(false);
  });

  test('a frame may hold only its own share of streams', async ({ page }) => {
    await openSurface(page, layoutId);
    const probe = await probeIn(page, 'module-probe');

    // `streams` is two per frame by default.
    const first = await open(probe, 'every_ms=200', { auto: true });
    const second = await open(probe, 'every_ms=200', { auto: true });
    const third = await open(probe, 'every_ms=200', { auto: true });
    await expect.poll(async () => (await stream(probe, third)).refusal).toBe('too_many');
    for (const id of [first, second]) {
      await expect.poll(async () => (await stream(probe, id)).streaming).toBe(true);
    }

    // One ended, room for one more.
    await cancel(probe, first);
    await expect.poll(async () => (await stream(probe, first)).ended).toBe(true);
    const fourth = await open(probe, 'every_ms=200', { auto: true });
    await expect.poll(async () => (await stream(probe, fourth)).streaming).toBe(true);

    await expect(byKey(page, 'module-probe').frameLocator('iframe').locator('#streams')).toHaveAttribute(
      'data-open',
      '2',
    );
    await shot(page, 70, 'module-streams');
  });

  test('the page holds no more streams than its connections can carry', async ({ page }) => {
    await openSurface(page, layoutId);
    const protocol = await page.evaluate(
      () => performance.getEntriesByType('navigation')[0].nextHopProtocol,
    );
    test.skip(
      protocol !== 'http/1.1',
      `served over ${protocol}, where the page's cap is 32 and three frames cannot reach it`,
    );

    const probe = await probeIn(page, 'module-probe');
    const only = await probeIn(page, 'module-only');
    const context = await probeIn(page, 'module-context');

    // Four is the page's cap over HTTP/1.1, two frames' worth.
    const held = [];
    for (const frame of [probe, only]) {
      for (let i = 0; i < 2; i += 1) {
        const id = await open(frame, 'every_ms=200', { auto: true });
        await expect.poll(async () => (await stream(frame, id)).streaming).toBe(true);
        held.push([frame, id]);
      }
    }

    // The third frame is within its own share, and past the page's.
    const refused = await open(context, 'every_ms=200', { auto: true });
    await expect.poll(async () => (await stream(context, refused)).refusal).toBe('too_many');

    // One let go anywhere on the page makes room for it.
    const [frame, id] = held[0];
    await cancel(frame, id);
    await expect.poll(async () => (await stream(frame, id)).ended).toBe(true);
    const admitted = await open(context, 'every_ms=200', { auto: true });
    await expect.poll(async () => (await stream(context, admitted)).streaming).toBe(true);
  });

  test('unmounting a frame ends its streams, and the platform sees them go', async ({ page }) => {
    const logged = [];
    page.on('console', (message) => logged.push(message.text()));

    await openSurface(page, layoutId);
    const probe = await probeIn(page, 'module-probe');
    const witness = await probeIn(page, 'module-only');
    const name = feedName('unmounted');

    const id = await open(probe, `id=${name}&every_ms=50`, { credit: 1 << 20, auto: true });
    await expect.poll(async () => (await stream(probe, id)).seqs.length).toBeGreaterThan(2);
    expect((await counts(witness, name)).open).toBe(true);

    // The panel taken off the surface: its frame goes, and its stream with it.
    await page.locator('button.mode').click();
    await byKey(page, 'module-probe').locator('button.drop').click();
    await expect(byKey(page, 'module-probe')).toHaveCount(0);
    await settled(page);

    // The frame is gone, so its module's word on it is gone too; the page
    // says on the console what it told the module as it went.
    await expect
      .poll(() => logged.find((line) => line.includes(`stream ${id} ended`)))
      .toMatch(new RegExp(`module ${PLATFORM}/module-probe: stream ${id} ended \\(unmounted\\)`));
    await expect.poll(async () => (await counts(witness, name)).open).toBe(false);
  });
});
