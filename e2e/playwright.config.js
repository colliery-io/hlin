// How the browser tests run.
//
// Against a demo somebody else started, deliberately. Playwright can start a
// server itself, but the thing worth testing is the demo a person is told to
// open in the README, not a second arrangement that only the tests ever see.

const { defineConfig, devices } = require('@playwright/test');

const SHELL = process.env.HLIN_URL || 'http://127.0.0.1:8080';

// `chromium` is Playwright's headless shell; `chromium-full` is the whole
// browser in its new headless mode, which is what Chrome ships and the only
// one here that runs a sandboxed frame in a process of its own.
const PROJECTS = {
  chromium: { ...devices['Desktop Chrome'] },
  'chromium-full': { ...devices['Desktop Chrome'], channel: 'chromium' },
  firefox: { ...devices['Desktop Firefox'] },
  webkit: { ...devices['Desktop Safari'] },
};
const BROWSERS = (process.env.HLIN_BROWSERS || 'chromium')
  .split(',')
  .map((name) => name.trim())
  .filter((name) => name in PROJECTS);

module.exports = defineConfig({
  testDir: './tests',
  outputDir: './results',

  // Screenshots are the point, and two tests writing to the same names at the
  // same time would interleave them. One worker, in order.
  workers: 1,
  fullyParallel: false,

  // A panel's first frame is behind a fetch to a platform, and a wasm bundle
  // has to compile before anything is on screen at all. Generous everywhere.
  timeout: 90_000,
  expect: { timeout: 30_000 },

  // A flake here would be a real signal about the stream, so a retry would
  // hide the thing most worth knowing.
  retries: 0,

  reporter: [['list'], ['html', { outputFolder: './report', open: 'never' }]],

  use: {
    baseURL: SHELL,
    viewport: { width: 1440, height: 900 },
    // A failure should leave enough behind to see what happened without
    // running it again.
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'off',
  },

  // Chromium unless asked otherwise:
  // `HLIN_BROWSERS=chromium,chromium-full,firefox,webkit` runs the same tests
  // in each, for the containment matrix above all (HLIN-S-0007,
  // *Containment*), whose answers are each browser's own.
  projects: BROWSERS.map((name) => ({ name, use: PROJECTS[name] })),
});
