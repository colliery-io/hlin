"""The demo: one command up, one command that proves it.

Everything the initiative built, running together, reproducible by anyone with
the repository. `demo up` starts the database, builds the frontend and starts
three sample platforms and the shell. `demo walkthrough` then drives the whole
thing over HTTP and asserts what should happen, so a regression is named rather
than discovered.

Processes started here are ordinary background processes with a registry file,
not containers. Containers would hide exactly the thing the demo exists to show:
that the shell is one binary, the platforms are theirs, and nothing in between
is doing any work.
"""

import json
import os
import signal
import subprocess
import time
import urllib.error
import urllib.request

import angreal

cwd = os.path.join(angreal.get_root(), "..")
STATE = os.path.join(cwd, "demo", "state")
REGISTRY = os.path.join(STATE, "processes.json")
#: What the running demo was built as: its flavour, and whether its
#: WebAssembly is optimised. The load-time tests assert their limits only
#: against a release build (e2e/tests/performance.spec.js).
BUILD = os.path.join(STATE, "build.json")
LOGS = os.path.join(STATE, "logs")
#: The shell configurations in this repository. Each names a frontend, which is
#: how the demo chooses a design system: a front end is a binary that picked a
#: pack, and the shell serves whichever one it is pointed at.
CONFIGS = {
    "demo": "demo/hlin.toml",
    "aurora": "demo/hlin-aurora.toml",
    "gallery": "demo/hlin-gallery.toml",
    "live": "demo/hlin-live.toml",
    "collab": "demo/hlin-collab.toml",
    # Not run as it is: `up` writes demo/state/hlin-twenty.toml, this plus one
    # `[[platforms]]` per entry in `WIDGETS`.
    "twenty": "demo/hlin-twenty.toml",
}

#: Flavours where people sign in through Dex rather than being the development
#: user. They start the identity provider and, instead of the sample platforms
#: below, the collaborative demo's own (HLIN-I-0010).
SIGNED_IN = {"collab"}

#: Where Dex answers, as both the shell and the browser reach it.
DEX = "http://127.0.0.1:5556/dex"

#: The variable carrying the client secret. Dex reads it (`secretEnv` in
#: demo/dex.yaml) and so does the shell (`client_secret_env`), which is how the
#: two agree without the secret being written in either file.
DEX_SECRET = "HLIN_DEMO_OIDC_SECRET"

SHELL = "http://127.0.0.1:8080"

#: What `up` starts, in the order it starts them. The shell is last because the
#: platforms verify tokens against keys it publishes.
#:
#: Three, all the same binary under different names, so a surface can hold
#: modules from three platforms at once: what the load-time requirement is
#: measured against (HLIN-S-0007, NFR-1.1). Each serves the annotations
#: module, built with the SDK from `SAMPLE_MODULE`.
PLATFORMS = [
    {"name": "orebank", "port": 8081},
    {"name": "stampmill", "port": 8082},
    {"name": "smelter", "port": 8085},
]

#: The sample platforms' one module built with the SDK (HLIN-T-0071), a Trunk
#: project the platforms serve from its `dist`.
SAMPLE_MODULE = "crates/hlin-sample-platform/module"

#: The collaborative demo's platforms: two that accept writes and decide their
#: own rules from who is asking (HLIN-T-0072, HLIN-T-0073). Each is its own
#: binary, and both verify the shell's tokens like the sample platforms do.
#: Each ships its own UI module (HLIN-T-0074, HLIN-T-0075), a Trunk project in
#: `module`, which the platform serves from that project's `dist`.
COLLAB_PLATFORMS = [
    {
        "name": "checklist",
        "port": 8083,
        "binary": "hlin-sample-checklist",
        "module": "crates/hlin-sample-checklist/module",
    },
    {
        "name": "feed",
        "port": 8084,
        "binary": "hlin-sample-feed",
        "module": "crates/hlin-sample-feed/module",
    },
]

#: Who composes and publishes the collaborative demo's surface, and what it is
#: called. Alice owns the `team` list, so the surface is hers to publish.
COLLAB_AUTHOR = "alice@example.com"
COLLAB_PASSWORD = "password"
COLLAB_TITLE = "The team"

#: The surface itself: the team's checklist and the feed, side by side.
COLLAB_PANELS = [
    ("checklist", "items", {"list": ["team"]}, 0, 0, 6, 6),
    ("feed", "posts", {}, 6, 0, 6, 6),
]

#: The twenty widgets of HLIN-I-0012, in the order "Twenty" shows them.
#:
#: The one list of them: `up --with twenty` builds each one's module, starts
#: it on its port, lists it in the shell's configuration and puts it on the
#: surface, and `down` stops it. Adding a widget is one entry here, with the
#: port 8200 plus its number in the initiative's table (the steps are in
#: crates/widgets/hlin-widget-support/src/lib.rs, *Adding a widget*).
#:
#: Everything else is by convention from the name: the crate is
#: crates/widgets/{name}, its binary hlin-widget-{name}, its module the package
#: hlin-widget-{name}-module in its `module/`, and its one panel `{name}`.
WIDGETS = [
    {"name": "clock", "port": 8201},
    {"name": "counter", "port": 8202},
    {"name": "poll", "port": 8203},
    {"name": "notes", "port": 8204},
    {"name": "dice", "port": 8205},
    {"name": "stopwatch", "port": 8206},
    {"name": "quote", "port": 8207},
    {"name": "sparkline", "port": 8208},
    {"name": "kanban", "port": 8209},
    {"name": "status", "port": 8210},
    {"name": "weather", "port": 8211},
    {"name": "pomodoro", "port": 8212},
    {"name": "shoutbox", "port": 8213},
    {"name": "reactions", "port": 8214},
    {"name": "bookmarks", "port": 8215},
    {"name": "oncall", "port": 8216},
    {"name": "picker", "port": 8217},
    {"name": "deploys", "port": 8218},
    {"name": "converter", "port": 8219},
    {"name": "meetings", "port": 8220},
]

#: What the twenty-widget surface is called, and its grid: three widgets
#: across a twelve-column grid, each five 60px rows tall, so twenty make a
#: page about three times the height of a laptop screen, with eight or nine in
#: view at a time (HLIN-I-0012 decision 3).
TWENTY_TITLE = "Twenty"
TWENTY_ACROSS = 3
TWENTY_WIDTH = 4
TWENTY_HEIGHT = 5

#: Where `up --with twenty` writes the shell configuration it runs.
TWENTY_CONFIG = os.path.join(STATE, "hlin-twenty.toml")

demo = angreal.command_group(name="demo", about="the running demo")


# -- The process registry -------------------------------------------------


def _registry():
    if not os.path.isfile(REGISTRY):
        return {}
    try:
        with open(REGISTRY) as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return {}


def _remember(processes):
    os.makedirs(STATE, exist_ok=True)
    with open(REGISTRY, "w") as handle:
        json.dump(processes, handle, indent=2)


def _alive(pid):
    """Whether this process is still running.

    Signal 0 asks the kernel without sending anything, which is the only way to
    tell a live process from a recycled process id without more bookkeeping than
    a demo justifies.
    """
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ProcessLookupError):
        return False


def _start(name, argv, port):
    """Run one thing in the background, with its output on disk."""
    os.makedirs(LOGS, exist_ok=True)
    log_path = os.path.join(LOGS, f"{name}.log")
    log = open(log_path, "w")

    environment = dict(os.environ)
    environment.setdefault("RUST_LOG", "info,hlin=debug")

    process = subprocess.Popen(
        argv, cwd=cwd, stdout=log, stderr=subprocess.STDOUT, env=environment
    )

    processes = _registry()
    processes[name] = {"pid": process.pid, "log": log_path, "port": port}
    _remember(processes)
    return process.pid


def _stop(name, entry):
    pid = entry.get("pid")
    if not pid or not _alive(pid):
        return False

    os.kill(pid, signal.SIGTERM)
    for _ in range(50):
        if not _alive(pid):
            return True
        time.sleep(0.1)

    # It had five seconds and did not take them.
    os.kill(pid, signal.SIGKILL)
    time.sleep(0.2)
    return True


# -- Waiting --------------------------------------------------------------


def _get(path, timeout=5):
    """Fetch a URL, returning (status, body). Never raises for a status."""
    try:
        with urllib.request.urlopen(SHELL + path, timeout=timeout) as answer:
            return answer.status, answer.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace")
    except (urllib.error.URLError, OSError, TimeoutError) as error:
        return 0, str(error)


def _frontend_of(config):
    """Which front end a shell configuration points at.

    Read from the file rather than passed alongside it, so `up` cannot build one
    front end and serve another. A configuration that names none gets the
    default, which is what the shell itself does.
    """
    try:
        with open(config) as handle:
            for line in handle:
                line = line.strip()
                if line.startswith("frontend"):
                    path = line.split("=", 1)[1].strip().strip('"')
                    return os.path.basename(os.path.dirname(path))
    except OSError:
        pass
    return "frontend-demo"


def _database_listening():
    """Whether something already answers where the shell expects Postgres.

    The containerised database is a convenience, not a requirement: the shell
    connects to a URL and applies its own migrations, and somebody running their
    own Postgres on that port should not be made to start a second one. This
    only proves a socket accepts connections; whether it is a usable database is
    settled a moment later, when the shell connects and migrates.
    """
    import socket

    try:
        with socket.create_connection(("127.0.0.1", 55432), timeout=1):
            return True
    except OSError:
        return False


def _database_up():
    """Start the containerised database and wait for it to accept connections.

    The container reports healthy before Postgres finishes its first-run
    initialisation, so polling the socket is what actually says it is usable.
    """
    from angreal.integrations.docker import DockerCompose

    compose_file = os.path.join(cwd, "docker-compose.yml")
    result = DockerCompose(compose_file, project_name="hlin").up(detach=True)
    if not result.success:
        print(result.stderr)
        return 1

    deadline = time.time() + 60
    while time.time() < deadline:
        if _database_listening():
            return 0
        time.sleep(1)

    print("The database did not become ready in time.")
    return 1


def _compose_dex(*arguments):
    """Run `docker compose` for the Dex service alone.

    Named service and profile both, so starting or stopping it never touches the
    database in the same project — which may be somebody else's, started from
    another checkout.
    """
    compose_file = os.path.join(cwd, "docker-compose.yml")
    return subprocess.run(
        ["docker", "compose", "-f", compose_file, "-p", "hlin", "--profile", "collab", *arguments],
        cwd=cwd,
        capture_output=True,
        text=True,
    )


def _dex_up():
    """Start Dex with the current secret and wait until it publishes discovery.

    Recreated rather than merely started, because the secret is read when the
    container starts and a Dex left over from an earlier `up` holds a secret the
    new shell does not have — which fails at the callback, after a person has
    typed their password, as "this sign-in could not be verified".
    """
    result = _compose_dex("up", "-d", "--force-recreate", "dex")
    if result.returncode != 0:
        print(result.stderr, flush=True)
        return 1

    if not _wait_for(f"{DEX}/.well-known/openid-configuration", seconds=60):
        print("Dex did not come up. `docker logs hlin-dev-dex` says why.", flush=True)
        return 1
    return 0


def _dex_down():
    """Stop Dex, if it is running. It keeps nothing worth keeping."""
    probe = subprocess.run(
        ["docker", "ps", "-a", "--filter", "name=^hlin-dev-dex$", "--format", "{{.Names}}"],
        capture_output=True,
        text=True,
    )
    if "hlin-dev-dex" not in probe.stdout:
        return
    _compose_dex("rm", "--stop", "--force", "dex")
    print("stopped Dex", flush=True)


def _wait_for(url, seconds=60):
    """Poll a URL until it answers, rather than sleeping and hoping.

    Readiness detection is where demo scripts flake, and a fixed sleep is how
    they do it: too short on a cold machine, wasted time on a warm one.
    """
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2) as answer:
                if answer.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(0.5)
    return False


def _wait_for_panels(seconds=30):
    """Wait until the shell is offering every sample platform's panels, not
    merely answering.

    The registry polls platforms on its own schedule and the shell serves
    requests from the moment it binds, so there is a window where everything
    looks healthy and every layout renders empty. Waiting for a panel to exist
    is waiting for the thing a person actually opens the demo to see. Every
    platform's, because a test that puts three platforms' modules on one
    surface fails confusingly when the third has not been polled yet.
    """
    wanted = {platform["name"] for platform in PLATFORMS}
    deadline = time.time() + seconds
    while time.time() < deadline:
        status, body = _get("/api/panels")
        if status == 200:
            try:
                offering = {
                    platform.get("id") for platform in json.loads(body) if platform.get("panels")
                }
                if wanted <= offering:
                    return True
            except ValueError:
                pass
        time.sleep(0.5)
    return False


# Whether this `demo up` builds the browser's WebAssembly optimised. Set by
# `demo up --release`; read by every build step that runs Trunk.
RELEASE = False

# Whether a release build can run `wasm-opt`, which Trunk downloads the first
# time it needs it. Decided once per `demo up --release` (`_wasm_opt_ready`).
WASM_OPT = True

#: The binaryen release every `Trunk.toml` pins as `wasm_opt`.
WASM_OPT_VERSION = "version_123"


def _wasm_opt_ready():
    """Whether Trunk can have `wasm-opt`: already downloaded, or downloadable.

    Asked before building rather than learned from a failed build, because a
    failed build says only that a build failed. Where it cannot be had (no
    network, and never fetched), a release build still compiles the Rust
    optimised, and only the wasm-opt pass is skipped: about a tenth more
    bytes, not a debug build.
    """
    home = os.path.expanduser("~")
    caches = [
        os.path.join(home, "Library", "Caches", "dev.trunkrs.trunk"),
        os.path.join(os.environ.get("XDG_CACHE_HOME", os.path.join(home, ".cache")), "trunk"),
    ]
    for cache in caches:
        if os.path.isdir(os.path.join(cache, f"wasm-opt-{WASM_OPT_VERSION}")):
            return True
    if os.environ.get("TRUNK_OFFLINE", "").lower() in ("1", "true"):
        return False
    try:
        request = urllib.request.Request(
            f"https://github.com/WebAssembly/binaryen/releases/tag/{WASM_OPT_VERSION}",
            method="HEAD",
        )
        with urllib.request.urlopen(request, timeout=10):
            return True
    except (urllib.error.URLError, OSError):
        return False


# -- up, down, status -----------------------------------------------------


@demo()
@angreal.command(
    name="up",
    about="start the whole demo: database, frontend, three platforms and the shell",
    tool=angreal.ToolDescription(
        """
        Bring up everything needed to look at Hlin: the development database,
        the built frontend, three sample platforms and the shell, as background
        processes with logs under demo/state/logs.

        `--with collab` instead signs people in through Dex (a container, with
        users alice@example.com, bob@example.com and carol@elsewhere.org,
        password `password`), sets the client secret for Dex and the shell,
        starts the checklist (8083) and feed (8084) platforms rather than the
        sample platforms, and signs in as Alice to publish a surface with both
        side by side, which is what Alice lands on.

        `--release` builds the browser's WebAssembly (the frontend and every
        module) optimised, which is what a deployment serves and what any
        measurement of load time or size should use: the release profile,
        then `wasm-opt -Oz`, which Trunk downloads once. Offline and never
        downloaded, it says so and builds without wasm-opt. The shell and platform
        binaries stay debug builds: this demo signs in with `dev`, which a
        release shell refuses.

        `--with twenty` starts the widget platforms listed in `WIDGETS`
        (8201 upward), each its own process, after building their modules
        and own UIs in parallel (skipping any whose sources have not
        changed), and the shell on `dev` sign-in with a configuration listing
        them all at their `/hlin` subtrees; then publishes "Twenty", a
        scrolling surface with every widget on it. A converted widget's own
        UI is at its port's `/`, as the demo-only `--local-user`.

        `--with twenty-compose` is the same twenty as they are meant to be
        deployed (HLIN-I-0013), in containers from deploy/twenty/compose.yml
        (project `hlin-twenty`): each widget on its own `<name>.comp.test`
        name, the shell a release build on `oidc` against a Dex of its own
        (https at dex.localhost:5557, from a CA it makes under
        demo/state/twenty), and Postgres of its own. Builds all twenty-one
        images from one builder stage (long the first time, quick after),
        waits until every container is healthy, signs in through Dex as Alice
        to publish "Twenty", and prints http://127.0.0.1:8090. Leaves the
        process demo, the development database and 8080 alone; `--release`
        does not apply, everything is release. `docker compose ... stop
        clock` / `start clock` is a platform going down and coming back.

        ## When to use
        - To see the product running
        - Before `angreal demo walkthrough` (standard flavour)
        - Before `angreal e2e signin` (`--with collab`)
        - Before `angreal e2e twenty` (`--with twenty`)
        - To see the twenty deployed as intended (`--with twenty-compose`)

        Safe to run repeatedly; it stops whatever it previously started first.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="with_",
    long="with",
    takes_value=True,
    help=(
        "which shell configuration to run: demo, aurora, gallery, live, collab, twenty, "
        "or twenty-compose (the twenty in containers)"
    ),
)
@angreal.argument(
    name="release",
    long="release",
    takes_value=False,
    is_flag=True,
    help="build the frontend and modules optimised, as a deployment would",
)
def demo_up(with_=None, release=False):
    # Containers, release builds throughout, with nothing of the process
    # demo's: its own ports, Dex and database. So it neither stops nor is
    # stopped by the rest of `up`.
    if with_ == "twenty-compose":
        return _compose_up()

    global RELEASE, WASM_OPT
    RELEASE = bool(release)
    WASM_OPT = RELEASE and _wasm_opt_ready()
    if RELEASE and not WASM_OPT:
        print(
            f"wasm-opt ({WASM_OPT_VERSION}) is not downloaded and cannot be "
            "fetched; building release WebAssembly without it (about a tenth "
            "larger)",
            flush=True,
        )
    flavour = with_ or "demo"
    if flavour not in CONFIGS:
        print(f"no configuration called `{flavour}`. One of: {', '.join(CONFIGS)}")
        return 1
    config = os.path.join(cwd, CONFIGS[flavour])
    twenty = flavour == "twenty"
    if twenty:
        config = _write_twenty_config(config)

    # Which front end to build is read from the configuration rather than
    # guessed, so the one that gets built is the one that gets served.
    frontend = _frontend_of(config)

    print("stopping anything already running", flush=True)
    _stop_everything(keep_database=True)
    os.makedirs(STATE, exist_ok=True)
    with open(BUILD, "w") as handle:
        json.dump({"flavour": flavour, "release": RELEASE}, handle)

    if _database_listening():
        print("a database is already listening on 55432; leaving it alone", flush=True)
    else:
        print("starting the database", flush=True)
        if _database_up() != 0:
            return 1

    signed_in = flavour in SIGNED_IN
    if signed_in:
        # One value, handed to both sides. Whatever is already in the
        # environment wins, so somebody running the shell by hand can choose it.
        import secrets

        os.environ.setdefault(DEX_SECRET, secrets.token_urlsafe(24))
        print("starting Dex", flush=True)
        if _dex_up() != 0:
            return 1

    print(f"building {frontend}", flush=True)
    if subprocess.run(_trunk(), cwd=os.path.join(cwd, "examples", frontend)).returncode:
        print(
            "The frontend did not build.\n"
            "  cargo install trunk\n"
            "  rustup target add wasm32-unknown-unknown"
        )
        return 1

    print("building the binaries", flush=True)
    if twenty:
        platforms = [f"hlin-widget-{widget['name']}" for widget in WIDGETS]
    elif signed_in:
        platforms = [platform["binary"] for platform in COLLAB_PLATFORMS]
    else:
        platforms = ["hlin-sample-platform"]
    binaries = ["hlin"] + platforms
    build = subprocess.run(
        ["cargo", "build"] + [flag for binary in binaries for flag in ("--bin", binary)],
        cwd=cwd,
    )
    if build.returncode != 0:
        return 1

    if signed_in and _build_modules() != 0:
        return 1

    # Before the sample platforms start, for the reason `_build_modules`
    # gives: a platform reads its module's files once, when it starts.
    if not signed_in and not twenty:
        print("building the sample platforms' module", flush=True)
        if subprocess.run(_trunk(), cwd=os.path.join(cwd, SAMPLE_MODULE)).returncode != 0:
            print(
                "The sample platforms' module did not build.\n"
                "  cargo install trunk\n"
                "  rustup target add wasm32-unknown-unknown"
            )
            return 1

    if twenty:
        return _twenty_up(config)

    # The platforms verify the shell's tokens, and the shell is not up yet. That
    # is deliberate and worth seeing: a platform that cannot reach the issuer's
    # keys refuses callers until it can, and recovers by itself.
    for platform in [] if signed_in else PLATFORMS:
        argv = [
            "./target/debug/hlin-sample-platform",
            "--name",
            platform["name"],
            "--port",
            str(platform["port"]),
            "--auth",
            "token",
            "--shell-keys",
            f"{SHELL}/.well-known/hlin-keys.json",
            "--module-dir",
            os.path.join(cwd, SAMPLE_MODULE, "dist"),
        ]
        if platform["name"] == "orebank":
            # One panel behind a group, so `forbidden` has something to be
            # demonstrated by.
            argv += ["--restrict-health-to", "oncall"]

        pid = _start(platform["name"], argv, platform["port"])
        print(f"  {platform['name']} on {platform['port']} (pid {pid})", flush=True)

    for platform in COLLAB_PLATFORMS if signed_in else []:
        argv = [
            f"./target/debug/{platform['binary']}",
            "--name",
            platform["name"],
            "--port",
            str(platform["port"]),
            "--shell-keys",
            f"{SHELL}/.well-known/hlin-keys.json",
            "--module-dir",
            os.path.join(cwd, platform["module"], "dist"),
        ]
        pid = _start(platform["name"], argv, platform["port"])
        print(f"  {platform['name']} on {platform['port']} (pid {pid})", flush=True)

    for platform in COLLAB_PLATFORMS if signed_in else PLATFORMS:
        url = f"http://127.0.0.1:{platform['port']}/.well-known/hlin.json"
        if not _wait_for(url, seconds=30):
            print(f"{platform['name']} did not come up. See demo/state/logs.")
            return 1

    pid = _start("shell", ["./target/debug/hlin", "serve", "--config", config], 8080)
    print(f"  shell on 8080 (pid {pid})")

    # Waiting on the health endpoint alone is not enough, and the way it fails
    # is nasty: a shell left over from an earlier run holds port 8080, this one
    # exits with `Address already in use`, and the health check passes because
    # the *old* shell answers it. `up` then reports success while serving a
    # configuration nobody asked for — which is how a run pointed at this file
    # spent ten minutes measuring the previous one's refresh interval.
    #
    # Checking the process we actually started is still alive costs nothing and
    # names the problem instead of hiding it.
    if not _wait_for(f"{SHELL}/api/health", seconds=30) or not _alive(pid):
        if not _alive(pid):
            print(
                "The shell exited. Something else may be holding port 8080:\n"
                "  lsof -i :8080\n"
                "See demo/state/logs/shell.log."
            )
        else:
            print("The shell did not come up. See demo/state/logs/shell.log.")
        return 1

    # Answering is not the same as ready. The shell binds its port and serves
    # `/api/health` before its registry has polled anything, so for the first
    # second or two `/api/panels` is empty and a layout naming a panel renders
    # nothing at all. A person would refresh; a test suite reports a failure,
    # which is how the first browser run after `up` came to fail while every
    # later one passed.
    if signed_in:
        print(f"signing in as {COLLAB_AUTHOR} to publish `{COLLAB_TITLE}`", flush=True)
        if _publish_collab_surface() != 0:
            return 1
        print(f"\nHlin is running at {SHELL}, signing people in through Dex.")
        print("Sign in as alice@example.com, bob@example.com or carol@elsewhere.org;")
        print("the password is `password`.")
        print("`angreal e2e signin` proves it in a browser; `angreal e2e walkthrough`, in two.")
        print("`angreal demo down` stops everything.")
        return 0

    if not _wait_for_panels(seconds=30):
        print("The shell came up but no platform is offering panels. See demo/state/logs.")
        return 1

    print(f"\nHlin is running at {SHELL}")
    print("Open it, press Edit, and put a panel on the surface.")
    print("`angreal demo walkthrough` proves the degradation cases.")
    print("`angreal demo down` stops everything.")
    return 0


@demo()
@angreal.command(name="status", about="show what the demo is running")
def demo_status():
    processes = _registry()
    if not processes:
        print("Nothing recorded. Try `angreal demo up`.")
        return 0

    for name, entry in processes.items():
        running = _alive(entry["pid"])
        port = entry.get("port")
        answering = ""
        if running and port:
            url = (
                f"{SHELL}/api/health"
                if port == 8080
                else f"http://127.0.0.1:{port}/api/health"
            )
            answering = " and answering" if _wait_for(url, seconds=1) else " but silent"
        print(
            f"{name:12} pid {entry['pid']:<8} "
            f"{'running' + answering if running else 'not running'}"
        )
        print(f"{'':12} {entry['log']}")
    return 0


@demo()
@angreal.command(
    name="down",
    about="stop everything the demo started",
    tool=angreal.ToolDescription(
        """
        Stop the shell, the sample platforms (or the checklist and feed, and
        Dex, if `--with collab` started them, or the widgets, if `--with
        twenty` did), and the development database, and forget the process
        registry. Also the containerised twenty (`--with twenty-compose`),
        whose database and signing key go with it.

        ## When to use
        - When finished with the demo

        The database keeps its data; `angreal db reset` discards it.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="keep_database",
    long="keep-database",
    takes_value=False,
    is_flag=True,
    help="leave the development database running",
)
def demo_down(keep_database=False):
    # Not in `_stop_everything`, which `up` runs too: the containerised twenty
    # shares nothing with the process demo, so starting one leaves the other.
    _compose_down()
    return _stop_everything(keep_database=keep_database)


def _stop_everything(keep_database=False):
    """The body of `down`, as a plain function.

    Separate from the command because `up` calls it too, and calling a
    decorated angreal command from Python is calling the framework's wrapper
    rather than the work.
    """
    processes = _registry()

    # The shell goes first, so the platforms are not left answering a shell that
    # is halfway through shutting down.
    everything = PLATFORMS + COLLAB_PLATFORMS + WIDGETS
    for name in ["shell"] + [platform["name"] for platform in everything]:
        entry = processes.get(name)
        if entry and _stop(name, entry):
            print(f"stopped {name}", flush=True)

    for recorded in (REGISTRY, BUILD):
        if os.path.isfile(recorded):
            os.remove(recorded)

    # Whether or not the flavour that started it is the one being stopped: a
    # Dex left running holds its port and a secret nothing else knows.
    _dex_down()

    if not keep_database:
        _database_down()

    return 0


def _database_down():
    """Stop the containerised database, if that is what is running.

    A database somebody else started — their own Postgres on the same port —
    is theirs to stop. `up` left it alone, and so does this.
    """
    from angreal.integrations.docker import DockerCompose

    compose_file = os.path.join(cwd, "docker-compose.yml")
    result = DockerCompose(compose_file, project_name="hlin").ps(all=True)
    if not (result.stdout or "").strip():
        return

    if "hlin-dev-postgres" not in (result.stdout or ""):
        print("the database on 55432 was not started here; leaving it alone", flush=True)
        return

    DockerCompose(compose_file, project_name="hlin").down()
    print("stopped the database", flush=True)


# -- The collaborative demo's surface -------------------------------------


def _build_modules():
    """Build each collaborative platform's own UI module with Trunk.

    Before the platforms start, because a platform reads its module's files
    once, when it starts: one started before its module was built serves no
    module, and the shell draws that panel as its table fallback until the
    platform is restarted. That is the right way for a platform to fail, and
    the wrong way for a demo to begin.
    """
    for platform in COLLAB_PLATFORMS:
        print(f"building {platform['name']}'s module", flush=True)
        where = os.path.join(cwd, platform["module"])
        if subprocess.run(_trunk(), cwd=where).returncode != 0:
            print(
                f"{platform['name']}'s module did not build.\n"
                "  cargo install trunk\n"
                "  rustup target add wasm32-unknown-unknown"
            )
            return 1
    return 0


def _signed_in_as(email, password, shell=SHELL, context=None):
    """A URL opener carrying a session for this person, signed in through Dex.

    The same journey a browser makes, with nothing the shell does not offer
    everybody: `/auth/login` sends it to Dex, Dex's form is posted, and Dex
    sends it back to `/auth/callback`, which sets the session cookie. No
    backdoor, so what is seeded is exactly what a person could have composed.

    `shell` is where the shell is published, and `context` the TLS context to
    reach Dex with, where Dex is on https from a CA of the demo's own (the
    containerised twenty's).
    """
    import html
    import http.cookiejar
    import re
    import urllib.parse

    jar = http.cookiejar.CookieJar()
    handlers = [urllib.request.HTTPCookieProcessor(jar)]
    if context is not None:
        handlers.append(urllib.request.HTTPSHandler(context=context))
    opener = urllib.request.build_opener(*handlers)

    try:
        with opener.open(f"{shell}/auth/login?next=/api/config", timeout=15) as page:
            form_url = page.geturl()
            body = page.read().decode("utf-8", "replace")
    except (urllib.error.URLError, OSError) as error:
        print(f"could not reach Dex's sign-in form: {error}", flush=True)
        return None

    # The form posts back to where it was served, or to its own action.
    action = re.search(r'<form[^>]*action="([^"]+)"', body)
    if action:
        form_url = urllib.parse.urljoin(form_url, html.unescape(action.group(1)))

    fields = urllib.parse.urlencode({"login": email, "password": password}).encode()
    try:
        with opener.open(form_url, data=fields, timeout=15) as answer:
            landed = answer.geturl()
            config = json.loads(answer.read().decode("utf-8"))
    except (urllib.error.URLError, OSError, ValueError) as error:
        print(f"signing in as {email} failed: {error}", flush=True)
        return None

    if not landed.startswith(shell) or (config.get("principal") or {}).get("email") != email:
        print(f"signing in as {email} did not come back signed in (at {landed})", flush=True)
        return None
    return opener


def _as(opener, method, path, body=None, shell=SHELL):
    """One JSON request with a session. Returns (status, parsed body or None)."""
    request = urllib.request.Request(
        shell + path,
        data=None if body is None else json.dumps(body).encode("utf-8"),
        headers={"content-type": "application/json"},
        method=method,
    )
    try:
        with opener.open(request, timeout=10) as answer:
            text = answer.read().decode("utf-8")
            return answer.status, json.loads(text) if text else None
    except urllib.error.HTTPError as error:
        return error.code, error.read()[:200]
    except (urllib.error.URLError, OSError, TimeoutError, ValueError) as error:
        return 0, str(error)


def _publish_collab_surface():
    """Publish the surface people land on: the checklist and the feed.

    Signed in as Alice, through the shell's own API, so the layout is hers and
    the rules that apply to it are the ones that apply to any layout. Her own
    layout is replaced rather than a new one created, so running `up` again
    leaves one surface, not one per run; and replacing it makes it her most
    recently changed layout, which is what `/api/layouts/home` opens for her.

    Published, so anyone signed in may open it (and fork it). What each of
    them sees on it is still each platform's decision: Carol is not on the
    team list, so the checklist refuses her its panel.
    """
    opener = _signed_in_as(COLLAB_AUTHOR, COLLAB_PASSWORD)
    if opener is None:
        return 1

    # The registry polls on its own schedule; a panel it has not seen yet
    # would be accepted and drawn as gone.
    wanted = {(platform, key) for platform, key, *_ in COLLAB_PANELS}
    deadline = time.time() + 30
    while True:
        status, catalog = _as(opener, "GET", "/api/panels")
        offered = (
            {(p["id"], panel["key"]) for p in catalog for panel in p["panels"]}
            if status == 200
            else set()
        )
        if wanted <= offered:
            break
        if time.time() > deadline:
            missing = ", ".join(f"{p}/{k}" for p, k in sorted(wanted - offered))
            print(f"the shell is not offering {missing}. See demo/state/logs.", flush=True)
            return 1
        time.sleep(0.5)

    status, owned = _as(opener, "GET", "/api/layouts")
    if status != 200:
        print(f"could not list Alice's layouts: {status} {owned}", flush=True)
        return 1
    existing = next((layout for layout in owned if layout["title"] == COLLAB_TITLE), None)

    if existing:
        layout = existing["id"]
    else:
        status, created = _as(opener, "POST", "/api/layouts", {"title": COLLAB_TITLE})
        if status != 201:
            print(f"could not create the surface: {status} {created}", flush=True)
            return 1
        layout = created["id"]

    status, written = _as(
        opener,
        "PUT",
        f"/api/layouts/{layout}",
        {
            "title": COLLAB_TITLE,
            "visibility": "published",
            "panels": [
                {
                    "platform_id": platform,
                    "panel_key": key,
                    "selections": selections,
                    "position": {"x": x, "y": y, "w": w, "h": h},
                }
                for platform, key, selections, x, y, w, h in COLLAB_PANELS
            ],
        },
    )
    if status != 200:
        print(f"could not publish the surface: {status} {written}", flush=True)
        return 1

    print(f"  published {SHELL}/s/{layout}", flush=True)
    return 0


# -- Twenty widgets on one surface ----------------------------------------


def _write_twenty_config(head):
    """Write the shell configuration for the twenty-widget demo, and say where.

    The committed file holds everything but the platforms, which are appended
    from `WIDGETS`, so the list of widgets is written down once and a new one
    cannot be started without the shell being told about it. Each platform's
    `base_url` is its `/hlin` subtree, as it will be on a real host
    (HLIN-I-0013): the widget's own UI has the root.
    """
    with open(head) as handle:
        text = handle.read()
    for widget in WIDGETS:
        text += (
            "\n[[platforms]]\n"
            f'id = "{widget["name"]}"\n'
            f'base_url = "{_widget_origin(widget)}{HLIN_BASE}"\n'
            'auth = { strategy = "hlin-token" }\n'
        )
    os.makedirs(STATE, exist_ok=True)
    with open(TWENTY_CONFIG, "w") as handle:
        handle.write(text)
    return TWENTY_CONFIG


#: Where each widget serves Hlin's surface: the manifest, the module, the API
#: Hlin calls and the event stream. Its own UI is at the root.
HLIN_BASE = "/hlin"

#: Who a widget's own `/api/` answers as, so its own UI at the root works in
#: the demo. `--local-user` is demo-only: the widgets listen on loopback, and
#: nobody else can reach them.
LOCAL_USER = "Local User"


def _widget_origin(widget):
    return f"http://127.0.0.1:{widget['port']}"


def _widget_manifest(widget):
    return f"{_widget_origin(widget)}{HLIN_BASE}/.well-known/hlin.json"


def _widget_dir(widget):
    return os.path.join(cwd, "crates", "widgets", widget["name"])


def _widget_module(widget):
    return os.path.join(_widget_dir(widget), "module")


#: A widget's Trunk builds: its Hlin module, and, once it is converted to
#: components mounted twice (HLIN-I-0013), its own UI. Each is a Trunk project
#: in a folder of that name beside the widget's server, built into its `dist`,
#: from the package `hlin-widget-{name}-{kind}`.
BUILD_KINDS = ("module", "ui")


def _builds(widget):
    """Which of `BUILD_KINDS` this widget has."""
    return [
        kind
        for kind in BUILD_KINDS
        if os.path.isfile(os.path.join(_widget_dir(widget), kind, "Cargo.toml"))
    ]


#: What a widget's builds are made from, beyond their own folder and the
#: widget's `components/`: the crates every widget build is built on, and the
#: lockfile that pins everything else. A change to any of them rebuilds every
#: build.
MODULE_INPUTS = [
    "crates/widgets/hlin-widget-ui",
    "crates/widgets/hlin-widget-module",
    "crates/hlin-module/src",
    "crates/hlin-bridge/src",
    "Cargo.lock",
]

#: The file in a build's `dist` that says which sources it was built from. A
#: dotfile, which the widget does not serve (`ModuleFiles::read`).
STAMP = ".built-from"


def _build_fingerprint(widget, kind):
    """A hash of everything one of a widget's builds is made from.

    Content rather than modification times, so a checkout, a rebase or a
    `touch` that changes nothing does not cost a rebuild, and an edit always
    does.
    """
    import hashlib

    digest = hashlib.sha256()
    digest.update(b"release" if RELEASE else b"debug")
    # So a build made without wasm-opt, offline, is made again once it can
    # be had.
    digest.update(b"wasm-opt" if RELEASE and WASM_OPT else b"")
    own = [os.path.join(_widget_dir(widget), folder) for folder in (kind, "components")]
    roots = own + [os.path.join(cwd, path) for path in MODULE_INPUTS]
    for root in roots:
        paths = [root] if os.path.isfile(root) else []
        for directory, subdirectories, files in os.walk(root):
            subdirectories[:] = sorted(d for d in subdirectories if d not in ("dist", "target"))
            paths += [os.path.join(directory, name) for name in sorted(files)]
        for path in paths:
            digest.update(os.path.relpath(path, cwd).encode())
            with open(path, "rb") as handle:
                digest.update(handle.read())
    return digest.hexdigest()


def _build_is_current(widget, kind, fingerprint):
    dist = os.path.join(_widget_dir(widget), kind, "dist")
    try:
        with open(os.path.join(dist, STAMP)) as handle:
            stamped = handle.read().strip()
    except OSError:
        return False
    return stamped == fingerprint and os.path.isfile(os.path.join(dist, "index.html"))


def _trunk():
    """`trunk build`, optimised when `demo up --release` asked for it.

    Without `wasm-opt` (`WASM_OPT`), the release profile is asked of cargo
    rather than of Trunk: the same optimised Rust, and Trunk, which runs
    wasm-opt only for `--release`, leaves it out.
    """
    if not RELEASE:
        return ["trunk", "build"]
    if WASM_OPT:
        return ["trunk", "build", "--release"]
    return ["trunk", "build", "--cargo-profile", "release"]


def _build_widget_modules():
    """Build every widget's module and own UI whose sources changed since
    their last build.

    The slow part of the demo, so done in two steps that each run in
    parallel: one `cargo build` for every stale build at once, which compiles
    the dependencies they share once and uses every core; then one `trunk
    build` per build, all at once, each finding the compiling done and only
    running wasm-bindgen and writing `dist`. Run the other way round, twenty
    Trunks would queue on cargo's lock and compile one after another.
    """
    started = time.time()
    everything = [(widget, kind) for widget in WIDGETS for kind in _builds(widget)]
    fingerprints = {
        (widget["name"], kind): _build_fingerprint(widget, kind) for widget, kind in everything
    }
    stale = [
        (widget, kind)
        for widget, kind in everything
        if not _build_is_current(widget, kind, fingerprints[(widget["name"], kind)])
    ]
    current = len(everything) - len(stale)
    if current:
        print(f"  {current} widget builds unchanged since their last build", flush=True)
    if not stale:
        return 0

    names = ", ".join(f"{widget['name']} {kind}" for widget, kind in stale)
    print(f"building {len(stale)} widget builds: {names}", flush=True)
    packages = [
        flag for widget, kind in stale for flag in ("-p", f"hlin-widget-{widget['name']}-{kind}")
    ]
    compiled = subprocess.run(
        ["cargo", "build", "--target", "wasm32-unknown-unknown"]
        + (["--release"] if RELEASE else [])
        + packages,
        cwd=cwd,
    )
    if compiled.returncode != 0:
        print("The widget builds did not compile.\n  rustup target add wasm32-unknown-unknown")
        return 1

    os.makedirs(LOGS, exist_ok=True)
    running = []
    for widget, kind in stale:
        log_path = os.path.join(LOGS, f"{kind}-{widget['name']}.log")
        log = open(log_path, "w")
        process = subprocess.Popen(
            _trunk(),
            cwd=os.path.join(_widget_dir(widget), kind),
            stdout=log,
            stderr=subprocess.STDOUT,
        )
        running.append((widget, kind, process, log, log_path))

    failed = []
    for widget, kind, process, log, log_path in running:
        process.wait()
        log.close()
        if process.returncode != 0:
            failed.append((f"{widget['name']}'s {kind}", log_path))
            continue
        # Written last, so a build that failed halfway is never taken as
        # current.
        dist = os.path.join(_widget_dir(widget), kind, "dist")
        with open(os.path.join(dist, STAMP), "w") as handle:
            handle.write(fingerprints[(widget["name"], kind)])

    if failed:
        for name, log_path in failed:
            print(f"{name} did not build; see {log_path}", flush=True)
        print("  cargo install trunk")
        return 1

    print(f"  built in {time.time() - started:.1f}s", flush=True)
    return 0


def _twenty_up(config):
    """The rest of `up --with twenty`, once the shell and widgets are built."""
    if _build_widget_modules() != 0:
        return 1

    # The widgets verify the shell's tokens, and the shell is not up yet. As
    # with the sample platforms that is fine: each fetches the keys when the
    # first token arrives.
    for widget in WIDGETS:
        pid = _start(widget["name"], _widget_argv(widget), widget["port"])
        print(f"  {widget['name']} on {widget['port']} (pid {pid})", flush=True)

    for widget in WIDGETS:
        if not _wait_for(_widget_manifest(widget), seconds=30):
            print(f"{widget['name']} did not come up. See demo/state/logs/{widget['name']}.log.")
            return 1

    pid = _start("shell", ["./target/debug/hlin", "serve", "--config", config], 8080)
    print(f"  shell on 8080 (pid {pid})")
    # The process is checked as well as the port, for the reason `up` gives:
    # an old shell holding 8080 would answer for a new one that exited.
    if not _wait_for(f"{SHELL}/api/health", seconds=30) or not _alive(pid):
        print("The shell did not come up. See demo/state/logs/shell.log and `lsof -i :8080`.")
        return 1

    print(f"publishing `{TWENTY_TITLE}`", flush=True)
    if _publish_twenty() != 0:
        return 1

    print(f"\nHlin is running at {SHELL}, with {len(WIDGETS)} widgets on `{TWENTY_TITLE}`.")
    own = [widget for widget in WIDGETS if "ui" in _builds(widget)]
    if own:
        print(f"Each widget's own UI is at its own port, as {LOCAL_USER}:")
        for widget in own:
            print(f"  {widget['name']}: {_widget_origin(widget)}/")
    print("`angreal e2e twenty` proves it in a browser.")
    print("`angreal demo down` stops everything.")
    return 0


def _widget_argv(widget):
    """How one widget is started, by `up --with twenty` and by `restart`."""
    name = widget["name"]
    argv = [
        f"./target/debug/hlin-widget-{name}",
        "--name",
        name,
        "--port",
        str(widget["port"]),
        "--shell-keys",
        f"{SHELL}/.well-known/hlin-keys.json",
        "--hlin-base",
        HLIN_BASE,
        "--module-dir",
        os.path.join(_widget_module(widget), "dist"),
    ]
    if "ui" in _builds(widget):
        argv += [
            "--ui-dir",
            os.path.join(_widget_dir(widget), "ui", "dist"),
            "--local-user",
            LOCAL_USER,
        ]
    return argv


@demo()
@angreal.command(
    name="restart",
    about="start one of the twenty widgets again, after it was stopped or killed",
    tool=angreal.ToolDescription(
        """
        Start one widget of the twenty-widget demo again, exactly as `up
        --with twenty` started it, and wait until it answers. If it is still
        running it is stopped first.

        ## When to use
        - After killing a widget's process to watch its panel degrade alone
          (`angreal e2e twenty-measure` does this to one widget)

        Nothing is rebuilt, and the widget's memory starts empty: its counts,
        notes and cards are gone.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="name",
    required=True,
    help="the widget, as `WIDGETS` names it: clock, counter, poll, …",
)
def demo_restart(name=None):
    widget = next((widget for widget in WIDGETS if widget["name"] == name), None)
    if widget is None:
        print(f"no widget called `{name}`. One of: {', '.join(w['name'] for w in WIDGETS)}")
        return 1

    entry = _registry().get(name)
    if entry and _stop(name, entry):
        print(f"stopped {name}", flush=True)

    pid = _start(name, _widget_argv(widget), widget["port"])
    if not _wait_for(_widget_manifest(widget), seconds=30):
        print(f"{name} did not come up. See demo/state/logs/{name}.log.", flush=True)
        return 1
    print(f"  {name} on {widget['port']} (pid {pid})", flush=True)
    return 0


def _twenty_panels():
    """Every widget's panel, placed in reading order, three across."""
    return [
        {
            "platform_id": widget["name"],
            "panel_key": widget["name"],
            "selections": {},
            "position": {
                "x": (index % TWENTY_ACROSS) * TWENTY_WIDTH,
                "y": (index // TWENTY_ACROSS) * TWENTY_HEIGHT,
                "w": TWENTY_WIDTH,
                "h": TWENTY_HEIGHT,
            },
        }
        for index, widget in enumerate(WIDGETS)
    ]


def _publish_twenty(opener=None, shell=SHELL, logs="demo/state/logs"):
    """Publish "Twenty" as the development user, or as whoever `opener` is
    signed in as.

    With `dev` sign-in every request is that user, so the shell's own API is
    enough, with none of the collaborative demo's signing in first, and
    nothing here a person could not have done by hand. The containerised
    twenty signs in through Dex first and passes its session as `opener`.
    Their "Twenty" is replaced rather than a new one created, so running `up`
    again leaves one surface; and replacing it makes it their most recently
    changed layout, which is where they land.
    """
    opener = opener or urllib.request.build_opener()

    # The registry polls on its own schedule; a panel it has not seen yet
    # would be accepted and drawn as gone.
    wanted = {(widget["name"], widget["name"]) for widget in WIDGETS}
    deadline = time.time() + 30
    while True:
        status, catalog = _as(opener, "GET", "/api/panels", shell=shell)
        offered = (
            {(p["id"], panel["key"]) for p in catalog for panel in p["panels"]}
            if status == 200
            else set()
        )
        if wanted <= offered:
            break
        if time.time() > deadline:
            missing = ", ".join(f"{p}/{k}" for p, k in sorted(wanted - offered))
            print(f"the shell is not offering {missing}. See {logs}.", flush=True)
            return 1
        time.sleep(0.5)

    status, owned = _as(opener, "GET", "/api/layouts", shell=shell)
    if status != 200:
        print(f"could not list the layouts: {status} {owned}", flush=True)
        return 1
    existing = next((layout for layout in owned if layout["title"] == TWENTY_TITLE), None)
    if existing:
        layout = existing["id"]
    else:
        status, created = _as(
            opener, "POST", "/api/layouts", {"title": TWENTY_TITLE}, shell=shell
        )
        if status != 201:
            print(f"could not create the surface: {status} {created}", flush=True)
            return 1
        layout = created["id"]

    status, written = _as(
        opener,
        "PUT",
        f"/api/layouts/{layout}",
        {"title": TWENTY_TITLE, "visibility": "published", "panels": _twenty_panels()},
        shell=shell,
    )
    if status != 200:
        print(f"could not publish the surface: {status} {written}", flush=True)
        return 1

    print(f"  published {shell}/s/{layout}", flush=True)
    return 0


# -- The twenty, in containers --------------------------------------------

#: The containerised twenty (HLIN-T-0094): twenty widget containers, the
#: shell as a release build signing people in through Dex, and Postgres, on a
#: compose network and project of their own. Nothing here is shared with the
#: process demo: its own ports, its own Dex, its own database.
COMPOSE_FILE = os.path.join(cwd, "deploy", "twenty", "compose.yml")
COMPOSE_CONFIG = os.path.join(cwd, "deploy", "twenty", "hlin.toml")
COMPOSE_PROJECT = "hlin-twenty"

#: Where the containerised shell is published: loopback, and not 8080, so it
#: and the process demo can run at once.
COMPOSE_SHELL = "http://127.0.0.1:8090"

#: Dex's issuer there, which the browser and the shell's container both reach
#: (deploy/twenty/dex.yaml says how).
COMPOSE_DEX = "https://dex.localhost:5557/dex"

#: What `up` makes for it, mounted by the compose file: the client secret
#: Dex and the shell share, and Dex's certificate with the CA that signed it.
COMPOSE_STATE = os.path.join(STATE, "twenty")
COMPOSE_SECRET = os.path.join(COMPOSE_STATE, "oidc.env")
COMPOSE_TLS = os.path.join(COMPOSE_STATE, "tls")


def _compose(*arguments, capture=True):
    return subprocess.run(
        ["docker", "compose", "-f", COMPOSE_FILE, "-p", COMPOSE_PROJECT, *arguments],
        cwd=cwd,
        capture_output=capture,
        text=True,
    )


def _compose_drift():
    """What differs between `WIDGETS` and the compose file and shell
    configuration, or None.

    The two files are written out in full, so a person can read them and run
    `docker compose` on them by hand; this is what keeps them the same list.
    """
    import re

    wanted = [widget["name"] for widget in WIDGETS]
    with open(COMPOSE_CONFIG) as handle:
        config = handle.read()
    configured = re.findall(r'^id = "([^"]+)"', config, re.MULTILINE)
    if configured != wanted:
        return f"{COMPOSE_CONFIG} lists {configured}, not {wanted}"
    for widget in wanted:
        if f'base_url = "http://{widget}.comp.test:8080{HLIN_BASE}"' not in config:
            return f"{COMPOSE_CONFIG} does not put {widget} at {widget}.comp.test"

    services = _compose("config", "--services")
    if services.returncode != 0:
        return f"`docker compose config` failed: {services.stderr.strip()}"
    missing = sorted(set(wanted) - set(services.stdout.split()))
    if missing:
        return f"{COMPOSE_FILE} has no service for {', '.join(missing)}"
    return None


def _compose_secret():
    """The client secret Dex and the shell share, kept across `up`s.

    Kept, because the compose file reads it for both whenever either is
    (re)created, and a new one each time would recreate both for nothing.
    """
    import secrets

    os.makedirs(COMPOSE_STATE, exist_ok=True)
    if not os.path.isfile(COMPOSE_SECRET):
        with open(COMPOSE_SECRET, "w") as handle:
            handle.write(f"{DEX_SECRET}={secrets.token_urlsafe(24)}\n")


def _compose_tls():
    """A CA of the demo's own, and a certificate from it for `dex.localhost`.

    A release shell will sign nobody in through a plain-http issuer, so Dex
    serves https. Made once and kept: the shell trusts the CA through its
    `ca_bundle`, and a person who chose to trust it in their browser should
    not have to again. Nothing but this demo's Dex is signed by it.
    """
    ca, cert, key = (os.path.join(COMPOSE_TLS, name) for name in ("ca.pem", "dex.crt", "dex.key"))
    if all(os.path.isfile(path) for path in (ca, cert, key)):
        return 0
    os.makedirs(COMPOSE_TLS, exist_ok=True)
    ca_key = os.path.join(COMPOSE_TLS, "ca.key")
    request = os.path.join(COMPOSE_TLS, "dex.csr")
    extensions = os.path.join(COMPOSE_TLS, "dex.ext")
    with open(extensions, "w") as handle:
        handle.write(
            "basicConstraints = critical, CA:FALSE\n"
            "keyUsage = critical, digitalSignature, keyEncipherment\n"
            "extendedKeyUsage = serverAuth\n"
            "subjectAltName = DNS:dex.localhost\n"
        )
    steps = [
        ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "3650",
         "-keyout", ca_key, "-out", ca, "-subj", "/CN=Hlin twenty demo CA",
         "-addext", "basicConstraints=critical,CA:TRUE",
         "-addext", "keyUsage=critical,keyCertSign,cRLSign"],
        ["openssl", "req", "-newkey", "rsa:2048", "-nodes", "-keyout", key,
         "-out", request, "-subj", "/CN=dex.localhost"],
        ["openssl", "x509", "-req", "-in", request, "-CA", ca, "-CAkey", ca_key,
         "-CAcreateserial", "-days", "825", "-out", cert, "-extfile", extensions],
    ]
    for step in steps:
        made = subprocess.run(step, capture_output=True, text=True)
        if made.returncode != 0:
            print(f"could not make Dex's certificate: {made.stderr.strip()}", flush=True)
            return 1
    # Dex runs as its own user in its container and reads the key through a
    # bind mount. A throwaway key for a throwaway CA, on loopback.
    os.chmod(key, 0o644)
    return 0


def _compose_context():
    """A TLS context trusting the demo's CA, to reach Dex as the shell does."""
    import ssl

    return ssl.create_default_context(cafile=os.path.join(COMPOSE_TLS, "ca.pem"))


def _wait_for_tls(url, context, seconds=60):
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2, context=context) as answer:
                if answer.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(0.5)
    return False


def _compose_up():
    """`demo up --with twenty-compose`: build, start, publish, say where."""
    drift = _compose_drift()
    if drift:
        print(f"the containerised twenty has drifted from WIDGETS: {drift}", flush=True)
        return 1
    _compose_secret()
    if _compose_tls() != 0:
        return 1

    # Built by compose, all twenty-one images from the one builder stage;
    # quick when nothing changed, since that stage is cached.
    print("building the images (the first build takes a while)", flush=True)
    started = time.time()
    if _compose("build", capture=False).returncode != 0:
        print("The images did not build.", flush=True)
        return 1
    print(f"  built in {time.time() - started:.0f}s", flush=True)

    print("starting Postgres, Dex, the shell and twenty widgets", flush=True)
    if _compose("up", "-d", "--wait", "--wait-timeout", "180", capture=False).returncode != 0:
        print(
            "Not everything came up healthy. "
            f"`docker compose -f {os.path.relpath(COMPOSE_FILE, cwd)} -p {COMPOSE_PROJECT} ps` "
            "says which.",
            flush=True,
        )
        return 1

    context = _compose_context()
    if not _wait_for_tls(f"{COMPOSE_DEX}/.well-known/openid-configuration", context):
        print(f"Dex is not answering at {COMPOSE_DEX}.", flush=True)
        return 1
    if not _wait_for(f"{COMPOSE_SHELL}/api/health", seconds=60):
        print(f"The shell is not answering at {COMPOSE_SHELL}.", flush=True)
        return 1

    print(f"signing in as {COLLAB_AUTHOR} to publish `{TWENTY_TITLE}`", flush=True)
    opener = _signed_in_as(COLLAB_AUTHOR, COLLAB_PASSWORD, shell=COMPOSE_SHELL, context=context)
    if opener is None:
        return 1
    logs = f"`docker compose -p {COMPOSE_PROJECT} logs`"
    if _publish_twenty(opener, shell=COMPOSE_SHELL, logs=logs) != 0:
        return 1

    compose = f"docker compose -f {os.path.relpath(COMPOSE_FILE, cwd)} -p {COMPOSE_PROJECT}"
    ca = os.path.relpath(os.path.join(COMPOSE_TLS, "ca.pem"), cwd)
    print(f"\nHlin is running at {COMPOSE_SHELL}, in containers, with {len(WIDGETS)} widgets")
    print(f"on `{TWENTY_TITLE}`. Sign in as alice@example.com, bob@example.com or")
    print("carol@elsewhere.org; the password is `password`.")
    print(f"Dex is at {COMPOSE_DEX}, on https from a CA of this demo's own ({ca}),")
    print("so a browser warns once unless told to trust it.")
    print("A platform going down and coming back:")
    print(f"  {compose} stop clock")
    print(f"  {compose} start clock")
    print("`angreal demo down` stops everything.")
    return 0


def _compose_down():
    """Stop the containerised twenty, if it is running, and throw away its
    database and the shell's key: `up` publishes "Twenty" again."""
    running = _compose("ps", "-a", "-q")
    if running.returncode != 0 or not running.stdout.strip():
        return
    _compose("down", "--volumes", "--remove-orphans")
    print("stopped the containerised twenty", flush=True)


# -- A surface worth opening ----------------------------------------------


#: The surface `demo compose` builds, in reading order.
#:
#: Chosen to make three claims at once rather than to fill a grid. Panels come
#: from two platforms that know nothing about each other. Every view kind in the
#: vocabulary appears. And two of them name a design-system component Hlin has
#: no word for, so the same surface shows both what the vocabulary covers and
#: what a design system adds on top of it.
COMPOSITION = [
    # The two that ask for an Aurora component, first, because they are the
    # thing worth looking at.
    # The graph gets the extra row: a DAG that runs off the bottom of its panel
    # is a worse advertisement for component forwarding than no DAG at all.
    ("orebank", "saturation", 0, 0, 3, 4),
    ("orebank", "pipeline", 3, 0, 6, 4),
    ("stampmill", "saturation", 9, 0, 3, 4),
    # Then the vocabulary, doing what it does.
    ("orebank", "live-rate", 0, 4, 3, 4),
    # Four rows, not three: a chart with axes, a unit and a legend loses its
    # bottom axis in three, and the bottom axis is where the time is.
    ("stampmill", "live-throughput", 3, 4, 6, 4),
    ("orebank", "worker-health", 9, 4, 3, 4),
    ("stampmill", "queue-depth", 0, 8, 6, 4),
    ("orebank", "throughput", 6, 8, 6, 4),
    # Last and full width, because it is the one to touch: pick a cluster and
    # watch the answer change while you watch it. It was missing from this list
    # entirely, so the panel that demonstrates a control demonstrated it to
    # nobody.
    # Six wide, like the other charts. A chart keeps its aspect when it scales,
    # so a full-width panel does not give it a wider picture — it gives it the
    # same picture with empty margins either side.
    ("orebank", "throughput-by-cluster", 0, 12, 6, 4),
    # The drill-down, beside the graph whose colours it explains: click a stage,
    # see what has just happened to it.
    ("orebank", "stage-activity", 6, 12, 6, 4),
]


@demo()
@angreal.command(
    name="compose",
    about="build a surface worth looking at, from both platforms",
    tool=angreal.ToolDescription(
        """
        Compose a curated surface across both sample platforms and print its
        URL: every view kind, plus two panels that ask for a design-system
        component Hlin has no vocabulary for.

        ## When to use
        - After `angreal demo up`, to have something to open

        Creates a layout of its own each time and leaves existing ones alone.
        """,
        risk_level="safe",
    ),
)
def demo_compose():
    status, body = _get("/api/panels")
    if status != 200:
        print("the shell is not answering. Try `angreal demo up`.")
        return 1

    offered = {
        (platform["id"], panel["key"])
        for platform in json.loads(body)
        for panel in platform["panels"]
    }

    # A panel the platforms are not currently offering is skipped rather than
    # sent: the shell would accept it and the surface would carry a panel that
    # can only ever say it is gone, which is a real behaviour but not one this
    # surface is trying to show.
    wanted = [entry for entry in COMPOSITION if (entry[0], entry[1]) in offered]
    for platform, key, *_ in COMPOSITION:
        if (platform, key) not in offered:
            print(f"skipping {platform}/{key}; it is not being offered")

    if not wanted:
        print("no panels to compose. Are both platforms up?")
        return 1

    layout = _post("/api/layouts", {"title": "Everything, composed"})
    if layout is None:
        return 1

    written = _put(
        f"/api/layouts/{layout}",
        {
            "title": "Everything, composed",
            "visibility": "personal",
            "panels": [
                {
                    "platform_id": platform,
                    "panel_key": key,
                    "selections": {},
                    "position": {"x": x, "y": y, "w": w, "h": h},
                }
                for platform, key, x, y, w, h in wanted
            ],
        },
    )
    if written is None:
        return 1

    print(f"\n{len(wanted)} panels from two platforms that share nothing:\n")
    print(f"  {SHELL}/s/{layout}")
    print(f"  {SHELL}/s/{layout}?pack=aurora   (with the gallery frontend)")
    print(
        "\n`saturation` and `pipeline` ask for a component by name.\n"
        "Aurora draws them as a meter and a graph; a pack without those\n"
        "components draws the stat and the table they also are."
    )
    return 0


def _post(path, body):
    """POST JSON, returning the new id or None having said why."""
    return _send("POST", path, body)


def _put(path, body):
    return _send("PUT", path, body)


def _send(method, path, body):
    request = urllib.request.Request(
        SHELL + path,
        data=json.dumps(body).encode("utf-8"),
        headers={"content-type": "application/json"},
        method=method,
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as answer:
            return json.loads(answer.read().decode("utf-8")).get("id")
    except urllib.error.HTTPError as error:
        print(f"{method} {path} was refused: {error.code} {error.read()[:200]}")
    except (urllib.error.URLError, OSError, TimeoutError) as error:
        print(f"{method} {path} failed: {error}")
    return None
