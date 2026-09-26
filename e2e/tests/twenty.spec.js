// Twenty platforms on one surface (HLIN-I-0012).
//
// Run against `angreal demo up --with twenty` by
//
//   angreal e2e twenty
//
// or against the same twenty in containers, `angreal demo up --with
// twenty-compose` (HLIN-I-0013), by
//
//   angreal e2e twenty --against compose
//
// which signs in through Dex first and reaches the widgets' own UIs on the
// compose network (twenty-deployment.js says how). Skipped elsewhere, like
// the sign-in suites.
//
// Three claims. Every widget on the published "Twenty" surface reaches
// `ready`: its platform served a module, the shell hosted it, and it drew.
// A shared widget is in step between browsers: a counter bumped, a vote cast
// or a kanban card added in one browser shows in another that is already
// open, without a reload. And the sparkline, the one widget that declares
// `time_range`, follows the surface's time picker. What carries it is the widget's own event stream, followed by the
// shell and delivered to each page's module as a bridge `changed`, which the
// module answers by fetching again.
//
// Both browsers are the same person: the development user, which is all
// `dev` sign-in has, or Alice, signed in through Dex, in containers. That
// proves the same thing two people would: the relay is per surface, not per
// person, and the second browser learns of the change only through it.
//
// The widgets keep their state in memory for as long as the demo runs, so
// nothing here assumes where the counter or the poll starts.

const { test, expect } = require('@playwright/test');
const { shot, shotOf } = require('./helpers');
const { WIDGETS, ownOrigin, ownContext, ownRequest } = require('./twenty-deployment');

/** How long another browser may take to show a change. */
const PROPAGATION = 15_000;

/** How long a module may take to reach `ready`: a wasm bundle to compile. */
const READY = 30_000;

const widget = (page, platform, key) =>
  page.locator(`section.panel[data-panel="${platform}/${key}"]`);
const inside = (panel) => panel.frameLocator('iframe');

/** Opens the surface and waits for these widgets to say they are ready. */
async function openSurface(page, surface, panels) {
  await page.goto(surface);
  await page.locator('header.bar').waitFor({ state: 'visible' });
  for (const { platform_id, panel_key } of panels) {
    const panel = widget(page, platform_id, panel_key);
    // Scrolled to first: the surface is taller than the screen, and a widget
    // below it may not be mounted until it is in view.
    await panel.scrollIntoViewIfNeeded();
    await expect(panel, `${platform_id}/${panel_key}`).toHaveAttribute('data-module', 'ready', {
      timeout: READY,
    });
  }
  await page.evaluate(() => window.scrollTo(0, 0));
}

/**
 * Waits until every widget in view has drawn something: its frame holds more
 * than the loading line.
 */
async function drawnInView(page) {
  const names = await page.evaluate(() =>
    Array.from(document.querySelectorAll('section.panel'))
      .filter((panel) => {
        const box = panel.getBoundingClientRect();
        return box.bottom > 0 && box.top < window.innerHeight;
      })
      .map((panel) => panel.dataset.panel),
  );
  for (const name of names) {
    const [platform, key] = name.split('/');
    await expect
      .poll(
        () =>
          inside(widget(page, platform, key))
            .locator('main.widget')
            .innerText()
            .catch(() => ''),
        { timeout: READY },
      )
      .not.toMatch(/^\s*(Loading…)?\s*$/);
  }
  // A frame's document holding its content is a frame or two before it is
  // painted.
  await page.waitForTimeout(250);
}

/**
 * Fails the test if the page navigates again after this.
 *
 * "Without a reload" is the claim, so it is checked rather than assumed.
 */
function forbidReloads(page, who) {
  let loads = 0;
  page.on('load', () => {
    loads += 1;
  });
  return () => expect(loads, `${who}'s page reloaded`).toBe(0);
}

/** Waits for another browser to show a change, and says how long it took. */
async function propagated(what, assertion) {
  const started = Date.now();
  await assertion({ timeout: PROPAGATION });
  const took = Date.now() - started;
  console.log(`  ${what}: ${took} ms`);
  test.info().annotations.push({ type: 'propagation', description: `${what}: ${took} ms` });
}

test.describe('twenty widgets on one surface', () => {
  test.describe.configure({ mode: 'serial' });

  let surface;
  let panels;

  test.beforeAll(async ({ request }) => {
    const offered = await (await request.get('/api/panels')).json();
    const ids = new Set(offered.map((platform) => platform.id));
    test.skip(!(ids.has('counter') && ids.has('poll')), 'this is not the twenty-widget demo');

    const layouts = await (await request.get('/api/layouts')).json();
    const twenty = layouts.find((layout) => layout.title === 'Twenty');
    expect(twenty, '`demo up --with twenty` publishes "Twenty"').toBeTruthy();
    surface = `/s/${twenty.id}`;

    const layout = await (await request.get(`/api/layouts/${twenty.id}`)).json();
    expect(layout.visibility).toBe('published');
    panels = layout.panels;
    // Every widget the shell offers is on it: `WIDGETS` is the one list.
    for (const platform of offered) {
      expect(
        panels.some((panel) => panel.platform_id === platform.id),
        `${platform.id} is on "Twenty"`,
      ).toBe(true);
    }
  });

  test('every widget on the published surface is ready', async ({ page }) => {
    const started = Date.now();
    await openSurface(page, surface, panels);
    console.log(`  ${panels.length} widgets ready in ${Date.now() - started} ms`);

    // Photographed once what is in view has drawn, not at `ready`: the shell
    // marks a panel ready on the module's handshake, a moment before its
    // frame paints, so a shot then shows empty frames. And a screenful at a
    // time, because a whole-page shot does not composite sandboxed frames.
    for (const [order, where] of [
      [200, 'top'],
      [201, 'middle'],
      [202, 'bottom'],
    ]) {
      await page.evaluate((to) => {
        const most = document.documentElement.scrollHeight - window.innerHeight;
        window.scrollTo(0, { top: 0, middle: most / 2, bottom: most }[to]);
      }, where);
      await drawnInView(page);
      await shot(page, order, `twenty-${where}`);
    }
  });

  test('the counter and the poll change in a second browser without a reload', async ({
    browser,
  }) => {
    const first = await (await browser.newContext()).newPage();
    const second = await (await browser.newContext()).newPage();
    await openSurface(first, surface, panels);
    await openSurface(second, surface, panels);
    const firstStayed = forbidReloads(first, 'the first browser');
    const secondStayed = forbidReloads(second, 'the second browser');

    // The counter: bumped in one, seen in the other.
    const counterHere = inside(widget(first, 'counter', 'counter'));
    const counterThere = inside(widget(second, 'counter', 'counter'));
    const before = Number(await counterThere.locator('.w-big').getAttribute('data-value'));
    await counterHere.getByRole('button', { name: 'Bump up' }).click();
    await expect(counterHere.locator('.w-big')).toHaveAttribute('data-value', String(before + 1));
    await propagated('a bump reaches the second browser', (options) =>
      expect(counterThere.locator('.w-big')).toHaveAttribute(
        'data-value',
        String(before + 1),
        options,
      ),
    );
    await shot(second, 210, 'twenty-counter-in-step');

    // The poll: a vote for whichever option is not already ours, so it
    // always moves a count, however the poll was left.
    const pollHere = inside(widget(first, 'poll', 'poll'));
    const pollThere = inside(widget(second, 'poll', 'poll'));
    const target = pollHere.locator('li.poll__option:not(.poll__option--mine)').first();
    const option = await target.getAttribute('data-option');
    const votesThere = pollThere.locator(`li[data-option="${option}"] .poll__votes`);
    const votesBefore = Number(await votesThere.textContent());
    await target.getByRole('button').click();
    await expect(pollHere.locator(`li[data-option="${option}"]`)).toHaveClass(/poll__option--mine/);
    await propagated('a vote reaches the second browser', (options) =>
      expect(votesThere).toHaveText(String(votesBefore + 1), options),
    );
    await expect(pollThere.locator(`li[data-option="${option}"]`)).toHaveClass(
      /poll__option--mine/,
    );
    await shot(second, 211, 'twenty-poll-in-step');

    firstStayed();
    secondStayed();
  });

  test('the deploy log streams lines in as they happen, without a reload', async ({ page }) => {
    // The deploy widget's module holds one streamed read open through the
    // shell (HLIN-S-0007, *Streaming*), and its platform writes a line about
    // once a second. So lines keep arriving on a page nobody touches.
    test.skip(
      !panels.some((panel) => panel.platform_id === 'deploys'),
      'the deploy widget is not on this surface',
    );
    await openSurface(page, surface, panels);
    const stayed = forbidReloads(page, 'the page');
    const panel = widget(page, 'deploys', 'deploys');
    await panel.scrollIntoViewIfNeeded();
    const log = inside(panel);
    await expect(log.locator('.deploys__status--live')).toBeVisible({ timeout: READY });

    const newest = async () =>
      Number(await log.locator('li.deploys__line').first().getAttribute('data-seq'));
    await expect(log.locator('li.deploys__line').first()).toBeVisible();
    const before = await newest();
    await propagated('three more deploy lines arrive', (options) =>
      expect.poll(newest, options).toBeGreaterThanOrEqual(before + 3),
    );
    await shotOf(panel, 218, 'twenty-deploys-streaming');

    stayed();
  });

  test('a shout reaches a second browser without a reload', async ({ browser }) => {
    const first = await (await browser.newContext()).newPage();
    const second = await (await browser.newContext()).newPage();
    await openSurface(first, surface, panels);
    await openSurface(second, surface, panels);
    const firstStayed = forbidReloads(first, 'the first browser');
    const secondStayed = forbidReloads(second, 'the second browser');

    // Words nobody has said before, so the check cannot pass on an old shout.
    const words = `Anyone for coffee? ${Date.now()}`;
    const here = inside(widget(first, 'shoutbox', 'shoutbox'));
    const there = inside(widget(second, 'shoutbox', 'shoutbox'));
    await widget(first, 'shoutbox', 'shoutbox').scrollIntoViewIfNeeded();
    await widget(second, 'shoutbox', 'shoutbox').scrollIntoViewIfNeeded();
    await here.getByRole('textbox', { name: 'Say something' }).fill(words);
    await here.getByRole('button', { name: 'Send' }).click();
    const mine = here.locator('li.shoutbox__shout', { hasText: words });
    await expect(mine).toHaveClass(/shoutbox__shout--mine/);
    await expect(here.getByRole('textbox', { name: 'Say something' })).toHaveValue('');
    await propagated('a shout reaches the second browser', (options) =>
      expect(there.locator('li.shoutbox__shout', { hasText: words })).toBeVisible(options),
    );
    await shot(second, 212, 'twenty-shout-in-step');

    firstStayed();
    secondStayed();
  });

  test('a kanban card added and taken off in one browser is in step in another', async ({
    browser,
  }) => {
    const first = await (await browser.newContext()).newPage();
    const second = await (await browser.newContext()).newPage();
    await openSurface(first, surface, panels);
    await openSurface(second, surface, panels);
    const firstStayed = forbidReloads(first, 'the first browser');
    const secondStayed = forbidReloads(second, 'the second browser');

    const here = inside(widget(first, 'kanban', 'kanban'));
    const there = inside(widget(second, 'kanban', 'kanban'));
    await widget(first, 'kanban', 'kanban').scrollIntoViewIfNeeded();
    await widget(second, 'kanban', 'kanban').scrollIntoViewIfNeeded();

    // A card nobody else could have added, so its arrival is this test's.
    const text = `Checked in step ${Date.now()}`;
    await here.getByRole('textbox', { name: 'New card' }).fill(text);
    await here.getByRole('button', { name: 'Add', exact: true }).click();
    await expect(here.locator('li.kanban__card', { hasText: text })).toBeVisible();
    await propagated('a card reaches the second browser', (options) =>
      expect(there.locator('li.kanban__card', { hasText: text })).toBeVisible(options),
    );
    await shot(second, 212, 'twenty-kanban-in-step');

    // Taken off again, which also keeps the column from filling up over runs.
    await here.getByRole('button', { name: `Take off “${text}”` }).click();
    await propagated('a card taken off leaves the second browser', (options) =>
      expect(there.locator('li.kanban__card', { hasText: text })).toHaveCount(0, options),
    );

    firstStayed();
    secondStayed();
  });

  test('the sparkline follows the time picker', async ({ page }) => {
    await openSurface(page, surface, panels);
    const panel = widget(page, 'sparkline', 'sparkline');
    await panel.scrollIntoViewIfNeeded();
    const line = inside(panel).locator('.sparkline');

    // One panel on the surface declares `time_range`, so the bar has a
    // picker, and its default hour reaches the module: a point a minute.
    await expect(line).toHaveAttribute('data-step', '60000');
    await expect.poll(async () => Number(await line.getAttribute('data-points'))).toBeGreaterThan(55);

    await page.locator('.bar .picker button', { hasText: '15m' }).click();
    await expect.poll(async () => Number(await line.getAttribute('data-points'))).toBeLessThan(17);

    // A day: still at most sixty points, so twenty-four minutes apart.
    await page.locator('.bar .picker button', { hasText: '24h' }).click();
    await expect(line).toHaveAttribute('data-step', String(24 * 60_000));
    await shot(page, 213, 'twenty-sparkline-day');
  });

  // HLIN-I-0013: a converted widget serves its own UI at the root of its
  // origin, and Hlin's surface under `/hlin`. Its Hlin module is the same
  // components with another client, so the two are one widget: a change in
  // either is in the other, through nothing but the platform.
  test("a converted widget's own UI is at its own root, and in step with its module", async ({
    browser,
    playwright,
  }) => {
    // Asked from where a person's browser would ask: this machine, or, in
    // containers, the compose network, the only thing that reaches them.
    const request = await ownRequest(playwright);
    for (const name of WIDGETS) {
      const origin = ownOrigin(name);
      const root = await request.get(`${origin}/`);
      expect(root.status(), `${name}: / is its own UI`).toBe(200);
      expect(await root.text()).toContain('<title>');
      const page = await request.get(`${origin}/nope`);
      expect(page.status(), `${name}: /nope is its own UI`).toBe(200);
      expect(await page.text()).toContain('<title>');
      const hlin = await request.get(`${origin}/hlin/nope`);
      expect(hlin.status(), `${name}: /hlin/nope`).toBe(404);
      expect(await hlin.text()).not.toContain('<title>');
    }
    await request.dispose();

    const surfacePage = await (await browser.newContext()).newPage();
    await openSurface(surfacePage, surface, panels);
    const own = await (await ownContext(browser, { viewport: { width: 420, height: 360 } })).newPage();

    // The clock and the poll draw at their own roots.
    await own.goto(`${ownOrigin('poll')}/`);
    await expect(own.locator('li.poll__option').first()).toBeVisible({ timeout: READY });
    await own.goto(`${ownOrigin('clock')}/`);
    await expect(own.locator('li.clock').first()).toBeVisible({ timeout: READY });
    await expect(own.locator('.clock__time').first()).toHaveText(/^\d\d:\d\d:\d\d$/);

    // The counter, both ways.
    await own.goto(`${ownOrigin('counter')}/`);
    const ownNumber = own.locator('.w-big');
    await expect(ownNumber).toHaveAttribute('data-value', /\d+/, { timeout: READY });
    const module = inside(widget(surfacePage, 'counter', 'counter'));
    const before = Number(await ownNumber.getAttribute('data-value'));
    await own.getByRole('button', { name: 'Bump up' }).click();
    await propagated('a bump on its own page reaches its module', (options) =>
      expect(module.locator('.w-big')).toHaveAttribute('data-value', String(before + 1), options),
    );
    await module.getByRole('button', { name: 'Bump up' }).click();
    await propagated('a bump in its module reaches its own page', (options) =>
      expect(ownNumber).toHaveAttribute('data-value', String(before + 2), options),
    );
    await shot(own, 214, 'twenty-counter-own-ui');

    // The clock's own page beside its module on "Twenty", as one picture.
    await own.goto(`${ownOrigin('clock')}/`);
    await expect(own.locator('li.clock').first()).toBeVisible({ timeout: READY });
    const ownShot = await own.screenshot();
    const moduleShot = await widget(surfacePage, 'clock', 'clock').screenshot();
    const both = await (await browser.newContext()).newPage();
    await both.setContent(
      `<body style="margin:0;display:flex;gap:16px;padding:16px;background:#0d1015;align-items:flex-start;font:13px system-ui;color:#8b95a3">` +
        `<figure style="margin:0"><img src="data:image/png;base64,${ownShot.toString('base64')}"><figcaption>${new URL(ownOrigin('clock')).host}/ (its own UI)</figcaption></figure>` +
        `<figure style="margin:0"><img src="data:image/png;base64,${moduleShot.toString('base64')}"><figcaption>its Hlin module on "Twenty"</figcaption></figure>` +
        `</body>`,
    );
    await both.setViewportSize({ width: 1000, height: 420 });
    await shot(both, 215, 'twenty-clock-own-ui-beside-module');
  });

  // HLIN-T-0096: widgets four to twelve draw at their own roots with the
  // direct client, and a roll on the dice's own page reaches its module.
  test('widgets four to twelve draw their own UIs, and the dice roll both ways', async ({ browser }) => {
    const drawn = {
      notes: '.note',
      dice: '.dice__latest',
      stopwatch: '.stopwatch',
      quote: '.quote',
      sparkline: '.sparkline',
      kanban: '.kanban__head',
      status: '.status',
      weather: '.weather__now',
      pomodoro: '.pomodoro',
    };
    const own = await (await ownContext(browser, { viewport: { width: 420, height: 360 } })).newPage();
    for (const [name, selector] of Object.entries(drawn)) {
      await own.goto(`${ownOrigin(name)}/`);
      await expect(own.locator(selector).first(), `${name}'s own UI`).toBeVisible({ timeout: READY });
      await expect(own.locator('.w-refusal, .w-failed'), `${name}'s own UI`).toHaveCount(0);
    }

    const surfacePage = await (await browser.newContext()).newPage();
    await openSurface(surfacePage, surface, panels);
    const module = inside(widget(surfacePage, 'dice', 'dice'));
    await expect(module.locator('.dice__latest')).toBeVisible({ timeout: READY });
    await own.goto(`${ownOrigin('dice')}/`);
    const ownLatest = own.locator('.dice__latest');
    await expect(ownLatest).toBeVisible({ timeout: READY });
    const before = Number(await ownLatest.getAttribute('data-number'));
    await own.getByRole('button', { name: 'Roll' }).click();
    await expect(ownLatest).toHaveAttribute('data-number', String(before + 1));
    await propagated('a roll on its own page reaches its module', (options) =>
      expect(module.locator('.dice__latest')).toHaveAttribute('data-number', String(before + 1), options),
    );
  });

  // HLIN-T-0097: widgets thirteen to twenty at their own roots. Each draws
  // there with the direct client; the deploy log streams its own `/api/` as
  // its module streams through the bridge; a shout said on the shoutbox's own
  // page is in its module on "Twenty".
  test('widgets thirteen to twenty draw at their own roots, the log streaming', async ({
    browser,
  }) => {
    const own = await (await ownContext(browser, { viewport: { width: 520, height: 480 } })).newPage();
    const drawn = {
      shoutbox: '.shoutbox__say',
      reactions: 'button.reactions__pill',
      bookmarks: '.bookmarks__add',
      oncall: '.oncall__name',
      picker: '.picker__stage',
      deploys: 'li.deploys__line',
      converter: '.converter__answer',
      meetings: 'ul.meetings, main.widget > p.w-quiet',
    };
    for (const [name, selector] of Object.entries(drawn)) {
      await own.goto(`${ownOrigin(name)}/`);
      await expect(own.locator(selector).first(), `${name} at its own root`).toBeVisible({
        timeout: READY,
      });
      await expect(own.locator('.w-refusal, .w-failed'), `${name}: nothing refused`).toHaveCount(0);
    }

    // The converter works it out in the page, asking for nothing.
    await own.goto(`${ownOrigin('converter')}/`);
    const asked = [];
    own.on('request', (request) => asked.push(request.url()));
    await own.getByRole('textbox', { name: 'Value' }).fill('2');
    await expect(own.locator('.converter__answer')).toHaveAttribute('data-value', /\d/);
    expect(asked, 'the converter asks its platform for nothing').toEqual([]);

    // The deploy log, on its own page: lines keep arriving on a stream of its
    // own `/api/`, as they do in its module through the bridge (above).
    await own.goto(`${ownOrigin('deploys')}/`);
    const stayed = forbidReloads(own, 'the deploy log');
    await expect(own.locator('.deploys__status--live')).toBeVisible({ timeout: READY });
    const newest = async () =>
      Number(await own.locator('li.deploys__line').first().getAttribute('data-seq'));
    const before = await newest();
    await propagated('three more deploy lines arrive on its own page', (options) =>
      expect.poll(newest, options).toBeGreaterThanOrEqual(before + 3),
    );
    await shot(own, 216, 'twenty-deploys-own-ui-streaming');
    stayed();

    // A shout on the shoutbox's own page, in its module on "Twenty".
    const surfacePage = await (await browser.newContext()).newPage();
    await openSurface(surfacePage, surface, panels);
    const module = inside(widget(surfacePage, 'shoutbox', 'shoutbox'));
    await widget(surfacePage, 'shoutbox', 'shoutbox').scrollIntoViewIfNeeded();
    await own.goto(`${ownOrigin('shoutbox')}/`);
    const words = `Said on its own page ${Date.now()}`;
    await own.getByRole('textbox', { name: 'Say something' }).fill(words);
    await own.getByRole('button', { name: 'Send' }).click();
    await expect(own.locator('li.shoutbox__shout--mine', { hasText: words })).toBeVisible();
    await propagated('a shout on its own page reaches its module', (options) =>
      expect(module.locator('li.shoutbox__shout', { hasText: words })).toBeVisible(options),
    );
  });
});
