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
// and the shell draws both panels, as tables, with each platform's data. And
// the same surface is not the same for everybody: Carol is not on the team
// list, so the checklist refuses her its panel while the feed, which anyone
// signed in may read, shows her the posts. Neither rule is the shell's.

const { test, expect } = require('@playwright/test');
const { shot } = require('./helpers');

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

    // Side by side, and drawn by the shell: a table each, with the data.
    await expect(checklist(page)).toHaveAttribute('data-state', 'ready');
    await expect(feed(page)).toHaveAttribute('data-state', 'ready');
    await expect(checklist(page).locator('table')).toBeVisible();
    await expect(feed(page).locator('table')).toBeVisible();
    await expect(checklist(page)).toContainText(TEAM_ITEM);
    await expect(feed(page)).toContainText(POST);
    expect(Number(await checklist(page).getAttribute('data-y'))).toBe(
      Number(await feed(page).getAttribute('data-y')),
    );
    await shot(page, 97, 'collab-alice');
    await alice.close();

    // Carol, opening the same published surface.
    const carol = await browser.newContext();
    const other = await carol.newPage();
    await signIn(other, 'carol@elsewhere.org', 'Carol');
    await other.goto(surface);
    await other.locator('header.bar').waitFor({ state: 'visible' });

    await expect(checklist(other)).toHaveAttribute('data-state', 'unavailable');
    await expect(checklist(other)).toContainText('you do not have access to this panel');
    await expect(checklist(other)).not.toContainText(TEAM_ITEM);

    await expect(feed(other)).toHaveAttribute('data-state', 'ready');
    await expect(feed(other)).toContainText(POST);
    await shot(other, 98, 'collab-carol');
    await carol.close();
  });
});
