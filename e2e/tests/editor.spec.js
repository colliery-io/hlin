// Editing a page selector, and a surface as JSON text (HLIN-T-0116).
//
// While a surface is composed, its page selectors show their groups and pages
// as controls: add, rename, reorder, remove. The composing catalogue shows the
// surface as JSON, to edit and apply, download, or load from a file. Text
// that is not a surface is refused with the reason, and nothing changes.
//
// Pack-agnostic: every front end here registers the view `about`.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface } = require('./helpers');

test.describe.configure({ mode: 'serial' });

async function writeItems(request, id, items) {
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.items = items;
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok(), await written.text()).toBeTruthy();
}

async function stored(request, id) {
  return (await request.get(`/api/layouts/${id}`)).json();
}

/** The selector `nav` as stored: [label, [[address-ish, label]]] per group. */
async function groupsOf(request, id) {
  const layout = await stored(request, id);
  const nav = layout.items.find((item) => item.id === 'nav');
  return nav.groups.map((group) => [
    group.label,
    group.pages.map((entry) => [
      entry.surface ? `/s/${entry.surface}` : entry.view ? `/v/${entry.view}` : `/page/${entry.page.platform}/${entry.page.path}`,
      entry.label ?? null,
    ]),
  ]);
}

test.describe('editing a surface', () => {
  const made = [];
  let alpha;
  let edited;

  test.beforeAll(async ({ request }) => {
    alpha = await freshLayout(request, 'Alpha');
    edited = await freshLayout(request, 'Edited');
    made.push(alpha, edited);
    await writeItems(request, edited, [
      {
        type: 'page-selector',
        id: 'nav',
        style: 'rail',
        target: 'main',
        position: { x: 0, y: 0, w: 4, h: 8 },
        groups: [{ label: 'A', pages: [{ surface: alpha }] }],
      },
      { type: 'content', id: 'main', position: { x: 4, y: 0, w: 8, h: 8 }, initial: { surface: alpha } },
    ]);
  });

  test.afterAll(async ({ request }) => {
    for (const id of made) {
      await discardLayout(request, id);
    }
  });

  test("a selector's pages are added, reordered and renamed while composing", async ({ page, request }) => {
    await openSurface(page, edited);
    // Not editable until Edit.
    await expect(page.locator('.selector-editor')).toHaveCount(0);
    await page.locator('header.bar button.mode:not(.sign-out)').click();

    const editor = page.locator('.selector-editor[data-selector="nav"]');
    await expect(editor).toBeVisible();

    // Add the view.
    await editor.locator('select.add-page').selectOption('/v/about');
    await expect.poll(() => groupsOf(request, edited)).toEqual([
      ['A', [[`/s/${alpha}`, null], ['/v/about', null]]],
    ]);

    // Move it up.
    await editor.locator('li.page-edit[data-address="/v/about"] button.page-up').click();
    await expect.poll(() => groupsOf(request, edited)).toEqual([
      ['A', [['/v/about', null], [`/s/${alpha}`, null]]],
    ]);

    // Rename it.
    const label = editor.locator('li.page-edit[data-address="/v/about"] input.page-label');
    await label.fill('About us');
    await label.press('Tab');
    await expect.poll(() => groupsOf(request, edited)).toEqual([
      ['A', [['/v/about', 'About us'], [`/s/${alpha}`, null]]],
    ]);

    // A second group, and the first renamed.
    await page.locator('.selector-editor input.new-group').fill('More');
    await page.locator('.selector-editor form.add-group button').click();
    const first = page.locator('.selector-editor fieldset.group-edit[data-group="0"] input.group-label');
    await first.fill('Places');
    await first.press('Tab');
    await expect.poll(async () => (await groupsOf(request, edited)).map(([name]) => name)).toEqual([
      'Places',
      'More',
    ]);
    await shot(page, 1, 'selector-editing');

    // Done: the selector as anyone sees it.
    await page.locator('header.bar button.mode:not(.sign-out)').click();
    const rail = page.locator('nav.page-selector[data-target="main"]');
    await expect(rail.locator('a.selector-page')).toHaveText(['About us', 'Alpha']);
    await expect(rail.locator('.selector-group-label')).toHaveText(['Places', 'More']);
  });

  test('a surface is edited as JSON, downloaded, and loaded from a file; what is not a surface is refused', async ({
    page,
    request,
  }) => {
    await openSurface(page, edited);
    await page.locator('header.bar button.mode:not(.sign-out)').click();

    const json = page.locator('.catalog .surface-json');
    const text = json.locator('textarea.json-text');
    await json.locator('button.json-show').click();
    const shown = JSON.parse(await text.inputValue());
    expect(shown.items.map((item) => item.id)).toEqual(['nav', 'main']);
    expect(shown.owner).toBeUndefined();

    // Download carries the same text.
    await expect(json.locator('a.json-download')).toHaveAttribute('href', /^data:application\/json/);

    // Edit and apply.
    shown.title = 'From JSON';
    await text.fill(JSON.stringify(shown, null, 2));
    await json.locator('button.json-apply').click();
    await expect.poll(async () => (await stored(request, edited)).title).toBe('From JSON');
    await expect(page.locator('header.bar .tagline')).toHaveText('From JSON');

    // Refused, and nothing changes.
    await text.fill('not json');
    await json.locator('button.json-apply').click();
    await expect(json.locator('.json-refused')).toContainText('not JSON');
    await text.fill(JSON.stringify({ items: [{ type: 'widget', id: 'w' }] }));
    await json.locator('button.json-apply').click();
    await expect(json.locator('.json-refused')).toContainText('`widget`');
    expect((await stored(request, edited)).items.map((item) => item.id)).toEqual(['nav', 'main']);

    // Load a file into the box, then apply it.
    const loaded = { ...shown, title: 'From a file' };
    await json.locator('input.json-file').setInputFiles({
      name: 'surface.json',
      mimeType: 'application/json',
      buffer: Buffer.from(JSON.stringify(loaded)),
    });
    await expect.poll(async () => JSON.parse(await text.inputValue()).title).toBe('From a file');
    await expect(json.locator('.json-refused')).toHaveCount(0);
    await json.locator('button.json-apply').click();
    await expect.poll(async () => (await stored(request, edited)).title).toBe('From a file');
    await shot(page, 2, 'surface-json');
  });
});
