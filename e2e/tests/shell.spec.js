// The shell at `/` (HLIN-T-0115, decision HLIN-A-0015, D13).
//
// `/` is a surface of shell components: Hlin's own by default — a bar, a rail
// of the person's surfaces and the platforms' pages, and their home surface
// in `main` — or the operator's, from `default_surface` in hlin.toml. Edit
// composes the surface open in the shell's first content component.
//
// The default shell is checked on every flavour but `shell`; the operator's
// on `angreal demo up --with shell`, whose demo/shell.json is titled
// "Operator shell". Each test skips on the flavour that is not its own.

const { test, expect } = require('@playwright/test');
const { shot } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const item = (page, id) => page.locator(`section.item[data-item="${id}"]`);

async function served(request) {
  return (await request.get('/api/shell')).json();
}

test.describe('the shell at /', () => {
  test("Hlin's own: a bar, a rail and the person's home surface, which Edit composes", async ({
    page,
    request,
  }) => {
    const shell = await served(request);
    test.skip(shell.title === 'Operator shell', "this shell is the operator's");

    const home = await (await request.get('/api/layouts/home')).json();

    await page.goto('/');
    // One bar: the shell's own.
    await expect(page.locator('header.bar')).toHaveCount(1);
    await expect(item(page, 'bar').locator('header.bar')).toBeVisible();

    const rail = page.locator('nav.page-selector[data-style="rail"][data-target="main"]');
    await expect(rail).toBeVisible();
    await expect(rail.locator('.selector-group-label').first()).toHaveText('Surfaces');
    await expect(rail.locator(`a.selector-page[data-address="/s/${home.id}"]`)).toHaveText(home.title);
    // The platforms' pages, a group each, and only those a module draws.
    await expect(rail.locator('a.selector-page[data-address="/page/orebank/module-page"]')).toBeVisible();
    await expect(rail.locator('a.selector-page[data-address="/page/orebank/overview"]')).toHaveCount(0);

    // `main` shows the home surface, which is current in the rail.
    await expect(rail.locator('a.selector-page[aria-current="page"]')).toHaveAttribute(
      'data-address',
      `/s/${home.id}`,
    );
    await expect(item(page, 'main').locator('.stage')).toBeVisible();
    await expect(page).toHaveURL(/\/$/);
    await shot(page, 1, 'shell-default');

    // Edit composes the home surface, in `main`, and the change is stored there.
    const edit = page.locator('header.bar button.mode:not(.sign-out)');
    await expect(edit).toHaveText('Edit');
    await edit.click();
    const catalog = item(page, 'main').locator('.catalog');
    await expect(catalog).toBeVisible();
    const before = (await (await request.get(`/api/layouts/${home.id}`)).json()).items.length;
    await catalog.locator('button.offer').first().click();
    await expect
      .poll(async () => (await (await request.get(`/api/layouts/${home.id}`)).json()).items.length, {
        timeout: 15_000,
      })
      .toBe(before + 1);
    await edit.click();
    await expect(catalog).toHaveCount(0);

    // Put it back as it was.
    const stored = await (await request.get(`/api/layouts/${home.id}`)).json();
    stored.items = stored.items.slice(0, before);
    await request.put(`/api/layouts/${home.id}`, { data: stored });
  });

  test("the operator's, from default_surface", async ({ page, request }) => {
    const shell = await served(request);
    test.skip(shell.title !== 'Operator shell', 'run with `angreal demo up --with shell`');

    await page.goto('/');
    const tabs = page.locator('nav.page-selector[data-style="tabs"][data-target="main"]');
    await expect(tabs.locator('a.selector-page')).toHaveText(['About', 'Module page']);
    await expect(item(page, 'main').locator('.demo-about')).toBeVisible();

    await tabs.locator('a.selector-page', { hasText: 'Module page' }).click();
    await expect(page).toHaveURL(/\/\?main=\/page\/orebank\/module-page$/);
    await expect(item(page, 'main').locator('section.page[data-page="orebank/module-page"]')).toBeVisible();
    await shot(page, 2, 'shell-operator');
  });
});
