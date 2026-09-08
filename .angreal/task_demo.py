"""The demo: one command up, one command that proves it.

Everything the initiative built, running together, reproducible by anyone with
the repository. `demo up` starts the database, builds the frontend and starts
two sample platforms and the shell. `demo walkthrough` then drives the whole
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
LOGS = os.path.join(STATE, "logs")
#: The shell configurations in this repository. Each names a frontend, which is
#: how the demo chooses a design system: a front end is a binary that picked a
#: pack, and the shell serves whichever one it is pointed at.
CONFIGS = {
    "demo": "demo/hlin.toml",
    "aurora": "demo/hlin-aurora.toml",
    "gallery": "demo/hlin-gallery.toml",
    "live": "demo/hlin-live.toml",
}

SHELL = "http://127.0.0.1:8080"

#: What `up` starts, in the order it starts them. The shell is last because the
#: platforms verify tokens against keys it publishes.
PLATFORMS = [
    {"name": "orebank", "port": 8081},
    {"name": "stampmill", "port": 8082},
]

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
    """Wait until the shell is offering panels, not merely answering.

    The registry polls platforms on its own schedule and the shell serves
    requests from the moment it binds, so there is a window where everything
    looks healthy and every layout renders empty. Waiting for a panel to exist
    is waiting for the thing a person actually opens the demo to see.
    """
    deadline = time.time() + seconds
    while time.time() < deadline:
        status, body = _get("/api/panels")
        if status == 200:
            try:
                if any(platform.get("panels") for platform in json.loads(body)):
                    return True
            except ValueError:
                pass
        time.sleep(0.5)
    return False


# -- up, down, status -----------------------------------------------------


@demo()
@angreal.command(
    name="up",
    about="start the whole demo: database, frontend, two platforms and the shell",
    tool=angreal.ToolDescription(
        """
        Bring up everything needed to look at Hlin: the development database,
        the built frontend, two sample platforms and the shell, as background
        processes with logs under demo/state/logs.

        ## When to use
        - To see the product running
        - Before `angreal demo walkthrough`

        Safe to run repeatedly; it stops whatever it previously started first.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="with_",
    long="with",
    takes_value=True,
    help="which shell configuration to run: demo, aurora, gallery or live",
)
def demo_up(with_=None):
    flavour = with_ or "demo"
    if flavour not in CONFIGS:
        print(f"no configuration called `{flavour}`. One of: {', '.join(CONFIGS)}")
        return 1
    config = os.path.join(cwd, CONFIGS[flavour])

    # Which front end to build is read from the configuration rather than
    # guessed, so the one that gets built is the one that gets served.
    frontend = _frontend_of(config)

    print("stopping anything already running", flush=True)
    _stop_everything(keep_database=True)

    if _database_listening():
        print("a database is already listening on 55432; leaving it alone", flush=True)
    else:
        print("starting the database", flush=True)
        if _database_up() != 0:
            return 1

    print(f"building {frontend}", flush=True)
    if subprocess.run(["trunk", "build"], cwd=os.path.join(cwd, "examples", frontend)).returncode:
        print(
            "The frontend did not build.\n"
            "  cargo install trunk\n"
            "  rustup target add wasm32-unknown-unknown"
        )
        return 1

    print("building the binaries", flush=True)
    build = subprocess.run(
        ["cargo", "build", "--bin", "hlin", "--bin", "hlin-sample-platform"], cwd=cwd
    )
    if build.returncode != 0:
        return 1

    # The platforms verify the shell's tokens, and the shell is not up yet. That
    # is deliberate and worth seeing: a platform that cannot reach the issuer's
    # keys refuses callers until it can, and recovers by itself.
    for platform in PLATFORMS:
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
        ]
        if platform["name"] == "orebank":
            # One panel behind a group, so `forbidden` has something to be
            # demonstrated by.
            argv += ["--restrict-health-to", "oncall"]

        pid = _start(platform["name"], argv, platform["port"])
        print(f"  {platform['name']} on {platform['port']} (pid {pid})", flush=True)

    for platform in PLATFORMS:
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
        Stop the shell, both sample platforms and the development database, and
        forget the process registry.

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
    for name in ["shell"] + [platform["name"] for platform in PLATFORMS]:
        entry = processes.get(name)
        if entry and _stop(name, entry):
            print(f"stopped {name}", flush=True)

    if os.path.isfile(REGISTRY):
        os.remove(REGISTRY)

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
