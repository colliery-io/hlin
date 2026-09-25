// Twenty platforms on one surface, measured (HLIN-T-0084).
//
// Run against `angreal demo up --with twenty --release` by
//
//   angreal e2e twenty-measure
//
// and skipped elsewhere. Release, because the numbers are about what a
// deployment would make a browser pay, and a debug module is five times the
// size of an optimised one.
//
// Two kinds of thing here. Numbers, taken three times and reported as the
// median, written to e2e/measurements/ and printed: how long from navigation
// until every widget in view says `ready` (the bridge's handshake) and until
// each has drawn something (its document holds more than "Loading…", which is
// what a person experiences); the bytes that crossed the wire, cold and warm;
// the page's memory; and the frames mounted while scrolling. And claims,
// asserted: the budget of twelve holds while scrolling and nothing in view is
// unmounted to keep it (REQ-4.3); a widget scrolled away gets its state back;
// and a widget whose platform is killed degrades alone and recovers when it
// is started again.
//
// The kill is real: this suite sends SIGKILL to one widget's process, found
// in the demo's process registry, and starts it again with `angreal demo
// restart`. So it is its own command, not part of `angreal e2e twenty`.

const { test, expect } = require('@playwright/test');
const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');
const { shot } = require('./helpers');

/** How many times each number is taken. The median is what is reported. */
const RUNS = Number(process.env.HLIN_RUNS || 3);

const ROOT = path.join(__dirname, '..', '..');
const REGISTRY = path.join(ROOT, 'demo', 'state', 'processes.json');
const OUT = path.join(__dirname, '..', 'measurements');

/** Frames mounted per surface, at most (HLIN-S-0007, *Budget*). */
const BUDGET = 12;

/** How long a widget may take to be ready and draw. */
const READY = 30_000;

/**
 * A network for the cold load to cross, besides loopback: 50 Mbit/s down,
 * 10 up, and 40 ms from each request to its answer. A good office or home
 * connection, not a bad one.
 */
const THROTTLED = {
  name: '50 Mbit/s, 40 ms round trip',
  offline: false,
  latency: 40,
  downloadThroughput: (50_000_000 / 8) | 0,
  uploadThroughput: (10_000_000 / 8) | 0,
};

/** The widget whose platform is killed: in view at the top, and shared. */
const VICTIM = 'dice';

const widget = (page, name) => page.locator(`section.panel[data-panel="${name}/${name}"]`);
const inside = (panel) => panel.frameLocator('iframe');

// -- What every page and frame records about itself --------------------------

/**
 * Installed in every document of the context before its own scripts run: the
 * shell's page and every module's frame.
 *
 * Timestamps are absolute (`timeOrigin + now`), so a frame's and the page's
 * can be compared: they are separate documents, possibly separate processes,
 * on one machine's clock. Recorded as they happen, so how often the test
 * polls for them does not change what they say.
 */
function instrument() {
  const at = () => performance.timeOrigin + performance.now();

  if (window.top !== window) {
    // A module's frame: when its document first holds something other than
    // the loading line. Text inside <style> does not count (every widget
    // draws its stylesheet into the body), and neither does text in <script>.
    const drew = () => {
      const body = document.body;
      if (!body) return false;
      if (body.querySelector('svg, canvas, img, input, textarea, select')) return true;
      const walker = document.createTreeWalker(body, NodeFilter.SHOW_TEXT);
      let text = '';
      for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        const parent = node.parentElement && node.parentElement.tagName;
        if (parent !== 'STYLE' && parent !== 'SCRIPT') text += node.data;
      }
      text = text.trim();
      return text !== '' && text !== 'Loading…';
    };
    const observer = new MutationObserver(() => {
      if (drew()) {
        window.__hlinFirstContent = at();
        observer.disconnect();
      }
    });
    observer.observe(document, { subtree: true, childList: true, characterData: true });
    return;
  }

  // The shell's page: every panel's module state as it changes, the frames
  // mounted at every change, and any frame taken away while its panel was in
  // view (REQ-4.3 says never, for the budget).
  const record = (window.__hlin = {
    origin: performance.timeOrigin,
    ready: {},
    states: [],
    mounted: [],
    removedInView: [],
  });
  const inView = (element) => {
    const box = element.getBoundingClientRect();
    return box.bottom > 0 && box.top < window.innerHeight && box.height > 0;
  };
  const count = () => document.querySelectorAll('section.panel iframe').length;
  new MutationObserver((changes) => {
    let framesMoved = false;
    for (const change of changes) {
      if (change.type === 'attributes') {
        const panel = change.target;
        if (!panel.matches || !panel.matches('section.panel')) continue;
        const name = panel.dataset.panel;
        record.states.push([name, panel.dataset.module || null, panel.dataset.state || null, at()]);
        if (panel.dataset.module === 'ready' && !(name in record.ready)) record.ready[name] = at();
        continue;
      }
      const frames = (nodes) =>
        Array.from(nodes).flatMap((node) =>
          node.tagName === 'IFRAME' ? [node] : node.querySelectorAll ? Array.from(node.querySelectorAll('iframe')) : [],
        );
      if (frames(change.addedNodes).length) framesMoved = true;
      if (frames(change.removedNodes).length) {
        framesMoved = true;
        const panel = change.target.closest && change.target.closest('section.panel');
        // A module given up on is torn down in view, and says so: that is
        // not the budget's doing.
        const givenUp = panel && ['unavailable', 'fallback'].includes(panel.dataset.module);
        if (panel && panel.isConnected && inView(panel) && !givenUp) {
          record.removedInView.push([panel.dataset.panel, window.scrollY, at()]);
        }
      }
    }
    if (framesMoved) record.mounted.push([count(), window.scrollY, at()]);
  }).observe(document, {
    subtree: true,
    childList: true,
    attributes: true,
    attributeFilter: ['data-module', 'data-state'],
  });
}

// -- Arithmetic and plumbing --------------------------------------------------

const median = (values) => {
  const sorted = values.filter((value) => value != null).sort((a, b) => a - b);
  if (!sorted.length) return null;
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const mb = (bytes) => (bytes == null ? '—' : `${(bytes / 1_000_000).toFixed(2)} MB`);
const ms = (value) => (value == null ? '—' : `${Math.round(value)} ms`);

/** Which of the four the bytes of this request count towards. */
function kind(url) {
  const { pathname } = new URL(url);
  if (pathname.startsWith('/m/')) return 'modules';
  if (pathname.startsWith('/p/')) return 'platforms';
  if (pathname.startsWith('/api/stream/')) return 'stream';
  return 'shell';
}

/**
 * Counts what crosses the wire for one context, from now until `total()`.
 *
 * Finished requests are counted by Playwright, which sees every frame's,
 * whatever process it is in: the encoded body plus the headers, and nothing
 * for what came from the cache. A request still open when the count is taken
 * is a stream (the surface's own, or a module's streamed read, which the
 * shell's page holds for it), and those the page's DevTools session counts as
 * they arrive.
 */
async function wire(context, page) {
  const finished = [];
  const onFinished = (request) =>
    finished.push(
      request.sizes().then(async (sizes) => {
        const response = await request.response();
        return {
          url: request.url(),
          status: response ? response.status() : 0,
          encoding: response ? (await response.allHeaders())['content-encoding'] || null : null,
          type: response ? (await response.allHeaders())['content-type'] || null : null,
          body: sizes.responseBodySize,
          headers: sizes.responseHeadersSize,
        };
      }),
    );
  context.on('requestfinished', onFinished);

  const cdp = await context.newCDPSession(page);
  await cdp.send('Network.enable');
  const open = new Map();
  cdp.on('Network.requestWillBeSent', ({ requestId, request }) =>
    open.set(requestId, { url: request.url, bytes: 0 }),
  );
  cdp.on('Network.dataReceived', ({ requestId, encodedDataLength }) => {
    const entry = open.get(requestId);
    if (entry) entry.bytes += encodedDataLength;
  });
  cdp.on('Network.loadingFinished', ({ requestId }) => open.delete(requestId));
  cdp.on('Network.loadingFailed', ({ requestId }) => open.delete(requestId));

  return {
    async total() {
      context.off('requestfinished', onFinished);
      const requests = await Promise.all(finished);
      const bytes = { modules: 0, platforms: 0, stream: 0, shell: 0 };
      for (const request of requests) bytes[kind(request.url)] += request.body + request.headers;
      for (const { url, bytes: streamed } of open.values()) {
        if (url.startsWith('http')) bytes[kind(url)] += streamed;
      }
      await cdp.detach().catch(() => {});
      return {
        bytes: { ...bytes, total: bytes.modules + bytes.platforms + bytes.stream + bytes.shell },
        requests,
      };
    },
  };
}

/**
 * The resident memory of this test's browser, by process type.
 *
 * Every process descended from this worker: Playwright launches the browser
 * from here, so another suite's browser running on the same machine is not
 * counted. Resident set size, which over-counts pages shared between
 * processes, and is what Activity Monitor would show.
 */
function browserMemory() {
  const table = execFileSync('ps', ['-A', '-o', 'pid=,ppid=,rss=,command='], { encoding: 'utf8' })
    .trim()
    .split('\n')
    .map((line) => {
      const [, pid, ppid, rss, command] = line.match(/^\s*(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/) || [];
      return { pid: Number(pid), ppid: Number(ppid), rss: Number(rss) * 1024, command };
    });
  const mine = new Set([process.pid]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const row of table) {
      if (!mine.has(row.pid) && mine.has(row.ppid)) {
        mine.add(row.pid);
        grew = true;
      }
    }
  }
  mine.delete(process.pid);
  const byType = {};
  let processes = 0;
  for (const row of table.filter((row) => mine.has(row.pid))) {
    // Not node: a helper the test runner started, not the browser.
    if (/\bnode\b/.test(row.command.split(' ')[0])) continue;
    const type = (row.command.match(/--type=([a-z-]+)/) || [, 'browser'])[1];
    byType[type] = (byType[type] || 0) + row.rss;
    processes += 1;
  }
  return {
    total: Object.values(byType).reduce((a, b) => a + b, 0),
    byType,
    processes,
    renderers: table.filter((row) => mine.has(row.pid) && /--type=renderer/.test(row.command)).length,
  };
}

/** The JS heaps of the page and of every module frame in its own process. */
async function heaps(context, page) {
  const cdp = await context.newCDPSession(page);
  await cdp.send('Performance.enable');
  const { metrics } = await cdp.send('Performance.getMetrics');
  const metric = (name) => (metrics.find((entry) => entry.name === name) || {}).value;
  await cdp.detach();

  let frames = 0;
  let outOfProcess = 0;
  let theirs = 0;
  for (const frame of page.frames()) {
    if (frame === page.mainFrame()) continue;
    frames += 1;
    // Only a frame in another process has a session of its own; one in the
    // page's process is already in the page's heap.
    const session = await context.newCDPSession(frame).catch(() => null);
    if (!session) continue;
    outOfProcess += 1;
    const { usedSize } = await session.send('Runtime.getHeapUsage').catch(() => ({ usedSize: 0 }));
    theirs += usedSize;
    await session.detach().catch(() => {});
  }
  return {
    page: metric('JSHeapUsedSize'),
    pageTotal: metric('JSHeapTotalSize'),
    frames: theirs,
    used: metric('JSHeapUsedSize') + theirs,
    documents: metric('Documents'),
    domFrames: frames,
    outOfProcess,
  };
}

/** The surface "Twenty", and its panels, from the shell's API. */
async function theSurface(request) {
  const offered = await (await request.get('/api/panels')).json();
  const ids = new Set(offered.map((platform) => platform.id));
  if (!(ids.has('counter') && ids.has('poll'))) return null;
  const layouts = await (await request.get('/api/layouts')).json();
  const twenty = layouts.find((layout) => layout.title === 'Twenty');
  const layout = await (await request.get(`/api/layouts/${twenty.id}`)).json();
  return { url: `/s/${twenty.id}`, panels: layout.panels.map((panel) => panel.platform_id) };
}

/** The panels whose box is on screen right now, by widget name. */
async function panelsInView(page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll('section.panel'))
      .filter((panel) => {
        const box = panel.getBoundingClientRect();
        return box.bottom > 0 && box.top < window.innerHeight;
      })
      .map((panel) => panel.dataset.panel.split('/')[0]),
  );
}

/** When this widget's frame first drew, if it has, as page milliseconds. */
async function firstContent(page, name) {
  const handle = await widget(page, name).locator('iframe').elementHandle({ timeout: 100 }).catch(() => null);
  const frame = handle && (await handle.contentFrame());
  if (!frame) return null;
  return frame.evaluate(() => window.__hlinFirstContent || null).catch(() => null);
}

/**
 * Open the surface and wait for every widget in view to be ready and to have
 * drawn. Returns how long each took from navigation, in milliseconds.
 */
async function openAndTime(page, surface, count) {
  await page.goto(surface.url);
  await expect(page.locator('section.panel')).toHaveCount(count, { timeout: READY });
  const names = await panelsInView(page);
  const origin = await page.evaluate(() => window.__hlin.origin);

  const ready = {};
  const drawn = {};
  const deadline = Date.now() + READY;
  while (Object.keys(drawn).length < names.length || Object.keys(ready).length < names.length) {
    if (Date.now() > deadline) {
      const missing = names.filter((name) => !(name in drawn) || !(name in ready));
      throw new Error(`not ready and drawn within ${READY} ms: ${missing.join(', ')}`);
    }
    const seen = await page.evaluate(() => window.__hlin.ready);
    for (const name of names) {
      if (!(name in ready) && seen[`${name}/${name}`]) ready[name] = seen[`${name}/${name}`] - origin;
      if (!(name in drawn)) {
        const at = await firstContent(page, name);
        if (at) drawn[name] = at - origin;
      }
    }
    await page.waitForTimeout(100);
  }
  return {
    inView: names,
    ready,
    drawn,
    allReady: Math.max(...Object.values(ready)),
    allDrawn: Math.max(...Object.values(drawn)),
  };
}

/**
 * Scroll top to bottom and back, 150 px every 150 ms, as a person reading
 * down a page might; and at every screenful stop for a second and count the
 * frames again, once any suspension has run its course.
 *
 * Returns those settled counts. The count at every change on the way is the
 * page's own record (`__hlin.mounted`).
 */
async function scrollThrough(page) {
  const height = await page.evaluate(() => document.documentElement.scrollHeight - window.innerHeight);
  const step = 150;
  const settled = [];
  const stop = async () => {
    await page.waitForTimeout(1000);
    settled.push(await page.evaluate(() => document.querySelectorAll('section.panel iframe').length));
  };
  const positions = [];
  for (let y = 0; y < height; y += step) positions.push(y);
  positions.push(height);
  for (const route of [positions, [...positions].reverse()]) {
    for (const [index, y] of route.entries()) {
      await page.evaluate((to) => window.scrollTo(0, to), y);
      await page.waitForTimeout(150);
      if (index % 6 === 5) await stop();
    }
    await stop();
  }
  return settled;
}

/**
 * What these assets weighed as sent (their encoded bodies, each once), and
 * what they would weigh uncompressed and gzipped, fetched once more to find
 * out (Playwright's request decodes what it is sent).
 */
async function gzipped(request, answered) {
  const sentBy = new Map(answered.map((r) => [r.url, r.body]));
  let raw = 0;
  let zipped = 0;
  let sent = 0;
  const each = [];
  for (const [url, onTheWire] of sentBy) {
    const body = await (await request.get(url)).body();
    const packed = zlib.gzipSync(body, { level: 6 }).length;
    raw += body.length;
    zipped += packed;
    sent += onTheWire;
    each.push({ url: new URL(url).pathname, raw: body.length, gzip: packed, sent: onTheWire });
  }
  return { raw, gzip: zipped, sent, each };
}

// -- The suite ----------------------------------------------------------------

test.describe('twenty widgets, measured', () => {
  test.describe.configure({ mode: 'serial' });

  let surface;
  const results = { runs: [], claims: {} };

  test.beforeAll(async ({ request }) => {
    surface = await theSurface(request);
    test.skip(!surface, 'this is not the twenty-widget demo');
  });

  test.afterAll(async () => {
    if (!results.runs.length) return;
    fs.mkdirSync(OUT, { recursive: true });
    const file = path.join(OUT, `twenty-${new Date().toISOString().replace(/[:.]/g, '-')}.json`);
    fs.writeFileSync(file, JSON.stringify(results, null, 2));
    console.log(`\n  written to ${path.relative(ROOT, file)}`);
  });

  test(`cold and warm, ${RUNS} times`, async ({ browser, request }) => {
    test.setTimeout(RUNS * 180_000);

    // What the browser holds with nothing open but an empty page, to set
    // the surface's memory against.
    {
      const context = await browser.newContext();
      const page = await context.newPage();
      await page.goto('about:blank');
      await page.waitForTimeout(1000);
      results.baseline = browserMemory();
      await context.close();
    }

    for (let run = 1; run <= RUNS; run += 1) {
      // Cold: a context of its own has an empty cache.
      const context = await browser.newContext();
      await context.addInitScript(instrument);
      const page = await context.newPage();
      const coldWire = await wire(context, page);
      const cold = await openAndTime(page, surface, surface.panels.length);
      // Settled: whatever the widgets fetch after drawing has arrived.
      await page.waitForTimeout(3000);
      const coldBytes = await coldWire.total();
      const coldHeap = await heaps(context, page);
      const coldMemory = browserMemory();

      // The budget, over a scroll down and back; and what the rest of the
      // surface costs to fetch, the widgets below the first screen.
      await page.evaluate(() => {
        window.__hlin.mounted = [];
        window.__hlin.removedInView = [];
      });
      const scrollWire = await wire(context, page);
      const settled = await scrollThrough(page);
      const scrollBytes = await scrollWire.total();
      const scrolled = await page.evaluate(() => ({
        mounted: window.__hlin.mounted.map(([count]) => count),
        removedInView: window.__hlin.removedInView,
      }));
      const afterScroll = { heap: await heaps(context, page), memory: browserMemory() };

      // Warm: the same context, navigating to the surface again.
      const warmWire = await wire(context, page);
      const warm = await openAndTime(page, surface, surface.panels.length);
      await page.waitForTimeout(3000);
      const warmBytes = await warmWire.total();

      if (run === 1) {
        // The screens a person sees, taken after the widgets drew rather
        // than at `ready`, and a screenful at a time: a whole-page shot does
        // not composite sandboxed frames reliably.
        for (const [order, where] of [
          [220, 'top'],
          [221, 'middle'],
          [222, 'bottom'],
        ]) {
          await page.evaluate((to) => {
            const most = document.documentElement.scrollHeight - window.innerHeight;
            window.scrollTo(0, { top: 0, middle: most / 2, bottom: most }[to]);
          }, where);
          for (const name of await panelsInView(page)) {
            await expect(widget(page, name)).toHaveAttribute('data-module', 'ready', { timeout: READY });
            await expect.poll(() => firstContent(page, name), { timeout: READY }).not.toBeNull();
          }
          await page.waitForTimeout(300);
          await shot(page, order, `twenty-drawn-${where}`);
        }
      }

      const compression =
        run === 1
          ? await (async () => {
              const everything = [...coldBytes.requests, ...scrollBytes.requests];
              const assets = everything.filter((r) => kind(r.url) === 'modules' && r.status === 200);
              const shellAssets = coldBytes.requests.filter(
                (r) => kind(r.url) === 'shell' && /\.(wasm|js|css)$/.test(new URL(r.url).pathname),
              );
              return {
                moduleEncodings: [...new Set(assets.map((r) => r.encoding || 'identity'))],
                shellEncodings: [...new Set(shellAssets.map((r) => r.encoding || 'identity'))],
                modules: await gzipped(request, assets),
                shell: await gzipped(request, shellAssets),
              };
            })()
          : undefined;

      await context.close();

      const one = {
        run,
        cold: { ...cold, bytes: coldBytes.bytes, heap: coldHeap, memory: coldMemory },
        warm: {
          ...warm,
          bytes: warmBytes.bytes,
          // What the second visit fetched again from `/m/`, and how.
          modules: warmBytes.requests
            .filter((r) => kind(r.url) === 'modules')
            .map((r) => [new URL(r.url).pathname, r.status, r.body]),
        },
        scroll: {
          bytes: scrollBytes.bytes,
          peakMounted: Math.max(0, ...scrolled.mounted),
          settledMounted: Math.max(0, ...settled),
          settled,
          ...scrolled,
        },
        afterScroll,
        compression,
      };
      results.runs.push(one);
      console.log(
        `  run ${run}: cold ${cold.inView.length} in view ready ${ms(cold.allReady)}, drawn ${ms(cold.allDrawn)}, ` +
          `${mb(coldBytes.bytes.total)}; warm ready ${ms(warm.allReady)}, drawn ${ms(warm.allDrawn)}, ` +
          `${mb(warmBytes.bytes.total)}; heap ${mb(coldHeap.used)}; browser ${mb(coldMemory.total)}; ` +
          `mounted at most ${one.scroll.settledMounted} settled, ${one.scroll.peakMounted} at the peak`,
      );

      // The budget is kept at every moment, not only once suspensions have
      // run their course: a frame counts until it has left the document, and
      // room is made before another is mounted (HLIN-T-0085). The peak is the
      // page's own count at every change in the document on the way.
      expect(one.scroll.peakMounted, 'frames in the document at the peak while scrolling').toBeLessThanOrEqual(BUDGET);
      expect(one.scroll.settledMounted, 'frames mounted, settled, while scrolling').toBeLessThanOrEqual(BUDGET);
      expect(scrolled.removedInView, 'frames unmounted while their panel was in view').toEqual([]);
      // A second visit fetches no module's wasm again: it has not changed
      // (HLIN-T-0088, where kanban's was fetched whole on every visit).
      expect(
        one.warm.modules.filter(([pathname, status, body]) => pathname.endsWith('.wasm') && status === 200 && body > 0),
        'module wasm fetched again on a warm visit',
      ).toEqual([]);
    }

    // The same cold load over a network rather than loopback: everything
    // above crossed no wire at all, and 7 MB is not free on a real one. The
    // frames share the page's renderer (checked above: one renderer
    // process), so the page's conditions are theirs too.
    for (const one of results.runs) {
      const context = await browser.newContext();
      await context.addInitScript(instrument);
      const page = await context.newPage();
      const cdp = await context.newCDPSession(page);
      await cdp.send('Network.enable');
      await cdp.send('Network.emulateNetworkConditions', THROTTLED);
      one.throttled = await openAndTime(page, surface, surface.panels.length);
      console.log(
        `  over ${THROTTLED.name}, run ${one.run}: ready ${ms(one.throttled.allReady)}, drawn ${ms(one.throttled.allDrawn)}`,
      );
      await context.close();
    }

    const pick = (get) => median(results.runs.map(get));
    results.medians = {
      inView: results.runs[0].cold.inView.length,
      coldReady: pick((r) => r.cold.allReady),
      coldDrawn: pick((r) => r.cold.allDrawn),
      warmReady: pick((r) => r.warm.allReady),
      warmDrawn: pick((r) => r.warm.allDrawn),
      coldBytes: Object.fromEntries(
        ['modules', 'platforms', 'stream', 'shell', 'total'].map((k) => [k, pick((r) => r.cold.bytes[k])]),
      ),
      warmBytes: Object.fromEntries(
        ['modules', 'platforms', 'stream', 'shell', 'total'].map((k) => [k, pick((r) => r.warm.bytes[k])]),
      ),
      heapUsed: pick((r) => r.cold.heap.used),
      browserMemory: pick((r) => r.cold.memory.total),
      rendererMemory: pick((r) => r.cold.memory.byType.renderer),
      renderers: pick((r) => r.cold.memory.renderers),
      heapAfterScroll: pick((r) => r.afterScroll.heap.used),
      browserMemoryAfterScroll: pick((r) => r.afterScroll.memory.total),
      scrollBytes: pick((r) => r.scroll.bytes.total),
      scrollModuleBytes: pick((r) => r.scroll.bytes.modules),
      settledMounted: Math.max(...results.runs.map((r) => r.scroll.settledMounted)),
      peakMounted: Math.max(...results.runs.map((r) => r.scroll.peakMounted)),
      baseline: results.baseline.total,
      throttledReady: pick((r) => r.throttled.allReady),
      throttledDrawn: pick((r) => r.throttled.allDrawn),
    };
    const m = results.medians;
    console.log(`\n  medians of ${RUNS}, ${m.inView} widgets in view at the top:`);
    console.log(`    cold: every one ready ${ms(m.coldReady)}, drawn ${ms(m.coldDrawn)}, ${mb(m.coldBytes.total)}`);
    console.log(`          modules ${mb(m.coldBytes.modules)}, platforms ${mb(m.coldBytes.platforms)}, stream ${mb(m.coldBytes.stream)}, shell ${mb(m.coldBytes.shell)}`);
    console.log(`    warm: every one ready ${ms(m.warmReady)}, drawn ${ms(m.warmDrawn)}, ${mb(m.warmBytes.total)}`);
    console.log(`    cold over ${THROTTLED.name}: every one ready ${ms(m.throttledReady)}, drawn ${ms(m.throttledDrawn)}`);
    console.log(`    JS heap ${mb(m.heapUsed)} (${mb(m.heapAfterScroll)} after a scroll)`);
    console.log(`    browser ${mb(m.browserMemory)} resident, renderers ${mb(m.rendererMemory)} in ${m.renderers} processes (${mb(m.browserMemoryAfterScroll)} after a scroll)`);
    console.log(`    (an empty page in the same browser: ${mb(m.baseline)} resident)`);
    console.log(`    the rest of the surface, scrolling down and back: ${mb(m.scrollBytes)}, modules ${mb(m.scrollModuleBytes)}`);
    console.log(`    most frames mounted while scrolling: ${m.settledMounted} settled, ${m.peakMounted} at the peak`);
    const c = results.runs[0].compression;
    console.log(`    module assets sent as ${c.moduleEncodings.join(', ')}: ${mb(c.modules.sent)} (${mb(c.modules.raw)} uncompressed, ${mb(c.modules.gzip)} gzipped)`);
    console.log(`    shell assets sent as ${c.shellEncodings.join(', ')}: ${mb(c.shell.sent)} (${mb(c.shell.raw)} uncompressed, ${mb(c.shell.gzip)} gzipped)`);
  });

  test('a widget scrolled away gets its state back when it returns', async ({ browser }) => {
    const context = await browser.newContext();
    await context.addInitScript(instrument);
    const page = await context.newPage();
    await page.goto(surface.url);
    await expect(page.locator('section.panel')).toHaveCount(surface.panels.length, { timeout: READY });

    const bring = async (name) => {
      await widget(page, name).scrollIntoViewIfNeeded();
      await expect(widget(page, name)).toHaveAttribute('data-module', 'ready', { timeout: READY });
      return inside(widget(page, name));
    };
    const gone = (name) => widget(page, name).locator('iframe');
    /** Scroll a screenful at a time to one end, so the budget works as it would for a person. */
    const scrollTo = async (end) => {
      const most = await page.evaluate(() => document.documentElement.scrollHeight - window.innerHeight);
      const from = await page.evaluate(() => window.scrollY);
      const to = end === 'bottom' ? most : 0;
      const steps = Math.ceil(Math.abs(to - from) / 300);
      for (let step = 1; step <= steps; step += 1) {
        await page.evaluate((y) => window.scrollTo(0, y), from + ((to - from) * step) / steps);
        await page.waitForTimeout(300);
      }
      await page.waitForTimeout(1500);
    };

    // At the top, two with something held: the stopwatch running (kept by
    // its platform) and the note half rewritten (a draft kept only in the
    // browser, handed to the shell at `suspend`).
    const stopwatch = await bring('stopwatch');
    if ((await stopwatch.locator('.stopwatch').getAttribute('data-running')) !== 'true') {
      await stopwatch.getByRole('button', { name: 'Start' }).click();
    }
    await expect(stopwatch.locator('.stopwatch')).toHaveAttribute('data-running', 'true');
    const notes = await bring('notes');
    const draft = `Half a thought ${Date.now()}`;
    await notes.getByRole('button', { name: 'Edit' }).click();
    await notes.getByRole('textbox', { name: 'Note' }).fill(draft);

    // Down to the bottom, where the budget takes both; and there, the
    // converter given a value (kept only in the browser, and handed to the
    // shell at `suspend`).
    await scrollTo('bottom');
    await expect(gone('notes'), 'the budget unmounted the note').toHaveCount(0, { timeout: 10_000 });
    await expect(gone('stopwatch'), 'the budget unmounted the stopwatch').toHaveCount(0, { timeout: 10_000 });
    const converter = await bring('converter');
    await converter.getByRole('textbox', { name: 'Value' }).fill('42');
    const answer = await converter.locator('.converter__answer').getAttribute('data-value');

    // Back to the top, which takes the converter, and brings the other two
    // back.
    await scrollTo('top');
    await expect(gone('converter'), 'the budget unmounted the converter').toHaveCount(0, { timeout: 10_000 });

    const watch = await bring('stopwatch');
    await expect(watch.locator('.stopwatch'), 'the stopwatch is still running').toHaveAttribute('data-running', 'true');
    const note = await bring('notes');
    await expect(note.locator('.note')).toBeVisible({ timeout: READY });
    await expect(note.getByRole('textbox', { name: 'Note' }), 'the note kept its draft').toHaveValue(draft);

    await scrollTo('bottom');
    const back = await bring('converter');
    await expect(back.getByRole('textbox', { name: 'Value' }), 'the converter kept its value').toHaveValue('42');
    await expect(back.locator('.converter__answer')).toHaveAttribute('data-value', answer);

    results.claims.suspend = {
      converter: 'value kept (suspend hook)',
      stopwatch: 'still running (its platform keeps it)',
      notes: 'draft kept (suspend hook)',
    };
    console.log(`  after scrolling away and back: ${JSON.stringify(results.claims.suspend)}`);
    test.info().annotations.push({ type: 'suspend', description: JSON.stringify(results.claims.suspend) });

    // Stopped again, so the demo is left as it was found.
    await scrollTo('top');
    await (await bring('stopwatch')).getByRole('button', { name: 'Stop' }).click();
    await context.close();
  });

  test('a shared widget changed in one browser reaches another', async ({ browser }) => {
    const times = [];
    const pages = [];
    for (const _ of [1, 2]) {
      const context = await browser.newContext();
      await context.addInitScript(instrument);
      const page = await context.newPage();
      await openAndTime(page, surface, surface.panels.length);
      pages.push(page);
    }
    const [first, second] = pages;
    const there = await widget(second, 'counter').locator('iframe').elementHandle();
    const thereFrame = await there.contentFrame();

    for (let run = 1; run <= RUNS; run += 1) {
      const before = Number(await inside(widget(second, 'counter')).locator('.w-big').getAttribute('data-value'));
      // The second browser notes the moment its counter changes, from inside
      // its own frame, so polling from here adds nothing to the time.
      const arrived = thereFrame.evaluate(
        (from) =>
          new Promise((resolve) => {
            const big = () => document.querySelector('.w-big');
            const check = () => {
              if (big() && Number(big().dataset.value) > from) {
                resolve(performance.timeOrigin + performance.now());
                return true;
              }
              return false;
            };
            if (!check()) {
              const observer = new MutationObserver(() => check() && observer.disconnect());
              observer.observe(document, { subtree: true, childList: true, attributes: true, characterData: true });
            }
          }),
        before,
      );
      const clicked = Date.now();
      await inside(widget(first, 'counter')).getByRole('button', { name: 'Bump up' }).click();
      const took = (await arrived) - clicked;
      times.push(took);
      console.log(`  a bump reached the second browser in ${Math.round(took)} ms`);
      await first.waitForTimeout(500);
    }
    results.claims.propagation = { times, median: median(times) };
    console.log(`  median ${ms(median(times))}`);
    for (const page of pages) await page.context().close();
  });

  test(`killing ${VICTIM}'s platform degrades its panel alone, and it recovers by itself`, async ({ browser }) => {
    test.setTimeout(240_000);
    const context = await browser.newContext();
    await context.addInitScript(instrument);
    const page = await context.newPage();
    const opened = await openAndTime(page, surface, surface.panels.length);
    expect(opened.inView).toContain(VICTIM);
    expect(opened.inView).toContain('counter');

    const { pid } = JSON.parse(fs.readFileSync(REGISTRY, 'utf8'))[VICTIM];
    const killedAt = await page.evaluate(() => performance.timeOrigin + performance.now());
    process.kill(pid, 'SIGKILL');
    console.log(`  killed ${VICTIM} (pid ${pid})`);

    // The open page. Its module is already running in the browser and
    // answers the shell's heartbeat whatever its platform is doing; what the
    // page sees is the shell refusing its requests as `unreachable`. A roll is
    // two (the write, and the read after it), and the module says so in its
    // own words and offers to try again; trying is the third in a row, and
    // the panel goes `stale` (HLIN-S-0007, *Panel states*).
    const victim = widget(page, VICTIM);
    const roll = inside(victim).getByRole('button', { name: 'Roll' });
    await roll.click();
    await expect(inside(victim).locator('.w-problem').first(), 'the roll is refused in words').toBeVisible({
      timeout: 15_000,
    });
    const again = inside(victim).locator('.w-failed').getByRole('button', { name: 'Try again' });
    await expect(again, 'a read that failed offers to try again').toBeVisible({ timeout: 15_000 });
    await again.click();
    await expect(victim, 'three refusals running: stale').toHaveAttribute('data-module', 'stale', { timeout: 15_000 });
    const staleAt = await page.evaluate(() => performance.timeOrigin + performance.now());

    // Watched past the registry's two missed polls, ten seconds apart, so the
    // shell has noticed too; what the panel went through is recorded, not
    // assumed.
    await page.waitForTimeout(25_000);
    await shot(page, 230, 'twenty-one-platform-killed');
    const counter = inside(widget(page, 'counter'));
    const count = Number(await counter.locator('.w-big').getAttribute('data-value'));
    await counter.getByRole('button', { name: 'Bump up' }).click();
    await expect(counter.locator('.w-big'), 'the counter still works').toHaveAttribute('data-value', String(count + 1));
    const states = await page.evaluate(() => window.__hlin.states);
    const afterKill = states.filter(([, , , at]) => at >= killedAt);
    const others = afterKill.filter(([name, module]) => !name.startsWith(`${VICTIM}/`) && module !== 'ready');
    expect(others, 'no other panel left `ready`').toEqual([]);
    await expect(victim, 'still stale while its platform is away').toHaveAttribute('data-module', 'stale');
    const openPage = {
      victimStates: afterKill
        .filter(([name]) => name.startsWith(`${VICTIM}/`))
        .map(([, module, state, at]) => [module, state, Math.round(at - killedAt)]),
      staleAfterKillMs: Math.round(staleAt - killedAt),
      victimSays: (await inside(victim).locator('main').innerText().catch(() => '')).slice(0, 200),
    };

    // A page opened while it is down: the module's entry cannot be fetched,
    // so it is `unavailable` at once, and the other nineteen are as ever.
    const fresh = await context.newPage();
    await fresh.goto(surface.url);
    await expect(widget(fresh, VICTIM)).toHaveAttribute('data-module', /unavailable|fallback/, { timeout: READY });
    for (const name of opened.inView.filter((name) => name !== VICTIM)) {
      await expect(widget(fresh, name), `${name} is unaffected`).toHaveAttribute('data-module', 'ready', { timeout: READY });
    }
    await shot(fresh, 231, 'twenty-one-platform-down-on-open');
    const freshDown = {
      module: await widget(fresh, VICTIM).getAttribute('data-module'),
      cause: await widget(fresh, VICTIM).getAttribute('data-module-cause'),
      data: await widget(fresh, VICTIM).getAttribute('data-state'),
    };

    // Started again, and nobody touches anything. The shell's subscription
    // to the platform's event stream comes back, which it takes as a change
    // to every one of the platform's panels: the open page's module fetches
    // again, and its first answer clears `stale`; the page opened while it
    // was down mounts its module again. Neither needs a click or a reload.
    const restartedAt = Date.now();
    execFileSync('angreal', ['demo', 'restart', VICTIM], { cwd: ROOT, stdio: 'inherit' });
    const up = Date.now();
    const BY_ITSELF = 60_000;
    await expect(victim, 'the open page recovers by itself').toHaveAttribute('data-module', 'ready', {
      timeout: BY_ITSELF,
    });
    await expect(roll, 'and draws its table again').toBeVisible({ timeout: 15_000 });
    const openPageBack = Date.now() - up;
    await expect(widget(fresh, VICTIM), 'the page opened while it was down recovers by itself').toHaveAttribute(
      'data-module',
      'ready',
      { timeout: BY_ITSELF },
    );
    await expect.poll(() => firstContent(fresh, VICTIM), { timeout: READY }).not.toBeNull();
    const freshPageBack = Date.now() - up;
    await shot(fresh, 232, 'twenty-one-platform-recovered');

    // And it works: a roll on the open page happens.
    await roll.click();
    await page.waitForTimeout(1000);
    await expect(inside(victim).locator('.w-problem'), 'the open page rolls again').toHaveCount(0);

    results.claims.kill = {
      openPage,
      freshPageWhileDown: freshDown,
      afterRestart: {
        restartTookMs: up - restartedAt,
        openPageBackMs: openPageBack,
        freshPageBackMs: freshPageBack,
      },
    };
    console.log(`  ${JSON.stringify(results.claims.kill, null, 2)}`);
    await context.close();
  });
});
