import angreal
import subprocess
import os

cwd = os.path.join(angreal.get_root(), '..')
check = angreal.command_group(name="check", about="commands for code quality checks")


@check()
@angreal.command(name="fmt", about="check code formatting")
def fmt_check():
    result = subprocess.run(["cargo", "fmt", "--check"], cwd=cwd)
    raise SystemExit(result.returncode)


@check()
@angreal.command(name="clippy", about="run clippy lints")
def clippy_check():
    result = subprocess.run(
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        cwd=cwd,
    )
    raise SystemExit(result.returncode)


@check()
@angreal.command(name="all", about="run all checks (fmt + clippy)")
def all_checks():
    for cmd in [
        ["cargo", "fmt", "--check"],
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "check", "--workspace"],
    ]:
        result = subprocess.run(cmd, cwd=cwd)
        if result.returncode != 0:
            raise SystemExit(result.returncode)
