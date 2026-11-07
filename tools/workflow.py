
"""Cooperative workflow helpers, using only the Python 3.11+ standard library.

run --timeout SECONDS -- COMMAND... uses one deadline for waiting and execution.
The exclusive target/workflow-heavy.lock coordinates cargo, ffmpeg and Haxe;
xtask verify owns target/verify-validation.lock and must invoke gates directly,
not recursively through this wrapper. Inherited markers reject nested run calls.
Locks left by a crash/SIGKILL are NEVER automatically removed: inspect the owner
before manual recovery. Only the lock instance acquired here is released.
Timeout exits 124; cleanup may take an additional bounded grace period. Command
exit codes are preserved (POSIX signal deaths are reported as 128 + signal).

On POSIX each command gets a new process group. Before cleanup signals its
leader, a bounded ps ancestry snapshot also collects invocation-owned descendant
groups (including gates using setpgid). Those groups receive TERM, then KILL.
If ps is absent/fails, only the original command group is cleaned and a warning
is reported. This is best effort, not containment: snapshots race process/group
changes; already-reparented, detached or escaped descendants may be missed.
Unverified/shared descendant groups are skipped, not chased after reparenting.
Windows uses a new process group and bounded taskkill /T /F, with direct-child
kill as fallback; without Job Objects, detached children or descendants of an
already-exited parent may survive. Neither is an OS sandbox. Commands are argv,
never evaluated by a shell by this helper.

Snapshots hash tracked files (including ignored tracked files) and nonignored
untracked files, including symlink text, not its destination. HEAD changes make
a baseline stale; moving to another root or changing its format is incompatible.
There is no arbitrary wall-clock expiry. Store baselines under ignored target/
to avoid counting the baseline itself as a new source file. Scans are not atomic:
quiesce writers while capturing/checking. Scope reports aggregate content delta;
parallel batches must pass the UNION of assigned write sets. It cannot attribute
concurrent writers, prevent writes, or establish security isolation. Ignored files,
file mode-only changes and edits subsequently reverted are outside that report.
"""

import argparse
import contextlib
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
FORMAT = 1
NESTING_ENV = "OXIDE_WORKFLOW_HEAVY_OWNER"


class WorkflowError(Exception):
    """An actionable workflow failure."""


class LockTimeout(WorkflowError):
    """The total deadline expired while another process owned the lock."""


class Interrupted(Exception):
    def __init__(self, signum):
        self.signum = signum


def log(message):
    print("workflow: " + message, file=sys.stderr, flush=True)


class HeavyLock:
    def __init__(self, root):
        self.path = Path(root) / "target/workflow-heavy.lock"
        self.fd = None
        self.token = None

    def owner(self):
        try:
            flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
            fd = os.open(self.path, flags)
            with os.fdopen(fd, "r", encoding="utf-8") as stream:
                if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
                    return "unknown PID (lock is not a regular file)"
                data = json.loads(stream.read(8192))
            return "PID {} on {}".format(data["pid"], data.get("host", "unknown host"))
        except (OSError, ValueError, KeyError, TypeError):
            return "unknown PID (unreadable/incomplete lock; inspect manually)"

    def acquire(self, deadline, command):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        announced = False
        while True:
            if time.monotonic() >= deadline:
                raise LockTimeout("timeout waiting for {}: owner {}; no stale lock removed".format(
                    self.path, self.owner()))
            try:
                self.fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            except FileExistsError:
                if not announced:
                    log("waiting for {} (owner {})".format(self.path, self.owner()))
                    announced = True
                time.sleep(min(0.05, max(0, deadline - time.monotonic())))
                continue
            try:
                data = {"pid": os.getpid(), "host": socket.gethostname(),
                        "started": time.time(), "token": uuid.uuid4().hex, "argv": command}
                with os.fdopen(os.dup(self.fd), "w", encoding="utf-8") as stream:
                    json.dump(data, stream)
                    stream.write("\n")
                self.token = data["token"]
            except BaseException:
                self.release()
                raise
            return

    def release(self):
        if self.fd is None:
            return
        try:
            held = os.fstat(self.fd)
            windows = os.name == "nt"
            if windows:
                # Windows CRT handles cannot be unlinked while open. Recheck both
                # identity and token after closing; this remains a cooperative lock.
                os.close(self.fd)
                self.fd = None
            try:
                current = self.path.lstat()
                owned = (held.st_dev, held.st_ino) == (current.st_dev, current.st_ino)
                if owned and windows and self.token is not None:
                    try:
                        record = json.loads(self.path.read_text(encoding="utf-8"))
                        owned = isinstance(record, dict) and record.get("token") == self.token
                    except (OSError, ValueError):
                        owned = False
                if owned:
                    self.path.unlink()
                else:
                    log("lock was replaced; leaving replacement untouched: " + str(self.path))
            except FileNotFoundError:
                pass
        finally:
            if self.fd is not None:
                os.close(self.fd)
                self.fd = None


@contextlib.contextmanager
def interrupt_handlers():
    previous = {}

    def interrupt(signum, _frame):
        raise Interrupted(signum)

    try:
        for name in ("SIGINT", "SIGTERM", "SIGHUP"):
            signum = getattr(signal, name, None)
            if signum is not None:
                previous[signum] = signal.signal(signum, interrupt)
        yield
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def invocation_groups(process):
    """Snapshot live ancestry before TERM can reparent separate-group gates."""
    leader = process.pid
    wrapper = os.getpid()
    protected = {0, 1, wrapper, os.getpgrp()}
    if leader in protected or leader < 0:
        log("refusing unsafe POSIX command group {}".format(leader))
        return set()
    groups = {leader}  # Popen(start_new_session=True) established this group.
    try:
        result = subprocess.run(["ps", "-axo", "pid=,ppid=,pgid="], shell=False,
                                timeout=1, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                text=True, check=False)
        if result.returncode:
            raise ValueError("ps exited {}: {}".format(result.returncode, result.stderr.strip()[:200]))
        table = {}
        children = {}
        members = {}
        for line in result.stdout.splitlines():
            if not line.strip():
                continue
            pid, parent, group = map(int, line.split())
            if pid <= 0 or parent < 0 or group < 0 or pid in table:
                raise ValueError("invalid or duplicate ps process row")
            table[pid] = (parent, group)
            children.setdefault(parent, []).append(pid)
            members.setdefault(group, set()).add(pid)
        if not table:
            raise ValueError("empty ps snapshot")
    except (OSError, ValueError, subprocess.TimeoutExpired) as exc:
        log("POSIX descendant-group collection failed: {}; cleaning only original command group {}; "
            "separate/reparented descendants may survive".format(exc, leader))
        return groups
    if leader not in table:
        # An already-exited leader cannot establish ancestry for separate groups.
        return groups
    if process.returncode is not None or table[leader] != (wrapper, leader):
        log("POSIX invocation leader identity no longer matches; refusing unverified groups")
        return set()
    descendants = set()
    pending = [leader]
    while pending:
        pid = pending.pop()
        if pid not in descendants:
            descendants.add(pid)
            pending.extend(children.get(pid, ()))
    for group in {table[pid][1] for pid in descendants} - {leader}:
        if group in protected:
            continue
        # Require an invocation-owned group leader and no observed outside members.
        # Do not infer ownership from a shared PGID or adopt unrelated orphans.
        if group in descendants and members[group] <= descendants:
            groups.add(group)
        else:
            log("skipping POSIX descendant group {}: exclusive invocation ownership unverified".format(group))
    return groups


def signal_group(group, signum):
    if group <= 1 or group in (os.getpid(), os.getpgrp()):
        log("refusing unsafe POSIX group {}".format(group))
        return False
    try:
        os.killpg(group, signum)
        return True
    except ProcessLookupError:
        return False
    except OSError as exc:
        log("could not signal POSIX group {} with {}: {}".format(group, signum, exc))
        return False


def terminate_tree(process):
    """Bounded best-effort cleanup; never refresh ancestry after killing parents."""
    if os.name == "posix":
        groups = sorted(invocation_groups(process), key=lambda group: (group == process.pid, group))
        active = {group for group in groups if signal_group(group, signal.SIGTERM)}
        until = time.monotonic() + 0.5
        while active and time.monotonic() < until:
            process.poll()
            active = {group for group in active if signal_group(group, 0)}
            if active:
                time.sleep(0.02)
        for group in groups:
            if group in active:
                signal_group(group, signal.SIGKILL)
    elif process.poll() is None:
        try:
            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                           shell=False, timeout=5, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL, check=False)
        except (OSError, subprocess.TimeoutExpired):
            pass
        if process.poll() is None:
            process.kill()
    process.wait(timeout=5)


def run_job(root, command, timeout):
    if not math.isfinite(timeout) or timeout <= 0:
        raise WorkflowError("--timeout must be finite and greater than zero")
    if not command:
        raise WorkflowError("run requires -- COMMAND...")
    if os.environ.get(NESTING_ENV):
        raise WorkflowError("nested workflow run rejected (owner {})".format(os.environ[NESTING_ENV]))
    lock = HeavyLock(root)
    process = None
    deadline = time.monotonic() + timeout
    with interrupt_handlers():
        try:
            lock.acquire(deadline, command)
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                log("timeout before command launch")
                return 124
            env = os.environ.copy()
            env[NESTING_ENV] = str(os.getpid())
            start_new_session: bool = os.name == "posix"
            creationflags: int = 0 if start_new_session else getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            log("lock owner PID {}; running argv {!r}".format(os.getpid(), command))
            process = subprocess.Popen(command, cwd=root, env=env, shell=False,
                                       start_new_session=start_new_session, creationflags=creationflags)
            try:
                result = process.wait(timeout=max(0, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                log("timeout after {}s (owner PID {}, command PID {}); terminating subprocess tree".format(
                    timeout, os.getpid(), process.pid))
                return 124
            if result:
                log("command failed with exit {} (PID {})".format(result, process.pid))
            return result if result >= 0 else 128 - result
        except Interrupted as exc:
            log("interrupted by signal {}; terminating own subprocess tree".format(exc.signum))
            return 128 + exc.signum
        except FileNotFoundError as exc:
            log("cannot launch command: " + str(exc))
            return 127
        finally:
            # A second interrupt must not skip tree cleanup or lock release.
            saved = {}
            for name in ("SIGINT", "SIGTERM", "SIGHUP"):
                signum = getattr(signal, name, None)
                if signum is not None:
                    saved[signum] = signal.signal(signum, signal.SIG_IGN)
            try:
                try:
                    if process is not None:
                        terminate_tree(process)
                finally:
                    lock.release()
            finally:
                for signum, handler in saved.items():
                    signal.signal(signum, handler)


def git_read(root, *args):
    command = ["git", "--no-pager", "--no-optional-locks", "-C", str(root), *args]
    try:
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                shell=False, timeout=30, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise WorkflowError("read-only git failed: " + str(exc)) from exc
    if result.returncode:
        raise WorkflowError("read-only git {} failed: {}".format(
            " ".join(args), result.stderr.decode("utf-8", errors="replace").strip()))
    return result.stdout


def revision(root):
    actual = Path(os.fsdecode(git_read(root, "rev-parse", "--show-toplevel")).strip()).resolve()
    if actual != Path(root).resolve():
        raise WorkflowError("source root must be the Git worktree root: " + str(actual))
    return git_read(root, "rev-parse", "--verify", "HEAD").decode("ascii").strip()


def source_path(value):
    if not isinstance(value, str) or not value or "\x00" in value or "\\" in value:
        raise WorkflowError("invalid repository-relative path: {!r}".format(value))
    parts = value.split("/")
    if PurePosixPath(value).is_absolute() or any(p in ("", ".", "..", ".git") for p in parts):
        raise WorkflowError("invalid repository-relative path: {!r}".format(value))
    if re.match(r"^[A-Za-z]:", value):
        raise WorkflowError("absolute paths are not allowed: " + value)
    return value


def hash_path(root, relative):
    relative = source_path(relative)
    path = Path(root)
    for component in relative.split("/")[:-1]:
        path /= component
        if path.is_symlink():
            raise WorkflowError("refusing to traverse symlink directory: " + relative)
    path = Path(root) / relative
    try:
        before = path.lstat()
    except FileNotFoundError:
        return None
    if stat.S_ISLNK(before.st_mode):
        kind = "symlink"
        digest = hashlib.sha256(os.fsencode(os.readlink(path))).hexdigest()
    elif stat.S_ISREG(before.st_mode):
        kind = "file"
        fd = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        with os.fdopen(fd, "rb") as stream:
            opened = os.fstat(stream.fileno())
            if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
                raise WorkflowError("file changed while hashing; retry: " + relative)
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
    else:
        raise WorkflowError("unsupported source path (directory/submodule/special file): " + relative)
    after = path.lstat()
    fields = ("st_dev", "st_ino", "st_mode", "st_size", "st_mtime_ns", "st_ctime_ns")
    if any(getattr(before, key) != getattr(after, key) for key in fields):
        raise WorkflowError("path changed while hashing; retry: " + relative)
    return {"kind": kind, "sha256": digest}


def collect_files(root):
    paths = git_read(root, "ls-files", "--cached", "--others", "--exclude-standard", "--full-name", "-z")
    files = {}
    for raw in sorted(set(paths.split(b"\0")) - {b""}):
        path = os.fsdecode(raw)
        entry = hash_path(root, path)
        if entry is not None:
            files[path] = entry
    return files


def capture(root):
    root = Path(root).resolve()
    head = revision(root)
    files = collect_files(root)
    if revision(root) != head:
        raise WorkflowError("Git revision changed during scan; retry with writers quiescent")
    return {"version": FORMAT, "algorithm": "sha256", "source_root": str(root),
            "revision": head, "created_ns": time.time_ns(), "files": files}


def write_snapshot(root, output):
    root = Path(root).resolve()
    output = Path(output)
    if not output.is_absolute():
        output = root / output
    if not output.resolve().is_relative_to(root) or output.is_symlink():
        raise WorkflowError("snapshot output must be inside the source root, not a symlink")
    data = capture(root)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=output.parent,
                                         prefix=".workflow-", suffix=".tmp", delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(data, stream, indent=2, sort_keys=True, ensure_ascii=True)
            stream.write("\n")
        os.replace(temporary, output)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
    print("snapshot: {} paths at {} -> {}".format(len(data["files"]), data["revision"], output))


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON key: " + key)
        result[key] = value
    return result


def load_baseline(root, filename, head):
    try:
        with open(filename, encoding="utf-8") as stream:
            data = json.load(stream, object_pairs_hook=unique_object)
    except (OSError, ValueError) as exc:
        raise WorkflowError("missing or invalid baseline {}: {}".format(filename, exc)) from exc
    required = {"version", "algorithm", "source_root", "revision", "created_ns", "files"}
    if (not isinstance(data, dict) or set(data) != required or type(data["version"]) is not int
            or data["version"] != FORMAT or data["algorithm"] != "sha256"
            or type(data["created_ns"]) is not int or data["created_ns"] <= 0
            or not isinstance(data["files"], dict)
            or not isinstance(data["revision"], str)
            or not re.fullmatch(r"[0-9a-f]{40}|[0-9a-f]{64}", data["revision"])):
        raise WorkflowError("incompatible baseline format; take a new snapshot")
    if data["source_root"] != str(Path(root).resolve()):
        raise WorkflowError("incompatible baseline source root; take a new snapshot")
    if data["revision"] != head:
        raise WorkflowError("stale baseline: Git revision changed; take a new snapshot")
    for path, entry in data["files"].items():
        source_path(path)
        if (not isinstance(entry, dict) or set(entry) != {"kind", "sha256"}
                or entry["kind"] not in ("file", "symlink")
                or not isinstance(entry["sha256"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"])):
            raise WorkflowError("incompatible baseline hash entry: " + path)
    return data


def content_delta(before, after):
    return {path: ("added" if path not in before else "deleted" if path not in after else "modified")
            for path in sorted(before.keys() | after.keys()) if before.get(path) != after.get(path)}


def allowed_paths(root, values, before):
    allowed = []
    for value in values:
        directory = value.endswith("/")
        path = source_path(value.rstrip("/") if directory else value)
        if any(c in path for c in "*?["):
            raise WorkflowError("--allow takes literal files/directories, not globs: " + value)
        candidate = Path(root) / path
        directory = directory or any(p.startswith(path + "/") for p in before)
        if not directory and path not in before:
            directory = candidate.is_dir() and not candidate.is_symlink()
        allowed.append((path, directory))
    return allowed


def path_allowed(path, allowed):
    return any(path == prefix or (directory and path.startswith(prefix + "/"))
               for prefix, directory in allowed)


def check_scope(root, baseline, values):
    head = revision(root)
    before = load_baseline(root, baseline, head)
    allowed = allowed_paths(root, values, before["files"])
    after = collect_files(root)
    if revision(root) != head:
        raise WorkflowError("Git revision changed during scope scan; take a new snapshot")
    changes = content_delta(before["files"], after)
    violations = [p for p in changes if not path_allowed(p, allowed)]
    for path, kind in changes.items():
        print("{} {} {}".format("VIOLATION" if path in violations else "allowed", kind, repr(path)))
    print("scope: {} changed paths, {} violations (aggregate delta; not writer attribution or isolation)".format(
        len(changes), len(violations)))
    return 1 if violations else 0


def main(argv=None, root=None):
    root = Path(root or ROOT).resolve()
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="action", required=True)
    run = commands.add_parser("run", help="serialize a heavy argv under a total wait/execution deadline")
    run.add_argument("--timeout", required=True, type=float)
    run.add_argument("command", nargs=argparse.REMAINDER)
    snapshot = commands.add_parser("snapshot", help="hash tracked and nonignored untracked content")
    snapshot.add_argument("--out", required=True)
    scope = commands.add_parser("scope", help="check aggregate delta against a union of literal write sets")
    scope.add_argument("--baseline", required=True)
    scope.add_argument("--allow", action="append", required=True)
    args = parser.parse_args(argv)
    try:
        if args.action == "run":
            if not args.command or args.command[0] != "--":
                raise WorkflowError("run requires -- before COMMAND")
            return run_job(root, args.command[1:], args.timeout)
        if args.action == "snapshot":
            write_snapshot(root, args.out)
            return 0
        baseline = Path(args.baseline)
        if not baseline.is_absolute():
            baseline = root / baseline
        return check_scope(root, baseline, args.allow)
    except LockTimeout as exc:
        log(str(exc))
        return 124
    except (WorkflowError, OSError, subprocess.SubprocessError) as exc:
        log(str(exc))
        return 2


if __name__ == "__main__":
    sys.exit(main())
