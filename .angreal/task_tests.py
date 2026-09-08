import angreal
import subprocess
import os

cwd = os.path.join(angreal.get_root(), '..')
test = angreal.command_group(name="test", about="commands for running tests")


def get_crates():
    """Discover crates in the workspace."""
    crates_dir = os.path.join(cwd, 'crates')
    if not os.path.isdir(crates_dir):
        return []
    return [d for d in os.listdir(crates_dir)
            if os.path.isfile(os.path.join(crates_dir, d, 'Cargo.toml'))]


def _run(cmd):
    """Run a command and exit with its return code."""
    result = subprocess.run(cmd, cwd=cwd)
    raise SystemExit(result.returncode)


def _add_crate_filter(cmd, crate_name, filter_str):
    """Add crate and filter arguments to a cargo command."""
    if crate_name:
        cmd.extend(["-p", crate_name])
    cmd.extend(["--", "--test-threads=1"])
    if filter_str:
        cmd.append(filter_str)
    return cmd


@test()
@angreal.command(name="unit", about="run unit tests (cargo test --lib)")
@angreal.argument(name="crate_name", required=False, help="specific crate to test (default: all)")
@angreal.argument(name="filter", long="filter", short="f", required=False, help="filter for specific tests")
def unit_tests(crate_name="", filter=""):
    cmd = ["cargo", "test", "--lib", "-v"]
    _run(_add_crate_filter(cmd, crate_name, filter))


@test()
@angreal.command(name="integration", about="run integration tests (tests/integration.rs)")
@angreal.argument(name="crate_name", required=False, help="specific crate to test (default: all)")
@angreal.argument(name="filter", long="filter", short="f", required=False, help="filter for specific tests")
def integration_tests(crate_name="", filter=""):
    cmd = ["cargo", "test", "--test", "integration"]
    _run(_add_crate_filter(cmd, crate_name, filter))


@test()
@angreal.command(name="functional", about="run functional tests (tests/functional.rs)")
@angreal.argument(name="crate_name", required=False, help="specific crate to test (default: all)")
@angreal.argument(name="filter", long="filter", short="f", required=False, help="filter for specific tests")
@angreal.argument(
    name="ignored",
    long="ignored",
    takes_value=False,
    help="include #[ignore] tests (e.g. tests needing external services)",
)
def functional_tests(crate_name="", filter="", ignored=False):
    cmd = ["cargo", "test", "--test", "functional"]
    if crate_name:
        cmd.extend(["-p", crate_name])
    cmd.extend(["--", "--test-threads=1"])
    if ignored:
        cmd.append("--ignored")
    if filter:
        cmd.append(filter)
    _run(cmd)


def _say_whether_a_database_is_there():
    """Print whether the store suite will run against Postgres.

    The test decides for itself and prints its own reason, but cargo captures
    stderr from a test that passes — so the one case worth noticing, a skip, is
    the one nobody sees. Saying it here puts it where a person is already
    looking.

    HLIN-A-0006 makes Postgres the only production backend, so a run without one
    has not checked the store that matters.
    """
    import socket

    try:
        with socket.create_connection(("127.0.0.1", 55432), timeout=1):
            print(
                "database on 55432: the store suite will run against Postgres",
                flush=True,
            )
            return
    except OSError:
        pass

    print(
        "no database on 55432: the Postgres store suite will SKIP.\n"
        "  Postgres is the only production backend (HLIN-A-0006), so this run\n"
        "  does not check it. `angreal db up` starts one; HLIN_REQUIRE_DATABASE=1\n"
        "  turns the skip into a failure, which is what CI should set.",
        flush=True,
    )


@test()
@angreal.command(name="all", about="run all tests (unit, integration, and functional)")
@angreal.argument(name="crate_name", required=False, help="specific crate to test (default: all)")
@angreal.argument(
    name="ignored",
    long="ignored",
    takes_value=False,
    help="include #[ignore] tests",
)
def all_tests(crate_name="", ignored=False):
    # `--no-fail-fast` because the default stops at the first test binary that
    # fails, and every crate after it silently never runs. A partial run then
    # reports a partial total, which reads exactly like a full one — that is
    # how a failing test went unnoticed across two commits while the runs it
    # truncated were reported as passing.
    _say_whether_a_database_is_there()

    cmd = ["cargo", "test", "-v", "--no-fail-fast"]
    if crate_name:
        cmd.extend(["-p", crate_name])
    cmd.extend(["--", "--test-threads=1"])
    if ignored:
        cmd.append("--ignored")
    _run(cmd)


@test()
@angreal.command(name="coverage", about="generate code coverage report")
@angreal.argument(
    name="type",
    long="type",
    short="t",
    required=False,
    help="test type to measure: unit, integration, functional, all (default: all)",
)
@angreal.argument(
    name="open",
    long="open",
    short="o",
    takes_value=False,
    help="open HTML report in browser",
)
def coverage(type="all", open=False):
    import webbrowser

    output_dir = os.path.join(cwd, "coverage")

    # Build the test selection flags for cargo-llvm-cov
    test_flags = []
    if type == "unit":
        test_flags = ["--lib"]
    elif type == "integration":
        test_flags = ["--test", "integration"]
    elif type == "functional":
        test_flags = ["--test", "functional"]
    # "all" uses no flags (runs everything)

    # Generate coverage
    run_cmd = ["cargo", "llvm-cov", "--workspace", "--no-report"] + test_flags + ["--", "--test-threads=1"]
    report_html = ["cargo", "llvm-cov", "report", "--workspace", "--html", "--output-dir", output_dir]
    report_lcov = ["cargo", "llvm-cov", "report", "--workspace", "--lcov", "--output-path", os.path.join(output_dir, "lcov.info")]
    report_text = ["cargo", "llvm-cov", "report", "--workspace"]

    for cmd in [run_cmd, report_html, report_lcov, report_text]:
        result = subprocess.run(cmd, cwd=cwd)
        if result.returncode != 0:
            raise SystemExit(result.returncode)

    if open:
        report = os.path.join(output_dir, "html", "index.html")
        webbrowser.open(f"file://{os.path.realpath(report)}")
