"""
Docs tasks: `angreal docs build`, `angreal docs serve` and `angreal docs api`.

The docs are an Astro Starlight site in docs/, with the Colliery theme
starlight-aurora (HLIN-T-0107). They need Node 22 or later, and plissken for
the API reference: `plissken render` writes it into
docs/src/content/docs/reference/api/ (see plissken.toml).
"""

import shutil
import subprocess
import sys
from pathlib import Path

import angreal  # type: ignore

PROJECT_ROOT = Path(angreal.get_root()).parent
DOCS_ROOT = PROJECT_ROOT / "docs"

docs = angreal.command_group(name="docs", about="documentation site commands")


def _plissken():
    plissken = shutil.which("plissken")
    if not plissken:
        print("plissken not found: cargo install plissken --version 0.1.0", file=sys.stderr)
        return 1
    return subprocess.run([plissken, "render"], cwd=str(PROJECT_ROOT)).returncode


def _npm(*args):
    npm = shutil.which("npm")
    if not npm:
        print("npm not found: install Node 22 or later", file=sys.stderr)
        return 1
    if not (DOCS_ROOT / "node_modules").exists():
        result = subprocess.run([npm, "ci", "--no-audit", "--no-fund"], cwd=str(DOCS_ROOT))
        if result.returncode != 0:
            return result.returncode
    return subprocess.run([npm, *args], cwd=str(DOCS_ROOT)).returncode


@docs()
@angreal.command(
    name="build",
    about="generate the API docs and build the site into docs/dist",
    tool=angreal.ToolDescription(
        "Runs `plissken render` for the API reference, then builds the Starlight "
        "docs site in docs/ into docs/dist. Installs the npm dependencies on the "
        "first run. Fails when plissken is missing or a page does not build.",
        risk_level="safe",
    ),
)
def docs_build():
    return _plissken() or _npm("run", "build")


@docs()
@angreal.command(
    name="serve",
    about="generate the API docs and serve the site with live reload",
    tool=angreal.ToolDescription(
        "Runs `plissken render`, then serves the docs site at "
        "http://localhost:4321 and reloads on each change. In a terminal it runs "
        "until Ctrl+C. From a script or an agent, Astro runs it in the "
        "background: stop it with `npx astro dev stop` in docs/.",
        risk_level="safe",
    ),
)
def docs_serve():
    return _plissken() or _npm("run", "dev")


@docs()
@angreal.command(
    name="api",
    about="regenerate the API docs only (plissken render)",
    tool=angreal.ToolDescription(
        "Runs `plissken render`, which writes the API reference pages and "
        "_sidebar.mjs into docs/src/content/docs/reference/api/.",
        risk_level="safe",
    ),
)
def docs_api():
    return _plissken()
