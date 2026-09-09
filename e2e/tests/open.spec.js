// An open shell: nobody signs in, and nothing can be written.
//
// Run against a shell configured with `strategy = "anonymous"`, which is not
// the demo's own — so every test here skips unless `/api/config` says the
// shell it is talking to is read-only. That keeps one suite honest against two
// very different deployments rather than needing a second runner:
//
//   HLIN_URL=http://127.0.0.1:8090 npx playwright test tests/open.spec.js
//
// The claim is about what a person can *see* and *do*, which is why it is here
// and not in the Rust tests: those prove the API refuses a write, and a browser
// that offered Edit anyway would still be a browser that let somebody arrange a
// whole surface before telling them.

const { test, expect } = require('@playwright/test');
const { shot, drewSomething } = require('./helpers');

test.describe('an open shell', () => {
  let config;

  test.beforeEach(async ({ request }) => {
    config = await (await request.get('/api/config')).json();
    test.skip(!config.read_only, 'this shell is not the open one');
  });

  test('names a visitor without anybody signing in', async ({ page }) => {
    await page.goto('/');

    // The cookie is the identity. HttpOnly, so the page cannot read it — which
    // is why this asks the browser's jar rather than `document.cookie`.
    const jar = await page.context().cookies();
    const visitor = jar.find((cookie) => cookie.name === 'hlin_visitor');

    expect(visitor, 'a visitor is given a name').toBeTruthy();
    expect(visitor.httpOnly).toBe(true);
    expect(visitor.value.length).toBeGreaterThan(20);
  });

  test('shows a published surface, with data on it', async ({ page }) => {
    await page.goto('/');

    const panel = page.locator('[data-panel]').first();
    await expect(panel).toHaveAttribute('data-state', 'ready', { timeout: 20_000 });
    await drewSomething(panel);

    await shot(page, 10, 'open-surface');
  });

  // The whole reason the front end is told, rather than finding out at Save.
  test('offers no way to change anything', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('[data-panel]').first()).toHaveAttribute(
      'data-state',
      'ready',
      { timeout: 20_000 },
    );

    await expect(page.locator('button.mode')).toHaveCount(0);
    await expect(page.locator('.catalog')).toHaveCount(0);

    await shot(page, 11, 'open-no-edit');
  });

  test('refuses a write even when the request is made directly', async ({ request }) => {
    // Not through the UI, because the UI no longer offers it. The guard has to
    // be the shell's, or hiding the button is only a suggestion.
    const refused = await request.post('/api/layouts', { data: { title: 'Mine' } });

    expect(refused.status()).toBe(403);
    const said = await refused.json();
    expect(said.error).toBe('read_only');
    expect(said.detail.length).toBeGreaterThan(40);
  });

  // The claim a single browser cannot make. If this fails, every visitor shares
  // one live surface and one person's click moves everybody else's charts —
  // which only shows up once a second person is looking.
  test('two visitors do not share a surface', async ({ browser, baseURL }) => {
    const ada = await browser.newContext({ baseURL });
    const grace = await browser.newContext({ baseURL });

    const one = await ada.newPage();
    const two = await grace.newPage();

    await one.goto('/');
    await two.goto('/');

    for (const page of [one, two]) {
      await expect(page.locator('[data-panel]').first()).toHaveAttribute(
        'data-state',
        'ready',
        { timeout: 20_000 },
      );
    }

    const named = async (context) =>
      (await context.cookies()).find((cookie) => cookie.name === 'hlin_visitor').value;

    expect(await named(ada)).not.toBe(await named(grace));

    // One moves the whole surface's window; the other must not follow.
    const settledOn = async (page) =>
      page.locator('.bar button.active').first().textContent();

    const before = await settledOn(two);
    await one.locator('.bar button', { hasText: '24h' }).click();
    await expect(one.locator('.bar button.active')).toHaveText('24h');

    await two.waitForTimeout(2_000);
    expect(await settledOn(two)).toBe(before);

    await shot(one, 12, 'open-visitor-one');
    await shot(two, 13, 'open-visitor-two');

    await ada.close();
    await grace.close();
  });
});
