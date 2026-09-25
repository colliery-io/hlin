// Twenty platforms on one surface (HLIN-I-0012).
//
// Run against `angreal demo up --with twenty` by
//
//   angreal e2e twenty
//
// and skipped elsewhere, like the sign-in suites.
//
// Two claims. Every widget on the published "Twenty" surface reaches
// `ready`: its platform served a module, the shell hosted it, and it drew.
// And a shared widget is in step between browsers: a counter bumped or a
// vote cast in one browser shows in another that is already open, without a
// reload. What carries it is the widget's own event stream, followed by the
// shell and delivered to each page's module as a bridge `changed`, which the
// module answers by fetching again.
//
// Both browsers are the development user, which is all `dev` sign-in has.
// That proves the same thing two people would: the relay is per surface, not
// per person, and the second browser learns of the change only through it.
//
// The widgets keep their state in memory for as long as the demo runs, so
// nothing here assumes where the counter or the poll starts.

const { test, expect } = require('@playwright/test');
const { shot, shotOf } = require('./helpers');

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
    await shot(page, 200, 'twenty-ready');
    await page.screenshot({
      path: require('path').join(__dirname, '..', 'screenshots', '201-twenty-whole.png'),
      fullPage: true,
    });
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
});
