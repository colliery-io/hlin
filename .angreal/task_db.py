"""Development database tasks.

The shell's store is Postgres (decision HLIN-A-0006), so a database is part of
running the tests rather than an optional extra. These tasks start one in a
container, migrate it, and throw it away again.
"""

import os
import subprocess
import time

import angreal
from angreal.integrations.docker import DockerCompose

cwd = os.path.join(angreal.get_root(), "..")
COMPOSE_FILE = os.path.join(cwd, "docker-compose.yml")
MIGRATIONS = os.path.join(cwd, "crates", "hlin", "migrations")

#: Where the containerised development database listens. Deliberately not 5432,
#: so it cannot collide with, or be mistaken for, a database anyone cares about.
DEV_DATABASE_URL = "postgres://hlin:hlin@localhost:55432/hlin"

db = angreal.command_group(name="db", about="commands for the development database")


def _compose():
    return DockerCompose(COMPOSE_FILE, project_name="hlin")


def _wait_until_ready(timeout_seconds=60):
    """Block until the database answers, or give up.

    The container reports healthy before Postgres finishes its first-run
    initialisation, so polling the socket is what actually tells us the
    database is usable.
    """
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        probe = subprocess.run(
            ["docker", "exec", "hlin-dev-postgres", "pg_isready", "-U", "hlin", "-d", "hlin"],
            capture_output=True,
        )
        if probe.returncode == 0:
            return True
        time.sleep(1)
    return False


@db()
@angreal.command(
    name="up",
    about="start the development database",
    tool=angreal.ToolDescription(
        """
        Start the containerised Postgres the shell's tests and local runs use,
        wait for it to accept connections, and apply migrations.

        ## When to use
        - Before running store tests or the shell locally
        - After `angreal db down`

        Safe to run when it is already up; it is a no-op then.
        """,
        risk_level="safe",
    ),
)
def db_up():
    result = _compose().up(detach=True)
    if not result.success:
        print(result.stderr)
        return 1

    if not _wait_until_ready():
        print("Database did not become ready in time.")
        return 1

    print(f"Database ready at {DEV_DATABASE_URL}")
    return _migrate()


@db()
@angreal.command(name="down", about="stop the development database, keeping its data")
def db_down():
    result = _compose().down()
    if not result.success:
        print(result.stderr)
        return 1
    print("Database stopped. Data is kept; use `angreal db reset` to discard it.")
    return 0


@db()
@angreal.command(
    name="reset",
    about="discard the development database and start a fresh one",
    tool=angreal.ToolDescription(
        """
        Destroy the development database including its volume, then start and
        migrate a fresh one.

        ## When to use
        - Migrations changed in a way that is not additive
        - Local data is in a state not worth understanding

        Destroys local development data. It touches nothing outside the
        containerised development database.
        """,
        risk_level="destructive",
    ),
)
def db_reset():
    result = _compose().down(volumes=True, remove_orphans=True)
    if not result.success:
        print(result.stderr)
        return 1
    print("Discarded the development database.")
    return db_up()


@db()
@angreal.command(name="migrate", about="apply pending migrations")
def db_migrate():
    return _migrate()


def _migrate():
    """Apply migrations with sqlx-cli, if it is installed.

    Migrations are also embedded in the shell binary and applied on start, so
    this is a convenience for working against the database directly rather than
    the only way they are applied.
    """
    database_url = os.environ.get("DATABASE_URL", DEV_DATABASE_URL)
    probe = subprocess.run(["which", "sqlx"], capture_output=True)
    if probe.returncode != 0:
        print(
            "sqlx-cli is not installed, so migrations were not applied here.\n"
            "The shell applies them on start, or: cargo install sqlx-cli --no-default-features --features postgres"
        )
        return 0

    result = subprocess.run(
        ["sqlx", "migrate", "run", "--source", MIGRATIONS, "--database-url", database_url],
        cwd=cwd,
    )
    return result.returncode


@db()
@angreal.command(name="status", about="show whether the development database is running")
def db_status():
    result = _compose().ps(all=True)
    print(result.stdout or "No containers.")
    if _wait_until_ready(timeout_seconds=1):
        print(f"\nReady at {DEV_DATABASE_URL}")
    else:
        print("\nNot accepting connections. Try `angreal db up`.")
    return 0
