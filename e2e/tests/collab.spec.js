// The collaborative demo's surface: a checklist and a feed, side by side.
//
// Run against `angreal demo up --with collab`, which starts both platforms and
// signs in as Alice to publish the surface this opens. Like signin.spec.js it
// skips unless the shell signs people in, so it does nothing in `angreal e2e
// test` against the standard demo.
//
//   angreal e2e signin
//
// What it claims: Alice lands on the surface without being told where it is,
// and both panels are drawn by their platforms' own modules, each with its
// platform's data. And the same surface is not the same for everybody: Carol
// is not on the team list, so the checklist's module shows her the
// checklist's refusal, in its words, while the feed, which anyone signed in
// may read, shows her the posts. Neither rule is the shell's.
//
// collab-modules.spec.js then changes things through the modules.

const { test, expect } = require('@playwright/test');
const { shot, shotOf } = require('./helpers');

const PASSWORD = 'password';

/** Seeded by hlin-sample-checklist on the `team` list, and by hlin-sample-feed. */
const TEAM_ITEM = "Book a room for Thursday's review";
const POST = 'Welcome to the feed.';

/** Sign in through Dex's form from wherever the shell sends an unsigned page. */
async function signIn(page, email, name) {
  await page.goto('/');
  await page.locator('#login').waitFor({ state: 'visible' });
  await page.locator('#login').fill(email);
  await page.locator('#password').fill(PASSWORD);
  await page.locator('#submit-login').click();
  await page.waitForURL((url) => url.port === '8080');
  await expect(page.locator('header.bar .viewer')).toHaveText(name);
}

function checklist(page) {
  return page.locator('section.panel[data-panel="checklist/items"]');
}

function feed(page) {
  return page.locator('section.panel[data-panel="feed/posts"]');
}

/** Inside a panel's module frame. */
function inside(panel) {
  return panel.frameLocator('iframe');
}

test.describe('the collaborative surface', () => {
  test.beforeEach(async ({ request }) => {
    const answer = await request.get('/api/config');
    const body = answer.status() === 401 ? await answer.json() : {};
    test.skip(!body.login, 'this shell does not sign people in itself');
  });

  test('Alice lands on both panels; Carol is refused the team list', async ({ browser }) => {
    // Alice, who published it: signing in is all it takes to arrive.
    const alice = await browser.newContext();
    const page = await alice.newPage();
    await signIn(page, 'alice@example.com', 'Alice');

    await page.waitForURL(/\/s\/[0-9a-f-]+$/);
    const surface = new URL(page.url()).pathname;

    const home = await (await page.request.get('/api/layouts/home')).json();
    expect(home.visibility).toBe('published');
    expect(home.panels.map((panel) => `${panel.platform_id}/${panel.panel_key}`).sort()).toEqual([
      'checklist/items',
      'feed/posts',
    ]);

    // Side by side, and drawn by each platform's own module, with the data.
    await expect(checklist(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(feed(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(checklist(page)).toHaveAttribute('data-state', 'ready');
    await expect(feed(page)).toHaveAttribute('data-state', 'ready');
    await expect(inside(checklist(page)).locator('.item__text', { hasText: TEAM_ITEM })).toBeVisible();
    await expect(inside(feed(page)).locator('.post__body', { hasText: POST })).toBeVisible();
    expect(Number(await checklist(page).getAttribute('data-y'))).toBe(
      Number(await feed(page).getAttribute('data-y')),
    );
    // A checklist and a feed: nothing on this surface answers to a time
    // range, so the bar carries no time controls at all (HLIN-T-0078).
    // Absent, not disabled: no presets, no date fields, no Apply.
    await expect(page.locator('.bar .picker')).toHaveCount(0);
    await expect(page.locator('.bar .custom')).toHaveCount(0);
    await expect(page.locator('.bar button', { hasText: /^(15m|1h|6h|24h|Apply)$/ })).toHaveCount(0);
    await shotOf(page.locator('header.bar'), 96, 'collab-bar');
    await shot(page, 97, 'collab-alice');
    await alice.close();

    // Carol, opening the same published surface.
    const carol = await browser.newContext();
    const other = await carol.newPage();
    await signIn(other, 'carol@elsewhere.org', 'Carol');
    await other.goto(surface);
    await other.locator('header.bar').waitFor({ state: 'visible' });

    // The checklist's module asks for the team list as Carol, and shows what
    // the checklist said: its words, not the shell's.
    await expect(checklist(other)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(inside(checklist(other)).locator('.refusal')).toHaveText(
      'Only members of Team can see or change it, and you are not one.',
    );
    await expect(inside(checklist(other)).locator('body')).not.toContainText(TEAM_ITEM);

    await expect(feed(other)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    await expect(inside(feed(other)).locator('.post__body', { hasText: POST })).toBeVisible();
    await shot(other, 98, 'collab-carol');
    await carol.close();
  });
});
