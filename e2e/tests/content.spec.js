// Shell components on a surface (HLIN-T-0113, decision HLIN-A-0015).
//
// A surface holds a bar and content components beside its panels, each in its
// own cells. A content component opens a page there: another surface, a
// platform's module page, or a view the front end wrote in code. What it
// cannot open is a placeholder that says why, and the item stays in the
// layout.
//
// Pack-agnostic: every front end in this repository registers the view
// `about`, and every assertion is on the shell's own markup.

const { test, expect } = require('@playwright/test');
const { shot, freshLayout, discardLayout, openSurface, asPanels } = require('./helpers');

test.describe.configure({ mode: 'serial' });

const item = (page, id) => page.locator(`section.item[data-item="${id}"]`);

/** Write a layout's items, keeping the rest of it as the shell has it. */
async function writeItems(request, id, items) {
  const current = await (await request.get(`/api/layouts/${id}`)).json();
  current.items = items;
  const written = await request.put(`/api/layouts/${id}`, { data: current });
  expect(written.ok(), await written.text()).toBeTruthy();
}

test.describe('shell components on a surface', () => {
  const made = [];

  async function layout(request, title) {
    const id = await freshLayout(request, title);
    made.push(id);
    return id;
  }

  test.afterAll(async ({ request }) => {
    for (const id of made) {
      await discardLayout(request, id);
    }
  });

  test('content components open a surface, a platform page and a view; what they cannot open says why', async ({
    page,
    request,
  }) => {
    const inner = await layout(request, 'Inner');
    await writeItems(
      request,
      inner,
      asPanels([
        { platform_id: 'orebank', panel_key: 'queue-depth', position: { x: 0, y: 0, w: 12, h: 3 } },
      ]),
    );

    const outer = await layout(request, 'Outer');
    await writeItems(request, outer, [
      { type: 'bar', id: 'bar', position: { x: 0, y: 0, w: 12, h: 1 } },
      { type: 'content', id: 'nested', position: { x: 0, y: 1, w: 6, h: 6 }, initial: { surface: inner } },
      {
        type: 'content',
        id: 'platform',
        position: { x: 6, y: 1, w: 6, h: 6 },
        initial: { page: { platform: 'orebank', path: 'module-page' } },
      },
      { type: 'content', id: 'view', position: { x: 0, y: 7, w: 4, h: 3 }, initial: { view: 'about' } },
      { type: 'content', id: 'gone', position: { x: 4, y: 7, w: 4, h: 3 }, initial: { view: 'no-such-view' } },
      { type: 'content', id: 'itself', position: { x: 8, y: 7, w: 4, h: 3 }, initial: { surface: outer } },
      { type: 'content', id: 'empty', position: { x: 0, y: 10, w: 4, h: 2 } },
    ]);

    await openSurface(page, outer);

    // A bar placed on a surface: the surface's own name, and no time picker
    // or Edit, which belong to the shell's bar alone.
    const bar = item(page, 'bar');
    await expect(bar.locator('header.bar')).toBeVisible();
    await expect(bar.locator('.picker')).toHaveCount(0);
    await expect(bar.locator('button.mode:not(.sign-out)')).toHaveCount(0);
    await expect(bar.locator('.tagline')).toHaveText('Outer');

    // Another surface, with its own panel drawn from its own stream.
    const nested = item(page, 'nested');
    await expect(nested.locator('section.panel[data-panel="orebank/queue-depth"]')).toHaveAttribute(
      'data-state',
      'ready',
      { timeout: 60_000 },
    );

    // A platform's module page, in the same kind of frame as a full-width page.
    const platform = item(page, 'platform');
    await expect(platform.locator('section.page[data-page="orebank/module-page"] iframe')).toBeVisible({
      timeout: 30_000,
    });

    // The front end's own view.
    await expect(item(page, 'view').locator('.demo-about')).toBeVisible();

    // What cannot be opened says so.
    await expect(item(page, 'gone').locator('.content-missing')).toHaveAttribute('data-cause', 'no-such-view');
    await expect(item(page, 'itself').locator('.content-missing')).toHaveAttribute('data-cause', 'cycle');
    await expect(item(page, 'empty').locator('.content-missing')).toHaveAttribute('data-cause', 'nothing-chosen');

    // And the layout still holds every item.
    const stored = await (await request.get(`/api/layouts/${outer}`)).json();
    expect(stored.items.map((entry) => entry.id)).toEqual([
      'bar',
      'nested',
      'platform',
      'view',
      'gone',
      'itself',
      'empty',
    ]);

    await shot(page, 1, 'shell-components');
  });

  test('surfaces open two levels deep and no deeper', async ({ page, request }) => {
    const third = await layout(request, 'Third');
    const second = await layout(request, 'Second');
    const first = await layout(request, 'First');
    const outer = await layout(request, 'Outermost');

    const opening = (id) => [
      { type: 'content', id: 'inside', position: { x: 0, y: 0, w: 12, h: 8 }, initial: { surface: id } },
    ];
    await writeItems(request, outer, opening(first));
    await writeItems(request, first, opening(second));
    await writeItems(request, second, opening(third));

    await openSurface(page, outer);

    // Outermost (0) opens First (1), which opens Second (2): the deepest. Each
    // holds a content component with the same id, so there are three, and
    // the one on Second shows a placeholder instead of opening Third.
    const level = 'section.item[data-item="inside"]';
    await expect(page.locator(level)).toHaveCount(3, { timeout: 30_000 });
    const deepest = page.locator(`${level} ${level} ${level}`);
    await expect(deepest.locator('.content-missing')).toHaveAttribute('data-cause', 'too-deep');
    await expect(page.locator('.content-missing')).toHaveCount(1);
  });

  test('a surface that cannot be fetched says so in its cells', async ({ page, request }) => {
    const outer = await layout(request, 'Pointing nowhere');
    await writeItems(request, outer, [
      {
        type: 'content',
        id: 'missing',
        position: { x: 0, y: 0, w: 6, h: 3 },
        initial: { surface: '00000000-0000-0000-0000-000000000000' },
      },
    ]);

    await openSurface(page, outer);
    await expect(item(page, 'missing').locator('.content-missing')).toHaveAttribute('data-cause', 'unreachable', {
      timeout: 30_000,
    });
  });
});
