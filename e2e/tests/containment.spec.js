// The containment matrix (HLIN-S-0007, *Containment*; NFR-1.3).
//
// Every isolation property of a module frame fails silently if it fails, so
// each is proven here in a real browser rather than argued. The subject is the
// sample platform's hostile module (crates/hlin-sample-platform/ui/hostile):
// plain JavaScript, judged by exactly the frame and policy every module gets,
// which tries each way out when asked and says what stopped it. What a frame
// cannot report about itself — a navigation that ended its document, a
// window that did or did not open, a request that did or did not leave the
// browser — is judged from outside, by the page and by Playwright.
//
// The rows, in the order the specification lists them:
//
// | Escape | Stopped by |
// |---|---|
// | Read the parent's document or anything on it | The sandbox's opaque origin |
// | Read cookies, storage, IndexedDB, caches | The sandbox's opaque origin |
// | Reach the network: fetch, XHR, WebSocket, EventSource, image, beacon | The module CSP (`connect-src`, `img-src`) |
// | Run script from elsewhere or from a string; a worker; a frame of its own | The module CSP (`script-src`, `worker-src`, `frame-src` by `default-src`) |
// | Navigate its frame off `/m/` | The shell page's CSP (`frame-src`) |
// | Open a window, navigate the top or a sibling, submit a form, open a dialog | The sandbox's flags, and `form-action 'none'` |
// | Use a powerful feature | The frame's empty `allow` |
// | Pose as the shell to another module | The SDK and the probe hearing only `window.parent` |
// | Call `/p/` directly | The CSP first; without it, `not_from_shell`, and no session |
// | Reach another platform through the bridge | The page routing by frame, never by message |
// | Flood messages or fetches | `messages_per_second`, `fetches_in_flight` (`too_many`) |
// | Starve the page with streams | `streams` per frame and the page-wide cap |
// | Throw | The frame's own failure |
// | Spin | The heartbeat, where the browser runs the frame in its own process |
//
// Chromium always. `HLIN_BROWSERS=chromium,chromium-full,firefox,webkit` runs
// the same matrix in the full Chromium browser and the others, where
// Playwright has them installed. Where they differ, the specification says
// how, and so do the tests.

const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, openSurface, shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

/** Where the sample platforms answer directly: what a module must never reach. */
const OREBANK = 'http://127.0.0.1:8081';

const byKey = (page, platform, key) =>
  page.locator(`section.panel[data-panel="${platform}/${key}"]`);

/** A panel's frame, once its module has said `ready`. */
async function frameOf(page, platform, key) {
  const panel = byKey(page, platform, key);
  await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 30_000 });
  const handle = await panel.locator('iframe').elementHandle();
  return handle.contentFrame();
}

/** Call one of the hostile module's attempts, in its own frame. */
const attempt = (frame, name, ...args) =>
  frame.evaluate(([n, a]) => window.hostile[n](...a), [name, args]);

/** Every address the attempts aim at. */
function targets(baseURL) {
  return {
    platform: `${OREBANK}/api/module/whoami`,
    shellApi: `${baseURL}/api/panels`,
    otherAssets: `${baseURL}/m/stampmill/ui/lure/lure.js`,
    elsewhere: 'https://example.com/',
    socket: baseURL.replace(/^http/, 'ws') + '/api/stream',
    pixel: `${OREBANK}/ui/lure/pixel.png`,
    beacon: `${OREBANK}/api/module/whoami`,
    lureScript: `${OREBANK}/ui/lure/lure.js`,
    otherScript: `${baseURL}/m/stampmill/ui/lure/lure.js`,
    lurePage: `${OREBANK}/ui/lure/index.html`,
  };
}

/**
 * Every request a module frame got an answer to from anywhere but the shell's
 * own origin. An answer, because a request the CSP stopped is still reported
 * to Playwright, as one that failed before it was sent; and a module frame's,
 * because the shell page itself may load what it likes (a pack's fonts).
 */
function watchLeaks(page, baseURL) {
  const leaks = [];
  const shell = new URL(baseURL).origin;
  page.context().on('response', (response) => {
    const url = response.url();
    let frame = null;
    try {
      frame = response.frame();
    } catch {
      // A service worker's request has no frame; the shell has none.
    }
    if (frame && frame === frame.page().mainFrame()) {
      return;
    }
    if (/^(https?|wss?):/.test(url) && new URL(url).origin !== shell) {
      leaks.push(url);
    }
  });
  return leaks;
}

/** What the shell page's own CSP reported blocking, from now on. */
async function watchPagePolicy(page) {
  await page.evaluate(() => {
    window.__blocked = [];
    document.addEventListener('securitypolicyviolation', (event) => {
      window.__blocked.push({ uri: event.blockedURI, directive: event.effectiveDirective });
    });
  });
  return () => page.evaluate(() => window.__blocked);
}

/** Every attempt that got through, for a readable failure. */
const escapes = (result) => result.tried.filter((one) => one.escaped);

test.describe('a hostile module cannot get out of its frame', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    layoutId = await freshLayout(request, 'Containment');
    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    current.panels = [
      ['orebank', 'module-hostile', { x: 0, y: 0, w: 4, h: 8 }],
      ['orebank', 'module-probe', { x: 4, y: 0, w: 4, h: 8 }],
      ['stampmill', 'module-hostile', { x: 8, y: 0, w: 4, h: 8 }],
      ['orebank', 'records-per-second', { x: 0, y: 8, w: 4, h: 3 }],
    ].map(([platform_id, panel_key, position]) => ({ platform_id, panel_key, position }));
    const written = await request.put(`/api/layouts/${layoutId}`, { data: current });
    expect(written.ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    await discardLayout(request, layoutId);
  });

  test('cannot read the parent document, or anything on the parent', async ({ page }) => {
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const result = await attempt(hostile, 'parentDocument');
    expect(escapes(result)).toEqual([]);
    // Refused, not merely empty: each read of the parent threw.
    for (const one of result.tried.filter((t) => t.what.startsWith('window.parent') || t.what.startsWith('window.top'))) {
      expect(one.how, one.what).toBe('SecurityError');
    }
  });

  test('cannot read the shell’s cookies or storage', async ({ page, context }) => {
    await openSurface(page, layoutId);
    // Something worth stealing, on the shell's origin.
    await context.addCookies([{ name: 'hlin_contained', value: 'secret', url: page.url() }]);
    await page.evaluate(() => {
      localStorage.setItem('hlin-contained', 'secret');
      sessionStorage.setItem('hlin-contained', 'secret');
    });
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const result = await attempt(hostile, 'cookiesAndStorage');
    expect(escapes(result)).toEqual([]);
    const how = Object.fromEntries(result.tried.map((one) => [one.what, one.how]));
    expect(how['document.cookie']).toBe('SecurityError');
    expect(how.localStorage).toBe('SecurityError');
    expect(how.sessionStorage).toBe('SecurityError');
  });

  test('cannot reach the network, by any route a document has', async ({ page }) => {
    const baseURL = test.info().project.use.baseURL;
    const leaks = watchLeaks(page, baseURL);
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const result = await attempt(hostile, 'network', targets(baseURL));
    expect(escapes(result)).toEqual([]);
    // Stopped by the module CSP, before any request was made.
    for (const one of result.tried.filter((t) => t.what.startsWith('fetch'))) {
      expect(one.how, one.what).toBe('CSP connect-src');
    }
    expect(leaks, 'nothing left for anywhere but the shell').toEqual([]);
  });

  test('cannot run script from anywhere but its own assets, or from a string', async ({ page }) => {
    const baseURL = test.info().project.use.baseURL;
    const leaks = watchLeaks(page, baseURL);
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const result = await attempt(hostile, 'scripts', targets(baseURL));
    expect(escapes(result)).toEqual([]);
    expect(leaks).toEqual([]);
  });

  test('cannot open a window, move the page, touch another frame or open a dialog', async ({
    page,
    context,
  }) => {
    const baseURL = test.info().project.use.baseURL;
    const leaks = watchLeaks(page, baseURL);
    const dialogs = [];
    page.on('dialog', (dialog) => {
      dialogs.push(dialog.message());
      dialog.dismiss();
    });
    await openSurface(page, layoutId);
    const address = page.url();
    const probe = await frameOf(page, 'orebank', 'module-probe');
    const generation = await probe.locator('#generation').textContent();
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const result = await attempt(hostile, 'windows', targets(baseURL));
    expect(escapes(result)).toEqual([]);

    // Judged from outside: still one page, still the surface, no dialog, and
    // the frame beside it neither moved nor fooled.
    await page.waitForTimeout(1_000);
    expect(context.pages()).toHaveLength(1);
    expect(page.url()).toBe(address);
    expect(dialogs).toEqual([]);
    expect(probe.url()).toContain('/m/orebank/ui/probe/index.html');
    await expect(byKey(page, 'orebank', 'module-probe')).toHaveAttribute('data-module', 'ready');
    await expect(probe.locator('#generation')).toHaveText(generation);
    expect(leaks).toEqual([]);
  });

  test('cannot use a powerful feature', async ({ page, browserName }) => {
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');
    const result = await attempt(hostile, 'features');
    // The frame's empty `allow` withholds every feature where a browser
    // governs it by permissions policy. Chromium governs writing the
    // clipboard that way; Firefox and WebKit do not, and let a frame write
    // (not read) the clipboard on a person's click, which Playwright's builds
    // of them allow without one. Recorded in the specification; nothing the
    // shell sends can withhold it there.
    const allowed = browserName === 'chromium' ? [] : ['clipboard write'];
    expect(escapes(result).map((one) => one.what)).toEqual(allowed);
  });

  for (const [name, where] of [
    ['its platform directly', () => `${OREBANK}/ui/lure/index.html`],
    ['the shell’s own pages', (base) => `${base}/`],
    ['the request proxy', (base) => `${base}/p/orebank/api/module/whoami`],
    ['a data: document', () => 'data:text/html,<p id=lured>lured</p>'],
  ]) {
    test(`cannot navigate its frame off /m/: to ${name}`, async ({ page }) => {
      const baseURL = test.info().project.use.baseURL;
      const target = where(baseURL);
      const answered = [];
      // Only what was fetched for a frame: the shell page asks the proxy on
      // the probe's behalf, which is what the proxy is for.
      page.on('response', (response) => {
        if (response.url() === target && response.frame() !== page.mainFrame()) {
          answered.push(response.status());
        }
      });
      await openSurface(page, layoutId);
      const blocked = await watchPagePolicy(page);
      const hostile = await frameOf(page, 'orebank', 'module-hostile');

      await hostile.evaluate((url) => window.hostile.navigate(url), target).catch(() => {});

      // The shell page's `frame-src` refused it: reported on the page, and
      // nothing was fetched for the frame.
      await expect
        .poll(async () => (await blocked()).map((one) => one.directive))
        .toContain('frame-src');
      expect(answered).toEqual([]);
      const handle = await byKey(page, 'orebank', 'module-hostile').locator('iframe').elementHandle();
      const now = await handle.contentFrame();
      // What is left in the frame is the browser's: Chromium and Firefox
      // show an error page, which answers nothing, so the module is given up
      // on as any that stops answering is; WebKit cancels the navigation and
      // leaves the module where it was. Neither is the target.
      expect(now.url()).not.toBe(target);
      if (!now.url().includes('/m/orebank/')) {
        await expect(byKey(page, 'orebank', 'module-hostile')).not.toHaveAttribute(
          'data-module',
          'ready',
          { timeout: 15_000 },
        );
      }
    });
  }

  test('may navigate only to module documents, and the page still knows it by its frame', async ({
    page,
  }) => {
    // `frame-src` confines a frame to `/m/`, not to its own platform's part
    // of it: a module can load another platform's module document into its
    // own frame. Recorded rather than prevented, because it gives nothing
    // away. The document runs under its own platform's CSP, in the same
    // sandbox, and the page still attributes the frame to the platform that
    // owns the panel: it is never sent `init` again, and anything it asked
    // would be carried to the first platform, not its own.
    const baseURL = test.info().project.use.baseURL;
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');
    const target = `${baseURL}/m/stampmill/ui/probe/index.html`;
    await hostile.evaluate((url) => window.hostile.navigate(url), target).catch(() => {});

    const handle = await byKey(page, 'orebank', 'module-hostile').locator('iframe').elementHandle();
    await expect.poll(async () => (await handle.contentFrame()).url()).toBe(target);
    const swapped = await handle.contentFrame();
    await page.waitForTimeout(1_500);
    // The sandbox came with it: an opaque origin, still.
    expect(await swapped.evaluate(() => window.origin)).toBe('null');
    await expect(swapped.locator('#state')).toHaveAttribute('data-state', 'waiting');
  });

  test('cannot call the request proxy directly, even with the module CSP taken away', async ({
    browser,
  }) => {
    // The module CSP stops a frame's `fetch` of `/p/` before it is made (see
    // the network row). This takes the CSP away, as a browser bug might, to
    // show what stands behind it: the request leaves the frame without the
    // viewer's session, `Sec-Fetch-Site` says it is not the shell's page, and
    // the proxy refuses it as `not_from_shell`. And the platform, asked
    // directly, refuses a caller with no token from the shell.
    test.skip(
      test.info().project.name === 'webkit',
      'Playwright cannot take a frame’s CSP away in WebKit, where the CSP stops it first',
    );
    const baseURL = test.info().project.use.baseURL;
    const context = await browser.newContext({ baseURL, bypassCSP: true });
    const page = await context.newPage();
    const refusals = {};
    page.on('response', async (response) => {
      if (response.frame() === page.mainFrame()) {
        // The shell page asking the proxy for the probe, as it should.
        return;
      }
      if (response.url().includes('/p/orebank/api/module/whoami')) {
        refusals[response.request().method()] = { status: response.status() };
      }
      if (response.url().startsWith(`${OREBANK}/api/module/whoami`)) {
        refusals.platform = { status: response.status() };
      }
    });
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    await attempt(hostile, 'direct', '/p/orebank/api/module/whoami', {
      credentials: 'include',
      mode: 'no-cors',
    });
    await attempt(hostile, 'direct', '/p/orebank/api/module/whoami', {
      method: 'POST',
      credentials: 'include',
      mode: 'no-cors',
      body: 'x',
    });
    await attempt(hostile, 'direct', `${OREBANK}/api/module/whoami`, {
      credentials: 'include',
      mode: 'no-cors',
    });

    // A 403 from the proxy on a read is only ever `not_from_shell`: the other
    // 403, `read_only`, is for writes. The frame's answers are opaque, so
    // Playwright is not shown their headers; the status is enough, beside the
    // same read made by the shell's page, which is answered.
    await expect.poll(() => Object.keys(refusals).sort()).toEqual(['GET', 'POST', 'platform']);
    expect(refusals.GET).toEqual({ status: 403 });
    expect(refusals.POST).toEqual({ status: 403 });
    expect(refusals.platform.status).toBe(401);
    const fromThePage = await page.evaluate(async () => {
      const answer = await fetch('/p/orebank/api/module/whoami');
      return answer.status;
    });
    expect(fromThePage).toBe(200);
    await context.close();
  });

  test('cannot reach another platform through the bridge', async ({ page }) => {
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');
    const result = await attempt(hostile, 'otherPlatform', 'stampmill');
    expect(escapes(result)).toEqual([]);
    // The page routed by the frame and ignored the field naming another
    // platform: the answer is its own platform's.
    const field = result.tried.find((one) => one.what.startsWith('a platform field'));
    expect(field.reached).toBe('orebank');
    // Every path that would climb out of the prefix was refused in the page.
    for (const one of result.tried.filter((t) => t !== field && !t.what.startsWith('a query'))) {
      expect(one.refusal, one.what).toBe('outside_prefix');
    }
  });

  test('cannot announce a change as another platform', async ({ page }) => {
    await openSurface(page, layoutId);
    const probe = await frameOf(page, 'orebank', 'module-probe');
    const other = await frameOf(page, 'stampmill', 'module-hostile');
    await attempt(other, 'forgeChange', 'orebank', 'module-probe');
    await page.waitForTimeout(1_500);
    // A change from a module reaches only its own platform's modules, and
    // stampmill's module named orebank's panel: orebank's probe heard nothing
    // from a module.
    expect(await probe.locator('#changes').getAttribute('data-module')).toBeNull();
  });

  test('cannot flood the page with messages or requests', async ({ page }) => {
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');

    const flooded = await attempt(hostile, 'flood', 200);
    expect(flooded.tried[0].refusal).toBe('too_many');

    // A second later, a burst of requests: no more than the frame may have in
    // flight get through.
    await page.waitForTimeout(1_100);
    const burst = await attempt(hostile, 'burst', 30);
    expect(burst.answered).toBeGreaterThan(0);
    expect(burst.answered).toBeLessThanOrEqual(8);
    expect(burst.refused).toBe(30 - burst.answered);
    // The page is still the page.
    await expect(byKey(page, 'orebank', 'module-probe')).toHaveAttribute('data-module', 'ready');
  });

  test('cannot starve the page by holding streams open', async ({ page }) => {
    await openSurface(page, layoutId);
    const one = await frameOf(page, 'orebank', 'module-hostile');
    const two = await frameOf(page, 'stampmill', 'module-hostile');

    // Each frame gets its two streams and no more...
    const first = await attempt(one, 'hold', 3);
    expect([first.open, first.refused]).toEqual([2, 1]);
    const second = await attempt(two, 'hold', 3);
    expect([second.open, second.refused]).toEqual([2, 1]);

    // ...which fills the page's cap over HTTP/1.1, so a third module's stream
    // is refused, while its whole requests and the shell's own stream carry
    // on: a panel the shell draws keeps moving.
    const probe = await frameOf(page, 'orebank', 'module-probe');
    const id = await probe.evaluate(() => window.probeStreams.open('every_ms=1000', { credit: 4096 }));
    await expect
      .poll(async () => (await probe.evaluate((re) => window.probeStreams.get(re), id)).refusal)
      .toBe('too_many');
    const counted = await probe.evaluate(() => window.probeStreams.counts('never-opened'));
    expect(counted).toBeNull();
    const stat = byKey(page, 'orebank', 'records-per-second');
    const before = await stat.textContent();
    await expect.poll(async () => stat.textContent(), { timeout: 15_000 }).not.toBe(before);
  });

  test('a module that throws is its own problem', async ({ page }) => {
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');
    await hostile.evaluate(() => window.hostile.fail());
    await page.waitForTimeout(500);
    await expect(byKey(page, 'orebank', 'module-hostile')).toHaveAttribute('data-module', 'ready');
    expect(errors.filter((message) => !message.includes('hostile module'))).toEqual([]);
  });

  test('a module that spins: measured, and where the browser isolates it, noticed', async ({
    page,
  }) => {
    // Whether a module's busy thread is the page's too is the browser's
    // decision, not the shell's: a sandboxed frame from the page's own site
    // runs in its own process only where the browser isolates sandboxed
    // frames. Chromium does (IsolateSandboxedIframes); its headless shell,
    // Firefox and WebKit, as of this writing, do not, and there a module
    // that spins holds the whole shell page still until it stops, including
    // the page's own heartbeat clock, so not even `stale` can be shown until
    // it is over. The specification records this (*Containment*); this test
    // measures it in whichever browser runs it, and asserts the isolation
    // only in full Chromium (`HLIN_BROWSERS=chromium-full`).
    const isolating = test.info().project.name === 'chromium-full';
    await openSurface(page, layoutId);
    const hostile = await frameOf(page, 'orebank', 'module-hostile');
    const panel = byKey(page, 'orebank', 'module-hostile');
    await page.evaluate(() => {
      const panel = document.querySelector('section.panel[data-panel="orebank/module-hostile"]');
      window.__states = [];
      new MutationObserver(() => window.__states.push(panel.dataset.module)).observe(panel, {
        attributes: true,
        attributeFilter: ['data-module'],
      });
    });

    // The shell page's own frames, counted while the module wedges its
    // thread for five seconds: long enough that a heartbeat must go unanswered
    // (one is sent at most two seconds in, and missed two seconds later), and
    // short enough that three cannot (six seconds), which would tear it down.
    await hostile.evaluate(() => window.hostile.spin(5_000));
    const measured = await page.evaluate(
      () =>
        new Promise((resolve) => {
          const frames = [];
          const start = performance.now();
          const step = (at) => {
            frames.push(at);
            if (at - start < 3_000) {
              requestAnimationFrame(step);
            } else {
              const gaps = frames.slice(1).map((one, i) => one - frames[i]);
              resolve({ frames: frames.length, longest: Math.round(Math.max(...gaps)) });
            }
          };
          requestAnimationFrame(step);
        }),
    );
    // Whatever happened meanwhile, the module is back once it stops.
    await expect(panel).toHaveAttribute('data-module', 'ready', { timeout: 15_000 });
    const states = await page.evaluate(() => window.__states);
    const said =
      `${test.info().project.name}: while a module spun for 5 s the shell page drew ` +
      `${measured.frames} frames in 3 s, longest gap ${measured.longest} ms; ` +
      `panel states ${JSON.stringify(states)}`;
    console.log(said);
    test.info().annotations.push({ type: 'spin', description: said });

    if (isolating) {
      // The page kept drawing, and its heartbeat saw the module go quiet.
      expect(measured.longest).toBeLessThan(1_000);
      expect(states).toContain('stale');
    }
    await shot(page, 85, 'containment');
  });
});
