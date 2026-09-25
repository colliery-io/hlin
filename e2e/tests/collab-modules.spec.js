// Changing things through the platforms' own modules (HLIN-T-0074, HLIN-T-0075).
//
// Run against `angreal demo up --with collab`, after collab.spec.js, by
//
//   angreal e2e signin
//
// and skipped elsewhere, like the other sign-in suites. Everything is clicked
// and typed inside the modules' frames, so each step is the whole path: the
// module's `fetch`, the page, the shell's request proxy with a token bound to
// the write, the platform's rule, and the module drawing what came back.
//
// What it claims: Alice adds an item in the checklist's module and crosses it
// off, and her module shows each change as the platform made it; Bob, opening
// the surface, sees the list as she left it. Bob's edit of Alice's post is
// refused, and the feed's module shows the feed's own words. Carol may read the
// feed but her post is refused, in the feed's words.
//
// Not claimed here: that Bob's open page follows Alice's change without a
// reload. That needs the shell to relay `changed` between browsers, and is
// HLIN-T-0077's to assert.
//
// The platforms keep their state in memory for as long as the demo runs, so
// everything written here is named for this run, and a second run finds its
// own.

const { test, expect } = require('@playwright/test');
const { shot } = require('./helpers');

const PASSWORD = 'password';

async function signIn(page, email, name) {
  await page.goto('/');
  await page.locator('#login').waitFor({ state: 'visible' });
  await page.locator('#login').fill(email);
  await page.locator('#password').fill(PASSWORD);
  await page.locator('#submit-login').click();
  await page.waitForURL((url) => url.port === '8080');
  await expect(page.locator('header.bar .viewer')).toHaveText(name);
}

const checklist = (page) => page.locator('section.panel[data-panel="checklist/items"]');
const feed = (page) => page.locator('section.panel[data-panel="feed/posts"]');
const inside = (panel) => panel.frameLocator('iframe');

/** Opens the surface and waits for both modules to say they are ready. */
async function openSurface(page, surface) {
  await page.goto(surface);
  await expect(checklist(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
  await expect(feed(page)).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
}

test.describe('changing things through the platforms\' own modules', () => {
  test.describe.configure({ mode: 'serial' });

  const run = Date.now().toString(36);
  const item = `Bring biscuits ${run}`;
  const post = `Standup moved to ten ${run}`;
  let surface;

  test.beforeAll(async ({ request, browser }) => {
    const answer = await request.get('/api/config');
    const body = answer.status() === 401 ? await answer.json() : {};
    test.skip(!body.login, 'this shell does not sign people in itself');

    // Where Alice lands is the published surface; the others open it by link.
    const context = await browser.newContext();
    const page = await context.newPage();
    await signIn(page, 'alice@example.com', 'Alice');
    await page.waitForURL(/\/s\/[0-9a-f-]+$/);
    surface = new URL(page.url()).pathname;
    await context.close();
  });

  test('Alice adds an item and crosses it off; Bob sees it as she left it', async ({ browser }) => {
    const alice = await browser.newContext();
    const page = await alice.newPage();
    await signIn(page, 'alice@example.com', 'Alice');
    await openSurface(page, surface);

    const list = inside(checklist(page));
    await list.getByPlaceholder('Add an item').fill(item);
    await list.getByRole('button', { name: 'Add' }).click();

    // Drawn from the platform's answer, not from what was typed: the row
    // appears once the checklist has it, with the author it recorded.
    const row = list.locator('li.item', { hasText: item });
    await expect(row).toBeVisible();
    await expect(row.locator('.item__author')).toHaveText('Alice');
    await expect(list.getByPlaceholder('Add an item')).toHaveValue('');

    await row.getByRole('checkbox').check();
    await expect(row).toHaveClass(/item--done/);
    await expect(row.getByRole('checkbox')).toBeChecked();
    await shot(page, 99, 'collab-module-checklist');
    await alice.close();

    // Bob, on the same list, opening the surface afterwards.
    const bob = await browser.newContext();
    const other = await bob.newPage();
    await signIn(other, 'bob@example.com', 'Bob');
    await openSurface(other, surface);
    const his = inside(checklist(other)).locator('li.item', { hasText: item });
    await expect(his).toHaveClass(/item--done/);
    // Alice added it and Bob does not own the list, so the checklist would
    // refuse him an edit, and its module does not offer one. Crossing off is
    // for every member.
    await expect(his.getByRole('button', { name: 'Edit' })).toHaveCount(0);
    await expect(his.getByRole('checkbox')).toBeEnabled();
    await bob.close();
  });

  test('Alice edits her item in place, then deletes it', async ({ browser }) => {
    const alice = await browser.newContext();
    const page = await alice.newPage();
    await signIn(page, 'alice@example.com', 'Alice');
    await openSurface(page, surface);

    const list = inside(checklist(page));
    const id = await list.locator('li.item', { hasText: item }).getAttribute('data-item');
    const row = list.locator(`li.item[data-item="${id}"]`);

    await row.getByRole('button', { name: 'Edit' }).click();
    await row.getByRole('textbox', { name: 'Item text' }).fill(`${item}, and tea`);
    await row.getByRole('textbox', { name: 'Item text' }).press('Enter');
    await expect(row.locator('.item__text')).toHaveText(`${item}, and tea`);

    await row.getByRole('button', { name: 'Delete' }).click();
    await expect(row).toHaveCount(0);
    await alice.close();
  });

  test("Bob's edit of Alice's post is refused in the feed's words", async ({ browser }) => {
    const alice = await browser.newContext();
    const page = await alice.newPage();
    await signIn(page, 'alice@example.com', 'Alice');
    await openSurface(page, surface);

    const posts = inside(feed(page));
    await posts.getByPlaceholder('Say something to everyone').fill(post);
    await posts.getByRole('button', { name: 'Post' }).click();
    const mine = posts.locator('li.post', { hasText: post });
    await expect(mine).toBeVisible();
    await expect(mine.locator('.post__author')).toHaveText('Alice');
    // Newest first.
    await expect(posts.locator('li.post').first()).toContainText(post);
    await shot(page, 100, 'collab-module-feed');
    await alice.close();

    const bob = await browser.newContext();
    const other = await bob.newPage();
    await signIn(other, 'bob@example.com', 'Bob');
    await openSurface(other, surface);

    // By id, because once it is being edited its words are in a text box,
    // which is not the post's text.
    const id = await inside(feed(other))
      .locator('li.post', { hasText: post })
      .getAttribute('data-post');
    const hers = inside(feed(other)).locator(`li.post[data-post="${id}"]`);
    await hers.getByRole('button', { name: 'Edit' }).click();
    await hers.locator('textarea').fill(`${post}, says Bob`);
    await hers.getByRole('button', { name: 'Save' }).click();

    await expect(inside(feed(other)).locator('.refusal')).toHaveText(
      'Only the author can edit this post',
    );
    await shot(other, 101, 'collab-module-feed-refused');
    // His words stay in the box, so he does not lose them; given up on, the
    // post is as Alice wrote it.
    await hers.getByRole('button', { name: 'Cancel' }).click();
    await expect(hers.locator('.post__body')).toHaveText(post);
    await bob.close();
  });

  test("Carol reads the feed, and her post is refused in the feed's words", async ({ browser }) => {
    const carol = await browser.newContext();
    const page = await carol.newPage();
    await signIn(page, 'carol@elsewhere.org', 'Carol');
    await openSurface(page, surface);

    const posts = inside(feed(page));
    await expect(posts.locator('.post__body', { hasText: post })).toBeVisible();
    await posts.getByPlaceholder('Say something to everyone').fill(`Hello from Carol ${run}`);
    await posts.getByRole('button', { name: 'Post' }).click();
    await expect(posts.locator('.refusal')).toHaveText('Only people at example.com can post here');
    // The compose box keeps what she typed, so she does not lose it; the
    // posts do not have it.
    await expect(posts.locator('li.post', { hasText: `Hello from Carol ${run}` })).toHaveCount(0);

    // The surface chose the team list, which the checklist refuses her. The
    // picker offers only her own, and choosing it shows it. (The module also
    // asks the shell, with `set-param`, to keep the choice; that the shell
    // does is HLIN-T-0067's.)
    const list = inside(checklist(page));
    await expect(list.locator('.refusal')).toContainText('Only members of Team');
    await list.getByRole('combobox', { name: 'List' }).selectOption({ label: "Carol's list" });
    await expect(list.locator('.item__text', { hasText: 'Renew the domain' })).toBeVisible();
    await carol.close();
  });
});
