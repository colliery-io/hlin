"""Frontend tasks.

A front end is a binary that chooses a design pack and mounts `hlin-ui`. There
is one per pack in `examples/`, and the shell serves whichever one its
configuration points at, so swapping the design system is a build and a config
line rather than a change to anything in `crates/`.

`trunk` drives the build, because a `wasm32` target and a server binary do not
share one crate cleanly.
"""

import os
import subprocess

import angreal

cwd = os.path.join(angreal.get_root(), "..")
EXAMPLES = os.path.join(cwd, "examples")

#: The front ends in this repository, one per design pack. `frontend-demo` is
#: what the repository demonstrates itself with, depending on nothing published;
#: `frontend-aurora` draws with Colliery's Aurora Dark; `frontend-gallery`
#: holds both and chooses at runtime, which is a demonstration rather than a
#: thing to deploy.
FRONTENDS = ["frontend-demo", "frontend-aurora", "frontend-gallery"]
DEFAULT = "frontend-demo"


def _require_trunk():
    if subprocess.run(["which", "trunk"], capture_output=True).returncode != 0:
        print(
            "trunk is not installed, and it is what builds the frontend.\n"
            "  cargo install trunk\n"
            "  rustup target add wasm32-unknown-unknown"
        )
        return False
    return True


def _at(name):
    """Where a front end lives, or a message saying it does not."""
    if name not in FRONTENDS:
        print(f"no front end named `{name}`. One of: {', '.join(FRONTENDS)}")
        return None
    return os.path.join(EXAMPLES, name)


ui = angreal.command_group(name="ui", about="commands for the frontend")


@ui()
@angreal.command(
    name="build",
    about="build a frontend the shell can serve",
    tool=angreal.ToolDescription(
        """
        Compile one of the front ends to WebAssembly, into its own `dist`
        directory, which is where the shell looks when its configuration points
        at it.

        ## When to use
        - Before running the shell, if you want something to look at
        - After changing anything in crates/hlin-ui or a design pack

        `--which frontend-aurora` builds the one drawn by Aurora Dark.
        """,
        risk_level="safe",
    ),
)
@angreal.argument(
    name="which",
    long="which",
    takes_value=True,
    help="which front end to build (default: frontend-demo)",
)
def ui_build(which=None):
    if not _require_trunk():
        return 1
    where = _at(which or DEFAULT)
    if where is None:
        return 1
    return subprocess.run(["trunk", "build"], cwd=where).returncode


@ui()
@angreal.command(name="watch", about="rebuild a frontend as it changes")
@angreal.argument(
    name="which",
    long="which",
    takes_value=True,
    help="which front end to watch (default: frontend-demo)",
)
def ui_watch(which=None):
    if not _require_trunk():
        return 1
    where = _at(which or DEFAULT)
    if where is None:
        return 1
    return subprocess.run(["trunk", "watch"], cwd=where).returncode


@ui()
@angreal.command(name="clean", about="remove every built frontend")
def ui_clean():
    import shutil

    removed = []
    for name in FRONTENDS:
        dist = os.path.join(EXAMPLES, name, "dist")
        if os.path.isdir(dist):
            shutil.rmtree(dist)
            removed.append(name)

    print(
        f"removed the build of {', '.join(removed)}" if removed else "nothing to remove"
    )
    return 0
