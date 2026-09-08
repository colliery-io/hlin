// Things every test needs, in one place.

const { expect } = require('@playwright/test');
const path = require('path');
const fs = require('fs');

const SHOTS = path.join(__dirname, '..', 'screenshots');

/**
 * Capture the page and name it after the step it shows.
 *
 * Numbered so the directory reads in order, and the number is passed in rather
 * than counted, because Playwright runs each spec file in its own process and a
 * counter in this module would restart per file and collide.
 */
async function shot(page, order, name) {
  fs.mkdirSync(SHOTS, { recursive: true });
  const file = path.join(SHOTS, `${String(order).padStart(2, '0')}-${name}.png`);
  await page.screenshot({ path: file, fullPage: false });
  return file;
}

/**
 * A layout of this test's own, through the shell's API.
 *
 * Tests must not work in whatever surface happens to be there: a run would
 * depend on the state a previous run left, and would leave its own mess for a
 * person opening the demo afterwards.
 */
async function freshLayout(request, title) {
  const answer = await request.post('/api/layouts', { data: { title } });
  if (!answer.ok()) {
    throw new Error(`the shell would not create a layout: ${answer.status()}`);
  }
  const layout = await answer.json();
  return layout.id;
}

/** Remove a layout the test made, whatever happened to it. */
async function discardLayout(request, id) {
  if (id) {
    await request.delete(`/api/layouts/${id}`);
  }
}

/** Open a surface and wait for the frontend to have drawn its chrome. */
async function openSurface(page, id) {
  await page.goto(`/s/${id}`);
  // The bar is the first thing the app renders, so its presence is the signal
  // that the WebAssembly bundle loaded and mounted rather than erroring.
  await page.locator('header.bar').waitFor({ state: 'visible' });
}

/**
 * Wait until the surface owes the shell nothing.
 *
 * Composition is optimistic: the draft changes when the button is pressed and
 * the write follows, so what is on screen is a promise for as long as the bar
 * says `saving…`. Anything that navigates — a reload, or simply the end of a
 * test, which closes the page — cancels a write still in flight, and the change
 * is gone with nothing to say so.
 *
 * Which makes this the last line of any test whose gesture is supposed to
 * outlive the page, not merely the ones that reload. A test that ends dirty
 * fails the *next* test, under load, in a way that looks like anything but this.
 */
async function settled(page) {
  await expect(page.locator('.saving')).toHaveCount(0);
}

/** The panel with this instance id. */
function panel(page, instance) {
  return page.locator(`section.panel[data-instance="${instance}"]`);
}

/** Where a panel sits, as the markup reports it. */
async function placementOf(locator) {
  return {
    x: Number(await locator.getAttribute('data-x')),
    y: Number(await locator.getAttribute('data-y')),
    w: Number(await locator.getAttribute('data-w')),
    h: Number(await locator.getAttribute('data-h')),
  };
}

/**
 * Drag from one point to another with real pointer events.
 *
 * Playwright's `dragTo` uses mouse events, and the grid listens for pointer
 * events with capture, so the gesture has to be made the way a pointer makes
 * it: down, several moves, up. Several rather than one, because a single jump
 * would not exercise the tracking a person's drag actually produces.
 */
async function dragBy(page, handle, dx, dy) {
  // Wait for the handle before measuring it. `boundingBox` does not retry the
  // way an assertion does, so without this a panel that has not arrived yet
  // fails as "the drag handle is not on screen" — which reads like a layout
  // bug and is really the test outrunning the stream. Panels arrive over SSE
  // after the bundle mounts, so how long that takes is a property of how busy
  // the shell is, not of the code under test.
  await handle.waitFor({ state: 'visible' });

  const box = await handle.boundingBox();
  if (!box) {
    throw new Error('the drag handle is not on screen');
  }

  const fromX = box.x + box.width / 2;
  const fromY = box.y + box.height / 2;

  await page.mouse.move(fromX, fromY);
  await page.mouse.down();
  for (let step = 1; step <= 8; step += 1) {
    await page.mouse.move(fromX + (dx * step) / 8, fromY + (dy * step) / 8);
    await page.waitForTimeout(20);
  }
  await page.mouse.up();
}

/**
 * Whether a panel drew anything, without knowing which pack drew it.
 *
 * The obvious assertion is a class name, and it is the wrong one: it ties the
 * suite to one design system and fails when the pack is swapped, which is
 * exactly the thing Hlin is built to allow. What is actually being claimed is
 * that the panel is not an empty box, so this asks whether there is content
 * below the heading, whatever a pack chose to put there.
 */
async function drewSomething(panel) {
  return panel.evaluate((node) => {
    const heading = node.querySelector('.panel-head');
    const body = Array.from(node.children).filter((child) => child !== heading);
    if (body.length === 0) {
      return false;
    }
    return body.some(
      (child) =>
        child.querySelector('svg, table, img, canvas, pre') !== null ||
        (child.textContent || '').trim().length > 0,
    );
  });
}

module.exports = {
  SHOTS,
  shot,
  freshLayout,
  discardLayout,
  openSurface,
  panel,
  placementOf,
  dragBy,
  drewSomething,
  settled,
};
