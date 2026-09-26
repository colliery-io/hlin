// The collaborative demo's story, as two people would do it (HLIN-T-0077).
//
// Run against `angreal demo up --with collab` by
//
//   angreal e2e walkthrough
//
// and skipped elsewhere, like the other sign-in suites.
//
// What it claims is HLIN-I-0010's exit criterion: Alice and Bob are signed in
// through Dex in two browsers at once, on the same published surface. Alice
// adds an item and it appears in Bob's checklist without his page being
// reloaded; Bob crosses it off and Alice's checklist shows it crossed. Bob
// tries to edit Alice's post and the feed refuses him, in its words. Carol,
// in a third browser, reads the feed, is refused when she posts, and is
// refused the team list.
//
// Neither page is reloaded after it opens. What carries a change from one
// browser to the other is the platform's own event stream, followed by the
// shell and delivered to each page's module as a bridge `changed`, which the
// module answers by fetching again (HLIN-T-0067). Nothing here waits a fixed
// time: each step waits, up to a bound, for the other browser to show it, and
// the time it took is printed.
//
// Newcomers land on a surface of their own (HLIN-T-0076), so Bob and Carol
// reach "The team" by its link, which is where Alice lands.
//
// The platforms keep their state in memory for as long as the demo runs, so
// everything written here is named for this run.

const { test, expect } = require('@playwright/test');
const { shot, signIn } = require('./helpers');

/** How long another browser may take to show a change. */
const PROPAGATION = 15_000;

const checklist = (page) => page.locator('section.panel[data-panel="checklist/items"]');
const feed = (page) => page.locator('section.panel[data-panel="feed/posts"]');
const inside = (panel) => panel.frameLocator('iframe');

/** Opens the surface and waits for both modules to say they are ready. */
async function openSurface(page, surface) {
  await page.goto(surface);
  await page.locator('header.bar').waitFor({ state: 'visible' });
  await expect(checklist(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
  await expect(feed(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
}

/** A person in a browser of their own, on the surface. */
async function arrive(browser, email, name, surface) {
  const context = await browser.newContext();
  const page = await context.newPage();
  await signIn(page, email, name);
  await openSurface(page, surface);
  return { context, page };
}

/**
 * Fails the test if the page navigates again after this.
 *
 * "Without a reload" is the claim, so it is checked rather than assumed: a
 * shell that recovered from something by reloading the page would pass every
 * other assertion here.
 */
function forbidReloads(page, who) {
  let loads = 0;
  page.on('load', () => {
    loads += 1;
  });
  return () => expect(loads, `${who}'s page reloaded`).toBe(0);
}

/** Waits for an assertion that another browser's change should satisfy, and says how long it took. */
async function propagated(what, assertion) {
  const started = Date.now();
  await assertion({ timeout: PROPAGATION });
  const took = Date.now() - started;
  console.log(`  ${what}: ${took} ms`);
  test.info().annotations.push({ type: 'propagation', description: `${what}: ${took} ms` });
}

test.describe('two people on the collaborative demo, in two browsers', () => {
  test.describe.configure({ mode: 'serial' });

  const run = Date.now().toString(36);
  const item = `Order the cake ${run}`;
  const post = `Retro is on Friday ${run}`;
  let surface;

  test.beforeAll(async ({ request, browser }) => {
    const answer = await request.get('/api/config');
    const body = answer.status() === 401 ? await answer.json() : {};
    test.skip(!body.login, 'this shell does not sign people in itself');

    // Where Alice lands is "The team"; the others open it by its link.
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, 'alice@example.com', 'Alice');
    await page.waitForURL(/\/s\/[0-9a-f-]+$/);
    surface = new URL(page.url()).pathname;
    const home = await (await page.request.get('/api/layouts/home')).json();
    expect(home.title).toBe('The team');
    await context.close();
  });

  test('Alice adds an item, Bob sees it; Bob crosses it off, Alice sees it crossed', async ({
    browser,
  }) => {
    const alice = await arrive(browser, 'alice@example.com', 'Alice', surface);
    const bob = await arrive(browser, 'bob@example.com', 'Bob', surface);
    const aliceStayed = forbidReloads(alice.page, 'Alice');
    const bobStayed = forbidReloads(bob.page, 'Bob');

    const hers = inside(checklist(alice.page));
    const his = inside(checklist(bob.page));
    await expect(his.locator('li.item', { hasText: item })).toHaveCount(0);
    await shot(alice.page, 110, 'walkthrough-alice-before');
    await shot(bob.page, 111, 'walkthrough-bob-before');

    // Alice adds it.
    await hers.getByPlaceholder('Add an item').fill(item);
    await hers.getByRole('button', { name: 'Add' }).click();
    const aliceRow = hers.locator('li.item', { hasText: item });
    await expect(aliceRow).toBeVisible();
    await expect(aliceRow.locator('.item__author')).toHaveText('Alice');
    await shot(alice.page, 112, 'walkthrough-alice-adds');

    // Bob's page, open all along, shows it.
    const bobRow = his.locator('li.item', { hasText: item });
    await propagated("Alice's item reaches Bob", (options) => expect(bobRow).toBeVisible(options));
    await expect(bobRow.locator('.item__author')).toHaveText('Alice');
    await expect(bobRow).not.toHaveClass(/item--done/);
    await shot(bob.page, 113, 'walkthrough-bob-sees-it');

    // Bob crosses it off.
    await bobRow.getByRole('checkbox').check();
    await expect(bobRow).toHaveClass(/item--done/);
    await shot(bob.page, 114, 'walkthrough-bob-crosses-off');

    // Alice's page, open all along, shows it crossed.
    await propagated('Bob crossing it off reaches Alice', (options) =>
      expect(aliceRow).toHaveClass(/item--done/, options),
    );
    await expect(aliceRow.getByRole('checkbox')).toBeChecked();
    await shot(alice.page, 115, 'walkthrough-alice-sees-it-crossed');

    aliceStayed();
    bobStayed();
    await alice.context.close();
    await bob.context.close();
  });

  test("Alice posts; Bob sees it, tries to edit it, and the feed refuses in its words", async ({
    browser,
  }) => {
    const alice = await arrive(browser, 'alice@example.com', 'Alice', surface);
    const bob = await arrive(browser, 'bob@example.com', 'Bob', surface);
    const bobStayed = forbidReloads(bob.page, 'Bob');

    const hers = inside(feed(alice.page));
    await hers.getByPlaceholder('Say something to everyone').fill(post);
    await hers.getByRole('button', { name: 'Post' }).click();
    await expect(hers.locator('li.post', { hasText: post })).toBeVisible();
    await shot(alice.page, 116, 'walkthrough-alice-posts');

    // Bob's feed, open all along, shows it: the feed's events travel the
    // same road as the checklist's.
    const his = inside(feed(bob.page));
    await propagated("Alice's post reaches Bob", (options) =>
      expect(his.locator('li.post', { hasText: post })).toBeVisible(options),
    );

    // By id, because once it is being edited its words are in a text box.
    const id = await his.locator('li.post', { hasText: post }).getAttribute('data-post');
    const target = his.locator(`li.post[data-post="${id}"]`);
    await expect(target.locator('.post__author')).toHaveText('Alice');
    await target.getByRole('button', { name: 'Edit' }).click();
    await target.locator('textarea').fill(`${post}, says Bob`);
    await target.getByRole('button', { name: 'Save' }).click();

    await expect(his.locator('.refusal')).toHaveText('Only the author can edit this post');
    await shot(bob.page, 117, 'walkthrough-bob-refused');

    // Given up on, the post is as Alice wrote it, for both of them.
    await target.getByRole('button', { name: 'Cancel' }).click();
    await expect(target.locator('.post__body')).toHaveText(post);
    await expect(hers.locator('li.post', { hasText: post }).locator('.post__body')).toHaveText(post);

    bobStayed();
    await alice.context.close();
    await bob.context.close();
  });

  test('Carol reads the feed, is refused when she posts, and is refused the team list', async ({
    browser,
  }) => {
    const carol = await arrive(browser, 'carol@elsewhere.org', 'Carol', surface);

    const posts = inside(feed(carol.page));
    await expect(posts.locator('.post__body', { hasText: post })).toBeVisible();
    await expect(posts.locator('.post__body', { hasText: 'Welcome to the feed.' })).toBeVisible();
    await shot(carol.page, 118, 'walkthrough-carol-reads');

    const hello = `Hello from Carol ${run}`;
    await posts.getByPlaceholder('Say something to everyone').fill(hello);
    await posts.getByRole('button', { name: 'Post' }).click();
    await expect(posts.locator('.refusal')).toHaveText('Only people at example.com can post here');
    await expect(posts.locator('li.post', { hasText: hello })).toHaveCount(0);
    await shot(carol.page, 119, 'walkthrough-carol-refused-post');

    // The team list, in the checklist's words, and none of what is on it.
    const list = inside(checklist(carol.page));
    await expect(list.locator('.refusal')).toHaveText(
      'Only members of Team can see or change it, and you are not one.',
    );
    await expect(list.locator('body')).not.toContainText(item);
    await shot(carol.page, 120, 'walkthrough-carol-refused-list');

    await carol.context.close();
  });
});
