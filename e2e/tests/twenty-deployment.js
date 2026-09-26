// Which twenty the suites are run against, and what differs between them.
//
// Two deployments of the same twenty widgets:
//
// - `process` (`angreal demo up --with twenty`): each widget a process on
//   this machine, its own UI at 127.0.0.1:8201–8220, stopped by SIGKILL and
//   started again by `angreal demo restart`.
// - `compose` (`angreal demo up --with twenty-compose`, HLIN-I-0013): each
//   widget a container at `<name>.comp.test:8080` on a compose network,
//   published nowhere; stopped and started by `docker compose stop/start`,
//   and its own UI reached through e2e/network-proxy.js, which `angreal e2e
//   twenty --against compose` runs on that network and passes here as
//   HLIN_NETWORK_PROXY.
//
// The shell's address, and signing in (the containerised shell signs people
// in through Dex), are playwright.config.js's and global-setup.js's business;
// everything else is here, so each spec says what it claims once, for both.

const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..');

/** `process` or `compose`: set by `angreal e2e twenty --against`. */
const DEPLOYMENT = process.env.HLIN_TWENTY === 'compose' ? 'compose' : 'process';
const COMPOSE = DEPLOYMENT === 'compose';

/** The process demo's widget ports, as `WIDGETS` in .angreal/task_demo.py has them. */
const PORTS = {
  clock: 8201,
  counter: 8202,
  poll: 8203,
  notes: 8204,
  dice: 8205,
  stopwatch: 8206,
  quote: 8207,
  sparkline: 8208,
  kanban: 8209,
  status: 8210,
  weather: 8211,
  pomodoro: 8212,
  shoutbox: 8213,
  reactions: 8214,
  bookmarks: 8215,
  oncall: 8216,
  picker: 8217,
  deploys: 8218,
  converter: 8219,
  meetings: 8220,
};

/** Every widget, by name. */
const WIDGETS = Object.keys(PORTS);

/**
 * Where a widget's own UI is: the root of its own origin. On the compose
 * network that is its own name, as `clock.comp.net` would be deployed.
 */
function ownOrigin(name) {
  return COMPOSE ? `http://${name}.comp.test:8080` : `http://127.0.0.1:${PORTS[name]}`;
}

/** What a context or request context needs to reach an own UI at all. */
function ownOptions() {
  if (!COMPOSE) return {};
  const proxy = process.env.HLIN_NETWORK_PROXY;
  if (!proxy) {
    throw new Error('HLIN_NETWORK_PROXY is unset: run this through `angreal e2e twenty --against compose`');
  }
  return { proxy: { server: proxy } };
}

/** A browser context for widgets' own pages. */
function ownContext(browser, options = {}) {
  return browser.newContext({ ...options, ...ownOptions() });
}

/** A request context for widgets' own origins, from where their pages are. */
function ownRequest(playwright) {
  return playwright.request.newContext(ownOptions());
}

// -- A platform going down and coming back ------------------------------------

const COMPOSE_FILE = path.join(ROOT, 'deploy', 'twenty', 'compose.yml');
const COMPOSE_PROJECT = 'hlin-twenty';
const REGISTRY = path.join(ROOT, 'demo', 'state', 'processes.json');

function compose(...args) {
  return execFileSync('docker', ['compose', '-f', COMPOSE_FILE, '-p', COMPOSE_PROJECT, ...args], {
    cwd: ROOT,
    encoding: 'utf8',
  });
}

/**
 * Take a widget's platform away. Returns how, for the record.
 *
 * A process is sent SIGKILL. A container is stopped as an operator would
 * stop it, `docker compose stop`: SIGTERM, and the widget's server shuts
 * down, closing what it held open.
 */
function stopPlatform(name) {
  if (COMPOSE) {
    compose('stop', name);
    return `docker compose stop ${name}`;
  }
  const { pid } = JSON.parse(fs.readFileSync(REGISTRY, 'utf8'))[name];
  process.kill(pid, 'SIGKILL');
  return `SIGKILL to ${name} (pid ${pid})`;
}

/**
 * Bring it back, and return once it answers: `angreal demo restart`, or
 * `docker compose start` and the healthcheck's request answered.
 *
 * Asked directly, inside the container, rather than waiting for Docker to
 * call it healthy: the healthcheck runs every five seconds, and the time
 * until it next did would be counted as the platform's.
 */
async function startPlatform(name) {
  if (!COMPOSE) {
    execFileSync('angreal', ['demo', 'restart', name], { cwd: ROOT, stdio: 'inherit' });
    return;
  }
  compose('start', name);
  const deadline = Date.now() + 60_000;
  for (;;) {
    try {
      compose('exec', '-T', name, 'curl', '-fsS', '-o', '/dev/null', 'http://127.0.0.1:8080/hlin/.well-known/hlin.json');
      return;
    } catch (error) {
      if (Date.now() > deadline) throw new Error(`${name} is not answering a minute after starting`);
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
}

module.exports = {
  DEPLOYMENT,
  COMPOSE,
  WIDGETS,
  ownOrigin,
  ownContext,
  ownRequest,
  stopPlatform,
  startPlatform,
};
