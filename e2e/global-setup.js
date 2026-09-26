// Signing in once, before a suite that runs against a shell which signs
// people in but whose tests are written for a demo that does not.
//
// The twenty suites were written against `demo up --with twenty`, where
// everybody is the development user. Against the containerised twenty
// (`demo up --with twenty-compose`) the shell signs people in through Dex, so
// `angreal e2e twenty --against compose` sets HLIN_SIGN_IN to somebody's
// email, and this signs them in through Dex's own form, as a person would,
// and keeps the session. playwright.config.js hands it to every context the
// tests make, and to their `request`.
//
// Only the cookies are carried, not the browser's cache: a context made with
// them is still cold, which is what the measurement needs.

const fs = require('fs');
const path = require('path');
const { chromium } = require('@playwright/test');
const { signIn } = require('./tests/helpers');

const SIGNED_IN = path.join(__dirname, '.auth', 'signed-in.json');

module.exports = async (config) => {
  const email = process.env.HLIN_SIGN_IN;
  if (!email) return;

  const { baseURL } = config.projects[0].use;
  const browser = await chromium.launch();
  try {
    // Dex is on https from a CA of the demo's own.
    const context = await browser.newContext({ baseURL, ignoreHTTPSErrors: true });
    await signIn(await context.newPage(), email);
    fs.mkdirSync(path.dirname(SIGNED_IN), { recursive: true });
    await context.storageState({ path: SIGNED_IN });
    console.log(`signed in through Dex as ${email}`);
  } finally {
    await browser.close();
  }
};

module.exports.SIGNED_IN = SIGNED_IN;
