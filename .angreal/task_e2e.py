"""Browser tests.

The only tests in this repository that put a real browser in front of the
frontend. Everything else asserts about states and status codes; these assert
about what is on a screen, and leave screenshots behind so a person can check
in a second what would otherwise take a paragraph.

They run against a demo started with `angreal demo up`, deliberately: the thing
worth testing is what the README tells someone to open, not a second
arrangement that only the tests ever see.
"""

import os
import subprocess

import angreal

cwd = os.path.join(angreal.get_root(), "..")
E2E = os.path.join(cwd, "e2e")
SHOTS = os.path.join(E2E, "screenshots")

SHELL = "http://127.0.0.1:8080"

e2e = angreal.command_group(name="e2e", about="browser tests")


def _npm_available():
    if subprocess.run(["which", "npm"], capture_output=True).returncode != 0:
        print("npm is not installed, and it is what runs the browser tests.", flush=True)
        return False
    return True


def _installed():
    return os.path.isdir(os.path.join(E2E, "node_modules"))


def _demo_running():
    import urllib.request

    try:
        with urllib.request.urlopen(f"{SHELL}/api/health", timeout=3) as answer:
            return answer.status == 200
    except Exception:
        return False


def _signs_people_in():
    """Whether the running shell sends an unsigned browser to a provider.

    The collaborative demo does; the standard one makes everybody the
    development user. The suites expect one or the other, and run against the
    wrong one they fail on every request with a 401 that says nothing about why.
    """
    import json
    import urllib.error
    import urllib.request

    try:
        with urllib.request.urlopen(f"{SHELL}/api/config", timeout=3):
            return False
    except urllib.error.HTTPError as error:
        if error.code != 401:
            return False
        try:
            return bool(json.loads(error.read().decode("utf-8")).get("login"))
        except ValueError:
            return False
    except Exception:
        return False


def _playwright(argv, env=None):
    """Run Playwright and say where to look afterwards.

    Not `_run`: angreal loads every task file into one namespace, and
    task_tests.py's `_run` replaced a helper of that name here, running
    Playwright from the repository root where it finds no configuration.

    `env`, added to this process's environment: where the shell is, and who
    to sign in as, for a suite run against the containerised twenty.
    """
    result = subprocess.run(argv, cwd=E2E, env={**os.environ, **(env or {})})

    if os.path.isdir(SHOTS):
        shots = sorted(name for name in os.listdir(SHOTS) if name.endswith(".png"))
        if shots:
            print(f"\n{len(shots)} screenshots in {SHOTS}")
            for name in shots:
                print(f"  {name}")

    # Flushed: angreal exits on a failing task without flushing what Python
    # buffered, so an unflushed explanation of a failure is never seen.
    if result.returncode:
        print(
            "\nA browser test failed. `npx playwright show-report` in e2e/ has the "
            "trace, with a screenshot and the DOM at the moment it went wrong.",
            flush=True,
        )

    return result.returncode


@e2e()
@angreal.command(
    name="install",
    about="fetch Playwright and the browser it drives",
    tool=angreal.ToolDescription(
        """
        Install the e2e project's dependencies and download the Chromium build
        Playwright drives.

        ## When to use
        - Once, before `angreal e2e test`
        - After the pinned Playwright version changes

        Downloads a browser, so the first run is slow and needs a network.
        """,
        risk_level="safe",
    ),
)
def e2e_install():
    if not _npm_available():
        return 1

    print("installing the e2e project", flush=True)
    if subprocess.run(["npm", "install"], cwd=E2E).returncode:
        return 1

    print("downloading the browser", flush=True)
    return subprocess.run(["npx", "playwright", "install", "chromium"], cwd=E2E).returncode


@e2e()
@angreal.command(
    name="test",
    about="run the browser tests against a running demo",
    tool=angreal.ToolDescription(
        """
        Drive a real browser through composing a surface and watching it
        degrade, asserting what is on screen at each step and writing a
        screenshot per step to e2e/screenshots.

        ## When to use
        - After `angreal demo up`
        - After changing anything in crates/hlin-ui or the design pack

        Creates and deletes layouts of its own through the API; it does not
        touch a surface anybody else composed.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="headed",
    long="headed",
    takes_value=False,
    is_flag=True,
    help="show the browser rather than running it headless",
)
@angreal.argument(
    name="filter",
    long="filter",
    short="k",
    takes_value=True,
    help="only run tests whose title matches this",
)
def e2e_test(headed=False, filter=None):
    if not _npm_available():
        return 1

    if not _installed():
        print("Playwright is not installed here. Run `angreal e2e install` first.", flush=True)
        return 1

    if not _demo_running():
        print(f"Nothing is answering at {SHELL}. Start it with `angreal demo up`.", flush=True)
        return 1

    if _signs_people_in():
        print(
            f"The shell at {SHELL} signs people in (`demo up --with collab`), and this\n"
            "suite composes as the development user. Run `angreal e2e signin` against\n"
            "it, or `angreal demo up` for this suite.",
            flush=True,
        )
        return 1

    if _is_twenty():
        print(
            f"The shell at {SHELL} is the twenty-widget demo (`demo up --with twenty`),\n"
            "and this suite composes from the sample platforms, which it does not run.\n"
            "Run `angreal e2e twenty` against it, or `angreal demo up` for this suite.",
            flush=True,
        )
        return 1

    argv = ["npx", "playwright", "test"]
    if headed:
        argv.append("--headed")
    if filter:
        argv += ["-g", filter]

    return _playwright(argv)


@e2e()
@angreal.command(
    name="signin",
    about="sign in through Dex in a browser, against the collaborative demo",
    tool=angreal.ToolDescription(
        """
        Drive a real browser through Dex's login form as each of the demo's
        three people, asserting the name the shell shows, the email its
        principal carries, and that signing out ends the session. Then open
        the published surface as Alice (both panels drawn with data) and as
        Carol (the checklist refuses her; the feed does not). Then change
        things through the platforms' own modules: Alice adds, ticks, edits
        and deletes an item and posts; Bob is refused an edit of her post and
        Carol a post of her own, each in the feed's words.

        ## When to use
        - After `angreal demo up --with collab`
        - After changing the oidc authenticator, sessions, or demo/dex.yaml

        Creates sessions and ends them, a first empty surface for Bob and
        Carol, and an item and a post named for the run in the platforms'
        memory, which `demo down` forgets.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="headed",
    long="headed",
    takes_value=False,
    is_flag=True,
    help="show the browser rather than running it headless",
)
def e2e_signin(headed=False):
    if not _collab_ready():
        return 1

    argv = ["npx", "playwright", "test", "signin.spec.js", "collab.spec.js", "collab-modules.spec.js"]
    if headed:
        argv.append("--headed")

    return _playwright(argv)


def _collab_ready():
    """Whether the collaborative demo is up for a suite that signs people in."""
    if not _npm_available():
        return False

    if not _installed():
        print("Playwright is not installed here. Run `angreal e2e install` first.", flush=True)
        return False

    if not _demo_running():
        print(f"Nothing is answering at {SHELL}. Start it with `angreal demo up --with collab`.", flush=True)
        return False

    # Checked here rather than left to the spec, which skips itself against a
    # shell that does not sign people in: a run that skipped everything would
    # otherwise report success.
    if not _signs_people_in():
        print(
            f"The shell at {SHELL} does not sign people in. Start the collaborative\n"
            "demo with `angreal demo up --with collab`.",
            flush=True,
        )
        return False

    return True


@e2e()
@angreal.command(
    name="walkthrough",
    about="two people in two browsers, against the collaborative demo",
    tool=angreal.ToolDescription(
        """
        Walk the collaborative demo's story with Alice and Bob signed in
        through Dex in two browsers at once: Alice adds an item and Bob's open
        page shows it without a reload; Bob crosses it off and Alice's shows
        it crossed. Bob is refused an edit of Alice's post, and Carol reads
        the feed but is refused a post and the team list, each in the
        platform's words. Prints how long each change took to reach the other
        browser, and writes a screenshot per step.

        ## When to use
        - After `angreal demo up --with collab`
        - After changing the change relay, the surface stream, or either
          collaborative platform or its module

        Creates sessions, and an item and a post named for the run in the
        platforms' memory, which `demo down` forgets.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="headed",
    long="headed",
    takes_value=False,
    is_flag=True,
    help="show the browsers rather than running them headless",
)
def e2e_walkthrough(headed=False):
    if not _collab_ready():
        return 1

    argv = ["npx", "playwright", "test", "walkthrough.spec.js"]
    if headed:
        argv.append("--headed")

    return _playwright(argv)


def _offered_platforms():
    """The ids of the platforms the running shell offers panels from."""
    import json
    import urllib.request

    try:
        with urllib.request.urlopen(f"{SHELL}/api/panels", timeout=3) as answer:
            return {platform["id"] for platform in json.loads(answer.read().decode("utf-8"))}
    except Exception:
        return set()


def _is_twenty():
    """Whether the running shell is the twenty-widget demo.

    Told by what it offers rather than by a layout's title, because layouts
    outlive the demo that made them in the database, and the platforms do not.
    """
    return {"counter", "poll"} <= _offered_platforms()


@e2e()
@angreal.command(
    name="twenty",
    about="every widget ready, and shared ones in step, against the twenty-widget demo",
    tool=angreal.ToolDescription(
        """
        Open the published "Twenty" surface in a browser and assert every
        widget on it reaches `ready`, scrolling each into view; then open it
        in a second browser context and show that bumping the counter and
        voting in the poll in one reaches the other without a reload,
        printing how long each took. Then opens the converted widgets' own
        UIs at their own ports' roots, and shows a counter bump there reach
        its module on "Twenty" and back. Writes a screenshot per step.

        With `--against compose`, the same suite against the twenty in
        containers (`angreal demo up --with twenty-compose`): signed in
        through Dex as Alice, the shell at 127.0.0.1:8090, and the widgets'
        own UIs opened by their names on the compose network
        (`clock.comp.test:8080`) through a proxy container this runs on that
        network for the length of the suite (e2e/network-proxy.js), since
        nothing publishes a widget.

        ## When to use
        - After `angreal demo up --with twenty`
        - After `angreal demo up --with twenty-compose`, with `--against
          compose`
        - After adding or converting a widget, or changing
          hlin-widget-support, hlin-widget-ui or hlin-widget-module

        Bumps the counter and moves the signed-in person's vote, in the
        widgets' memory, which `demo down` forgets.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="headed",
    long="headed",
    takes_value=False,
    is_flag=True,
    help="show the browsers rather than running them headless",
)
@angreal.argument(
    name="against",
    long="against",
    takes_value=True,
    help="which twenty: `process` (demo up --with twenty, the default) or `compose` "
    "(demo up --with twenty-compose)",
)
def e2e_twenty(headed=False, against=None):
    argv = ["npx", "playwright", "test", "twenty.spec.js"]
    if headed:
        argv.append("--headed")

    against = against or "process"
    if against not in AGAINST:
        print(f"--against is `process` or `compose`, not `{against}`.", flush=True)
        return 1
    if against == "process":
        if not _twenty_ready():
            return 1
        return _playwright(argv)

    if not _containers_ready():
        return 1
    proxy = _network_proxy_up()
    if proxy is None:
        return 1
    try:
        return _playwright(argv, env={**_CONTAINERS_ENV, "HLIN_NETWORK_PROXY": proxy})
    finally:
        _network_proxy_down()


def _twenty_ready():
    """Whether the twenty-widget demo is up for a suite that expects it."""
    if not _npm_available():
        return False

    if not _installed():
        print("Playwright is not installed here. Run `angreal e2e install` first.", flush=True)
        return False

    if not _demo_running():
        print(f"Nothing is answering at {SHELL}. Start it with `angreal demo up --with twenty`.", flush=True)
        if _containers_running():
            print("The twenty in containers is running: add `--against compose`.", flush=True)
        return False

    # Checked here rather than left to the spec, which skips itself against
    # another demo: a run that skipped everything would report success.
    if _signs_people_in() or not _is_twenty():
        print(
            f"The shell at {SHELL} is not the twenty-widget demo. Start it with\n"
            "`angreal demo up --with twenty`.",
            flush=True,
        )
        return False

    return True


@e2e()
@angreal.command(
    name="twenty-measure",
    about="time, weigh and break the twenty-widget demo, three times over",
    tool=angreal.ToolDescription(
        """
        Measure the published "Twenty" surface in a browser, three times, and
        print the medians: cold and warm time from navigation to every widget
        in view `ready` and to its first content, bytes over the wire by kind
        (module assets, platform requests, streams, the shell), JS heap and
        the browser's resident memory, and the most frames mounted while
        scrolling to the bottom and back. Asserts the budget of twelve holds
        and nothing in view is unmounted; that a widget scrolled away gets
        its state back; times a counter bump from one browser to another; and
        kills one widget's process (dice) with SIGKILL, asserts only its panel
        degrades, and starts it again with `angreal demo restart dice`.
        Writes everything to e2e/measurements/ as JSON, and screenshots of
        the drawn surface, top, middle and bottom.

        ## When to use
        - After `angreal demo up --with twenty --release`: debug modules are
          five times the size and measure the wrong thing
        - After `angreal demo up --with twenty-compose`, with `--against
          compose`: the same against the twenty in containers (release
          throughout), signed in through Dex as Alice, the dice container
          stopped with `docker compose stop dice` and started with `docker
          compose start dice`
        - When recording or rechecking HLIN-T-0084's or HLIN-T-0095's numbers

        Either way the widget goes down and comes back twice on one open page.

        Kills and restarts the dice widget, which forgets its rolls, and
        bumps the counter.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="runs",
    long="runs",
    takes_value=True,
    help="how many times to measure (default 3); the median is reported",
)
@angreal.argument(
    name="against",
    long="against",
    takes_value=True,
    help="which twenty: `process` (demo up --with twenty --release, the default) or "
    "`compose` (demo up --with twenty-compose)",
)
def e2e_twenty_measure(runs=None, against=None):
    against = against or "process"
    if against not in AGAINST:
        print(f"--against is `process` or `compose`, not `{against}`.", flush=True)
        return 1
    if against == "process" and not _twenty_ready():
        return 1
    if against == "compose" and not _containers_ready():
        return 1

    if runs:
        os.environ["HLIN_RUNS"] = str(int(runs))
    argv = ["npx", "playwright", "test", "twenty-measure.spec.js"]
    return _playwright(argv, env=_CONTAINERS_ENV if against == "compose" else None)


# -- The twenty in containers ----------------------------------------------

#: What `--against` may name: the twenty as processes (`demo up --with
#: twenty`) or in containers (`demo up --with twenty-compose`, HLIN-I-0013).
AGAINST = ("process", "compose")

#: The containerised twenty, as .angreal/task_demo.py brings it up. Named
#: apart from that file's own names for the same things: angreal loads every
#: task file into one namespace.
CONTAINERS_SHELL = "http://127.0.0.1:8090"
CONTAINERS_FILE = os.path.join(cwd, "deploy", "twenty", "compose.yml")
CONTAINERS_PROJECT = "hlin-twenty"
CONTAINERS_NETWORK = f"{CONTAINERS_PROJECT}_comp"

#: Who the suites are, signed in through Dex by e2e/global-setup.js: Alice,
#: who published "Twenty".
CONTAINERS_PERSON = "alice@example.com"

_CONTAINERS_ENV = {
    "HLIN_URL": CONTAINERS_SHELL,
    "HLIN_TWENTY": "compose",
    "HLIN_SIGN_IN": CONTAINERS_PERSON,
}

#: The proxy onto the compose network (e2e/network-proxy.js), in a container
#: on it, published on a loopback port Docker picks, for as long as a suite
#: runs.
NETWORK_PROXY = f"{CONTAINERS_PROJECT}-e2e-proxy"
NETWORK_PROXY_IMAGE = "node:22-alpine"


def _containers_running():
    """Whether the containerised twenty's shell container is running."""
    result = subprocess.run(
        ["docker", "compose", "-f", CONTAINERS_FILE, "-p", CONTAINERS_PROJECT,
         "ps", "--status", "running", "-q", "hlin"],
        cwd=cwd,
        capture_output=True,
        text=True,
    )
    return result.returncode == 0 and bool(result.stdout.strip())


def _containers_ready():
    """Whether the twenty in containers is up for a suite, or why not."""
    import json
    import urllib.error
    import urllib.request

    if not _npm_available():
        return False

    if not _installed():
        print("Playwright is not installed here. Run `angreal e2e install` first.", flush=True)
        return False

    if not _containers_running():
        print(
            f"The twenty in containers (compose project `{CONTAINERS_PROJECT}`) is not running.\n"
            "Start it with `angreal demo up --with twenty-compose`.",
            flush=True,
        )
        if _demo_running() and _is_twenty():
            print("The twenty as processes is running: leave out `--against compose`.", flush=True)
        return False

    # The published shell is the containerised one, and signs people in:
    # every test signs in through Dex first, and a shell that did not would
    # fail them all with something that says nothing about why.
    try:
        with urllib.request.urlopen(f"{CONTAINERS_SHELL}/api/config", timeout=3):
            signs_in = False
    except urllib.error.HTTPError as error:
        try:
            signs_in = error.code == 401 and bool(json.loads(error.read().decode("utf-8")).get("login"))
        except ValueError:
            signs_in = False
    except Exception:
        print(f"Nothing is answering at {CONTAINERS_SHELL}, where the containerised shell is published.", flush=True)
        return False
    if not signs_in:
        print(
            f"The shell at {CONTAINERS_SHELL} does not sign people in, so it is not the\n"
            "containerised twenty's. `angreal demo up --with twenty-compose` starts that.",
            flush=True,
        )
        return False
    return True


def _network_proxy_down():
    subprocess.run(["docker", "rm", "-f", NETWORK_PROXY], capture_output=True)


def _network_proxy_up():
    """Start the proxy onto the compose network, and return its address.

    Waits until a widget answers through it, so the first test does not
    race it.
    """
    import time
    import urllib.request

    _network_proxy_down()
    started = subprocess.run(
        [
            "docker", "run", "-d", "--rm", "--name", NETWORK_PROXY,
            "--network", CONTAINERS_NETWORK,
            "-p", "127.0.0.1::3128",
            "-v", f"{os.path.join(E2E, 'network-proxy.js')}:/proxy.js:ro",
            NETWORK_PROXY_IMAGE, "node", "/proxy.js",
        ],
        capture_output=True,
        text=True,
    )
    if started.returncode != 0:
        print(f"could not start the proxy onto {CONTAINERS_NETWORK}: {started.stderr.strip()}", flush=True)
        return None
    port = subprocess.run(["docker", "port", NETWORK_PROXY, "3128/tcp"], capture_output=True, text=True)
    address = port.stdout.strip().splitlines()[0] if port.returncode == 0 and port.stdout.strip() else None
    if address is None:
        print("the proxy onto the compose network published no port", flush=True)
        _network_proxy_down()
        return None
    proxy = f"http://{address}"

    opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": proxy}))
    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            with opener.open("http://clock.comp.test:8080/hlin/.well-known/hlin.json", timeout=2) as answer:
                if answer.status == 200:
                    print(f"widgets' own UIs through {proxy}, on {CONTAINERS_NETWORK}", flush=True)
                    return proxy
        except Exception:
            pass
        time.sleep(0.5)
    print(f"nothing answered through the proxy at {proxy}", flush=True)
    _network_proxy_down()
    return None


@e2e()
@angreal.command(name="shots", about="show where the screenshots are")
def e2e_shots():
    if not os.path.isdir(SHOTS):
        print("No screenshots yet. Run `angreal e2e test`.")
        return 0

    shots = sorted(name for name in os.listdir(SHOTS) if name.endswith(".png"))
    if not shots:
        print("No screenshots yet. Run `angreal e2e test`.")
        return 0

    print(f"{len(shots)} screenshots in {SHOTS}\n")
    for name in shots:
        size = os.path.getsize(os.path.join(SHOTS, name)) // 1024
        print(f"  {name:40} {size:>5} KB")
    return 0


@e2e()
@angreal.command(name="clean", about="remove screenshots, reports and traces")
def e2e_clean():
    import shutil

    removed = []
    for name in ["screenshots", "report", "results"]:
        target = os.path.join(E2E, name)
        if os.path.isdir(target):
            shutil.rmtree(target)
            removed.append(name)

    print(f"removed {', '.join(removed)}" if removed else "nothing to remove")
    return 0
