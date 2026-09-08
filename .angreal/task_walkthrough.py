"""The walkthrough: the demo, asserted.

`angreal demo up` shows that Hlin runs. This shows that it degrades the way it
promises to, which is the harder and more interesting claim, and the one a
person watching a demo cannot check for themselves.

Every step is an assertion with a sentence attached. The first one that fails
stops the run and exits non-zero, so a regression arrives named rather than as a
screenshot of something looking wrong.

Deliberately dependency-free: `urllib` and a line-by-line reader over the
server-sent event stream, so this runs wherever angreal does.
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

import angreal

from task_demo import PLATFORMS, SHELL, _alive, _registry, _remember, _start, _stop, demo

cwd = os.path.join(angreal.get_root(), "..")

#: The platform whose panels are killed and revived. The other one is the
#: control: its panels must go on working throughout, or "one platform
#: misbehaving" is indistinguishable from "the shell fell over".
SUBJECT = "stampmill"
CONTROL = "orebank"

#: The panel the breaking manifest drops.
DROPPED = "queue-depth"


class Failed(Exception):
    """An assertion that did not hold, with what should have happened."""


def say(text=""):
    """Write progress where it will still be seen when this fails.

    angreal shows a task's stdout when the task succeeds and swallows it when
    it does not, which is exactly backwards for a script whose whole job is to
    name what went wrong. stderr comes through either way.
    """
    print(text, file=sys.stderr, flush=True)


def check(condition, said):
    if not condition:
        raise Failed(said)


def step(number, what):
    say(f"\n{number}. {what}")


# -- Talking to the shell -------------------------------------------------


def get(path, timeout=10):
    with urllib.request.urlopen(SHELL + path, timeout=timeout) as answer:
        return json.loads(answer.read().decode("utf-8"))


def send(method, path, body=None, timeout=10):
    data = json.dumps(body).encode("utf-8") if body is not None else b"{}"
    request = urllib.request.Request(
        SHELL + path, data=data, method=method, headers={"content-type": "application/json"}
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as answer:
            raw = answer.read().decode("utf-8")
            return answer.status, json.loads(raw) if raw else None
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8", "replace")
        try:
            return error.code, json.loads(raw)
        except ValueError:
            return error.code, raw


def frames(surface_id, seconds=8):
    """Read the stream for a while and return the panel frames that arrived.

    Server-sent events are line-oriented, so this needs no library: an `event:`
    line names the type and a `data:` line carries the JSON.
    """
    collected = []
    deadline = time.time() + seconds
    try:
        with urllib.request.urlopen(
            f"{SHELL}/api/stream/{surface_id}", timeout=seconds + 2
        ) as stream:
            event = None
            for raw in stream:
                line = raw.decode("utf-8").rstrip("\n")
                if line.startswith("event:"):
                    event = line[6:].strip()
                elif line.startswith("data:") and event == "panel":
                    try:
                        collected.append(json.loads(line[5:].strip()))
                    except ValueError:
                        pass
                if time.time() > deadline:
                    break
    except (urllib.error.URLError, OSError, TimeoutError):
        pass
    return collected


def latest(collected):
    """The last frame seen for each panel instance."""
    newest = {}
    for frame in collected:
        newest[frame["instance"]] = frame
    return newest


def wait_for_panels(surface_id, wanted, seconds=90):
    """Poll the stream until every named panel is in the state asked of it.

    `wanted` maps an instance id to a predicate. Bounded and polled rather than
    slept through, because the shell's own timings — a ten second poll, a
    two-observation debounce, a thirty second backoff — mean the right answer
    can be half a minute away, and a fixed sleep would be either flaky or slow.
    """
    deadline = time.time() + seconds
    seen = {}
    while time.time() < deadline:
        seen = latest(frames(surface_id, seconds=6))
        if all(
            instance in seen and predicate(seen[instance])
            for instance, predicate in wanted.items()
        ):
            return seen
    return seen


def shell_log():
    entry = _registry().get("shell")
    if not entry:
        return ""
    try:
        with open(entry["log"]) as handle:
            return handle.read()
    except OSError:
        return ""


def wait_for_log(needle, seconds=90):
    deadline = time.time() + seconds
    while time.time() < deadline:
        if needle in shell_log():
            return True
        time.sleep(2)
    return False


# -- Managing the subject platform ----------------------------------------


def stop_subject():
    processes = _registry()
    entry = processes.get(SUBJECT)
    if entry:
        _stop(SUBJECT, entry)
        processes.pop(SUBJECT, None)
        _remember(processes)


def start_subject(breaking=False):
    port = next(p["port"] for p in PLATFORMS if p["name"] == SUBJECT)
    argv = [
        "./target/debug/hlin-sample-platform",
        "--name",
        SUBJECT,
        "--port",
        str(port),
        "--auth",
        "token",
        "--shell-keys",
        f"{SHELL}/.well-known/hlin-keys.json",
    ]
    if breaking:
        argv.append("--breaking")

    _start(SUBJECT, argv, port)

    deadline = time.time() + 30
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(
                f"http://127.0.0.1:{port}/.well-known/hlin.json", timeout=2
            ) as answer:
                if answer.status == 200:
                    return True
        except Exception:
            pass
        time.sleep(0.5)
    return False


# -- The walkthrough ------------------------------------------------------


@demo()
@angreal.command(
    name="walkthrough",
    about="drive the running demo and assert what should happen",
    tool=angreal.ToolDescription(
        """
        Compose a surface from both platforms, then kill one, break its
        contract, and put it back, asserting the shell's behaviour at every
        step. Exits non-zero on the first assertion that fails.

        ## When to use
        - After `angreal demo up`
        - To check a change did not break how Hlin degrades

        Stops and restarts one sample platform, and leaves it running normally
        at the end. It touches nothing outside the demo.
        """,
        risk_level="safe",
    ),
)
def walkthrough():
    if not _registry().get("shell") or not _alive(_registry()["shell"]["pid"]):
        say("The demo is not running. Start it with `angreal demo up`.")
        return 1

    try:
        _walk()
    except Failed as failure:
        say(f"\nFAILED: {failure}")
        return 1
    except (urllib.error.URLError, OSError, TimeoutError) as error:
        say(f"\nFAILED: could not reach the shell: {error}")
        return 1
    finally:
        # However this went, leave the demo the way it was found.
        stop_subject()
        start_subject(breaking=False)

    say("\nEverything held.")
    return 0


def _walk():
    step(1, "both platforms are discovered, with their panels accepted")

    platforms = get("/api/platforms")
    by_id = {platform["id"]: platform for platform in platforms}

    for name in (CONTROL, SUBJECT):
        check(name in by_id, f"{name} should be listed by /api/platforms")
        check(
            by_id[name]["reachable"],
            f"{name} should be reachable; the shell is polling it every ten seconds",
        )
        check(
            by_id[name]["panels"],
            f"{name} should offer panels; an empty list means its manifest was rejected",
        )
        check(
            not by_id[name]["rejected"],
            f"{name} should have no rejected panels, and has {by_id[name]['rejected']}",
        )

    say(
        f"   {CONTROL}: {len(by_id[CONTROL]['panels'])} panels, "
        f"{SUBJECT}: {len(by_id[SUBJECT]['panels'])} panels"
    )

    step(2, "a surface is composed from both platforms")

    # A layout of its own, never the principal's home one.
    #
    # This used to take whatever `/api/layouts/home` returned and write its two
    # panels over it. That is the most recently touched layout, which after
    # `angreal demo compose` is the surface somebody was just told to open — so
    # running the walkthrough silently replaced a nine-panel demo with a
    # two-panel one, and the person who opened the link saw a working shell
    # showing almost nothing. `demo compose` already creates its own; this now
    # does the same.
    status, layout = send("POST", "/api/layouts", {"title": "Walkthrough"})
    check(status in (200, 201), f"creating a layout should succeed, and answered {status}")
    layout_id = layout["id"]

    control_panel = "records-per-second"
    check(
        any(panel["key"] == DROPPED for panel in by_id[SUBJECT]["panels"]),
        f"{SUBJECT} should offer {DROPPED}, which the rest of this depends on",
    )

    layout["title"] = "Walkthrough"
    layout["panels"] = [
        {
            "platform_id": CONTROL,
            "panel_key": control_panel,
            "position": {"x": 0, "y": 0, "w": 6, "h": 3},
        },
        {
            "platform_id": SUBJECT,
            "panel_key": DROPPED,
            "position": {"x": 6, "y": 0, "w": 6, "h": 3},
        },
    ]

    status, written = send("PUT", f"/api/layouts/{layout_id}", layout)
    check(status == 200, f"writing the layout should succeed, and answered {status}")
    check(len(written["panels"]) == 2, "both panels should have been stored")

    control_id = written["panels"][0]["id"]
    subject_id = written["panels"][1]["id"]
    check(
        control_id and subject_id,
        "the shell should have given each panel an identity, since the browser sent none",
    )
    say(f"   layout {layout_id}, panels from {CONTROL} and {SUBJECT}")

    step(3, "the stream delivers ready frames for panels from both platforms")

    ready = lambda frame: frame["state"] == "ready"  # noqa: E731
    seen = wait_for_panels(layout_id, {control_id: ready, subject_id: ready}, seconds=90)

    check(
        seen.get(control_id, {}).get("state") == "ready",
        f"{CONTROL}'s panel should be ready, and is {seen.get(control_id, {}).get('state')}",
    )
    check(
        seen.get(subject_id, {}).get("state") == "ready",
        f"{SUBJECT}'s panel should be ready, and is {seen.get(subject_id, {}).get('state')}",
    )
    check(
        seen[control_id].get("envelope"),
        "a ready panel should carry its envelope, or there is nothing to draw",
    )
    say("   both ready, both carrying data")

    step(4, "changing the time range advances the generation")

    before = seen[control_id]["generation"]
    now = time.time()
    status, _ = send(
        "POST",
        f"/api/stream/{layout_id}/params",
        {
            "protocol_version": 1,
            "generation": before + 1,
            "time_range": {
                "from": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now - 21600)),
                "to": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now)),
            },
            "selections": {},
        },
    )
    check(status == 202, f"a parameter change should be accepted, and answered {status}")

    moved = wait_for_panels(
        layout_id,
        {control_id: lambda frame: frame["generation"] > before and frame["state"] == "ready"},
        seconds=60,
    )
    check(
        moved[control_id]["generation"] > before,
        f"frames should answer the new generation, and are still at {before}",
    )
    say(f"   generation {before} -> {moved[control_id]['generation']}, new frames arrived")

    step(5, f"stopping {SUBJECT} makes its panels unreachable, and only its panels")

    stop_subject()

    gone = wait_for_panels(
        layout_id,
        {
            subject_id: lambda frame: frame["state"] == "unavailable"
            and frame.get("cause") == "unreachable",
            control_id: lambda frame: frame["state"] in ("ready", "stale"),
        },
        seconds=120,
    )
    check(
        gone.get(subject_id, {}).get("state") == "unavailable",
        f"{SUBJECT}'s panel should be unavailable, and is "
        f"{gone.get(subject_id, {}).get('state')}",
    )
    check(
        gone[subject_id].get("cause") == "unreachable",
        f"the cause should be unreachable rather than {gone[subject_id].get('cause')}; "
        "a platform that is not answering is not a platform sending nonsense",
    )
    check(
        gone.get(control_id, {}).get("state") in ("ready", "stale"),
        f"{CONTROL}'s panel should be unaffected, and is "
        f"{gone.get(control_id, {}).get('state')}; one platform failing must not "
        "take the surface with it",
    )
    say(f"   {SUBJECT} unreachable, {CONTROL} {gone[control_id]['state']}")

    step(6, f"{SUBJECT} comes back having dropped a panel without a major bump")

    check(start_subject(breaking=True), f"{SUBJECT} should restart with --breaking")

    check(
        wait_for_log("contract violation", seconds=120),
        "the shell log should carry a violation; the shell is the only thing "
        "enforcing this contract, so a silent shell means nobody is",
    )

    log = shell_log()
    violation = [line for line in log.splitlines() if "contract violation" in line]
    check(violation, "there should be a violation line to read")
    named = violation[-1]
    check(
        DROPPED in named,
        f"the violation should name the removed panel {DROPPED}, and says: {named}",
    )
    say(f"   {named.strip()[-160:]}")

    # And in a place an operator can still find it tomorrow. A log line is a
    # signal only for somebody who happens to be watching when it fires; the
    # question the whole mechanism exists to answer is whether this platform
    # has ever done this, and that needs an answer that outlives the log.
    platforms = get("/api/platforms")
    subject = next((p for p in platforms if p["id"] == SUBJECT), None)
    check(subject is not None, f"{SUBJECT} should be listed by /api/platforms")

    recorded = (subject or {}).get("last_violation")
    check(
        recorded,
        "the violation should be readable from /api/platforms, not only from the log",
    )
    if recorded:
        check(
            any(DROPPED in change for change in recorded.get("changes", [])),
            f"the recorded violation should name {DROPPED}: {recorded.get('changes')}",
        )
        say(f"   recorded: declared {recorded['declared']}, seen {recorded['seen']} time(s)")

    step(7, "the removed panel says it is gone, rather than saying nothing")

    withdrawn = wait_for_panels(
        layout_id,
        {
            subject_id: lambda frame: frame["state"] == "unavailable"
            and frame.get("cause") == "unknown"
        },
        seconds=120,
    )
    check(
        withdrawn.get(subject_id, {}).get("cause") == "unknown",
        f"the withdrawn panel's cause should be unknown, and is "
        f"{withdrawn.get(subject_id, {}).get('cause')}; a panel that was taken away "
        "is not the same as one that is broken",
    )
    check(
        withdrawn[subject_id].get("detail"),
        "and it should carry a sentence a viewer can read",
    )
    say(f"   \"{withdrawn[subject_id]['detail']}\"")

    step(8, f"{SUBJECT} restarts normally and the panel comes back")

    stop_subject()
    check(start_subject(breaking=False), f"{SUBJECT} should restart")

    restored = wait_for_panels(
        layout_id, {subject_id: lambda frame: frame["state"] == "ready"}, seconds=180
    )
    check(
        restored.get(subject_id, {}).get("state") == "ready",
        f"the panel should be ready again, and is "
        f"{restored.get(subject_id, {}).get('state')}; a shell that noticed a panel "
        "leaving and not it returning would need restarting after every deploy",
    )
    say("   back to ready, with no restart of the shell")
