// Signing in through a real identity provider.
//
// Run against the collaborative demo, `angreal demo up --with collab`, where
// the shell's authenticator is `oidc` and the provider is Dex. Every test here
// skips unless the shell answers an unsigned request with somewhere to sign
// in, which is what makes it safe to leave in the one suite: against the
// standard demo, where everybody is the development user, it does nothing.
//
//   angreal e2e signin
//
// This is also the first place an id token signed by a real provider is read
// end to end, which is why it checks email as well as the name on screen: the
// shell's unit tests never build a signed token, so `email` reaching the
// principal is proved here or nowhere (HLIN-T-0060).

const { test, expect } = require('@playwright/test');
const { shot } = require('./helpers');

/** The demo's people, as demo/dex.yaml defines them. */
const PEOPLE = [
  { email: 'alice@example.com', name: 'Alice' },
  { email: 'bob@example.com', name: 'Bob' },
  { email: 'carol@elsewhere.org', name: 'Carol' },
];

const PASSWORD = 'password';

test.describe('signing in through Dex', () => {
  test.beforeEach(async ({ request }) => {
    const answer = await request.get('/api/config');
    const body = answer.status() === 401 ? await answer.json() : {};
    test.skip(!body.login, 'this shell does not sign people in itself');
  });

  for (const [index, person] of PEOPLE.entries()) {
    test(`${person.email} signs in, is named, and signs out`, async ({ page }) => {
      // Straight to the shell, as a person would: the frontend asks who this
      // is, is told where to sign in, and goes there.
      await page.goto('/');

      // Dex's own form. One connector, so it skips the chooser.
      await page.locator('#login').waitFor({ state: 'visible' });
      expect(new URL(page.url()).port).toBe('5556');
      await page.locator('#login').fill(person.email);
      await page.locator('#password').fill(PASSWORD);
      await page.locator('#submit-login').click();

      // Back on the shell, as themselves.
      await page.waitForURL((url) => url.port === '8080');
      await expect(page.locator('header.bar .viewer')).toHaveText(person.name);
      await shot(page, 90 + index * 2, `signed-in-${person.name.toLowerCase()}`);

      // The principal carries the provider's email, which is what a platform
      // deciding on it will be told. Asked with the page's own cookies.
      const config = await (await page.request.get('/api/config')).json();
      expect(config.principal.name).toBe(person.name);
      expect(config.principal.email).toBe(person.email);
      expect(config.sign_out).toBe('/auth/logout');

      // Signing out ends the session, not just the cookie: the shell refuses
      // the next request and the browser is sent to sign in again.
      await page.locator('header.bar .sign-out').click();
      await page.locator('#login').waitFor({ state: 'visible' });
      await shot(page, 91 + index * 2, `signed-out-${person.name.toLowerCase()}`);

      const after = await page.request.get('/api/config');
      expect(after.status()).toBe(401);
    });
  }

  test('a session cookie taken before signing out stops working', async ({ page, context }) => {
    await page.goto('/');
    await page.locator('#login').fill(PEOPLE[0].email);
    await page.locator('#password').fill(PASSWORD);
    await page.locator('#submit-login').click();
    await expect(page.locator('header.bar .viewer')).toHaveText(PEOPLE[0].name);

    const taken = (await context.cookies()).find((cookie) => cookie.name === 'hlin_session');
    expect(taken).toBeTruthy();

    await page.locator('header.bar .sign-out').click();
    await page.locator('#login').waitFor({ state: 'visible' });

    // Put the old value back, as somebody who copied it would.
    await context.addCookies([taken]);
    const replayed = await page.request.get('/api/config');
    expect(replayed.status()).toBe(401);
  });
});
