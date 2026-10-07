// An operator's brand on the shell, with no rebuild (COLLIERY-I-0609).
//
// Against `angreal demo up --with brand`: the Aurora front end, with a
// `[brand]` from demo/brand/. The claims are read off the page a person sees:
// the name in the bar and the tab, the logo drawn, the brand's accent in the
// chrome and in a module's frame, in light and in dark, and the favicon.
//
// Every other flavour has no brand, and the last test says so: there the page
// is Hlin, and the stylesheet the app always links is empty.

const { test, expect } = require('@playwright/test');
const { freshLayout, discardLayout, shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const PLATFORM = 'orebank';
const NAME = 'Acme Operations';
// demo/brand/brand.css: Aurora's `--ice`, which fills `--hlin-accent`.
const ACCENT = { light: 'rgb(176, 19, 95)', dark: 'rgb(255, 122, 184)' };

/** The colour `role` resolves to on the page, as the browser computes it. */
const resolved = (page, role) =>
  page.evaluate((role) => {
    const probe = document.createElement('span');
    probe.style.display = 'none';
    probe.style.color = `var(${role})`;
    document.body.append(probe);
    const colour = getComputedStyle(probe).color;
    probe.remove();
    return colour;
  }, role);

/** What the shell says its brand is. */
async function brandOf(request) {
  const answer = await request.get('/api/config');
  expect(answer.ok()).toBeTruthy();
  return (await answer.json()).brand;
}

test.describe('a white-labelled shell', () => {
  let layoutId;

  test.beforeAll(async ({ request }) => {
    test.skip((await brandOf(request)).name !== NAME, 'this demo is not the branded one');
    layoutId = await freshLayout(request, 'Branded');
    const current = await (await request.get(`/api/layouts/${layoutId}`)).json();
    current.panels = [
      { platform_id: PLATFORM, panel_key: 'module-context', position: { x: 0, y: 0, w: 6, h: 14 } },
    ];
    expect((await request.put(`/api/layouts/${layoutId}`, { data: current })).ok()).toBeTruthy();
  });

  test.afterAll(async ({ request }) => {
    if (layoutId) await discardLayout(request, layoutId);
  });

  test("the bar, the tab and the favicon are the operator's", async ({ page, request }) => {
    await page.goto(`/s/${layoutId}`);
    const brand = page.locator('header.bar .brand');
    await expect(brand.locator('.brand-name')).toHaveText(NAME);
    await expect(page).toHaveTitle(NAME);
    // Drawn, not merely present: an image that failed has no natural width.
    const logo = brand.locator('img.brand-logo');
    await expect(logo).toHaveAttribute('src', '/brand/logo');
    await expect
      .poll(() => logo.evaluate((image) => image.complete && image.naturalWidth > 0))
      .toBe(true);
    await expect(page.locator('header.bar')).not.toContainText('Hlin');

    const favicon = await request.get('/favicon.ico');
    expect(favicon.ok()).toBeTruthy();
    expect(favicon.headers()['content-type']).toBe('image/svg+xml');
  });

  test("the brand's accent holds in the chrome and in a module, light and dark", async ({
    page,
  }) => {
    await page.goto(`/s/${layoutId}`);
    const context = page.locator(`section.panel[data-panel="${PLATFORM}/module-context"]`);
    await expect(context).toHaveAttribute('data-module', 'ready', { timeout: 20_000 });
    const theme = context.frameLocator('iframe').locator('#theme');
    const toggle = page.locator('header.bar .cl-theme-toggle');

    for (const [choice, scheme, order] of [
      ['Light', 'light', 235],
      ['Dark', 'dark', 236],
    ]) {
      await toggle.getByRole('button', { name: choice }).click();
      await expect(page.locator('html')).toHaveAttribute('data-theme', scheme);
      await expect.poll(() => resolved(page, '--hlin-accent')).toBe(ACCENT[scheme]);
      // The time picker's chosen range is the chrome's own accent.
      await expect(page.locator('header.bar .picker button.active')).toHaveCSS(
        'background-color',
        ACCENT[scheme],
      );
      await expect(theme).toHaveText(new RegExp(`^${scheme}, 11 tokens$`));
      await expect(theme).toHaveAttribute('data-accent', ACCENT[scheme]);
      await shot(page, order, `brand-${scheme}`);
    }
  });
});

test('with no brand, the page is Hlin and the brand stylesheet is empty', async ({
  page,
  request,
}) => {
  test.skip((await brandOf(request)).name !== 'Hlin', 'this demo is branded');
  await page.goto('/');
  await expect(page.locator('header.bar .brand-name')).toHaveText('Hlin');
  await expect(page).toHaveTitle('Hlin');
  await expect(page.locator('header.bar img.brand-logo')).toHaveCount(0);

  const sheet = await request.get('/brand/style.css');
  expect(sheet.ok()).toBeTruthy();
  expect(sheet.headers()['content-type']).toContain('text/css');
  expect(await sheet.text()).toBe('');
  expect((await request.get('/favicon.ico')).status()).toBe(404);
});
