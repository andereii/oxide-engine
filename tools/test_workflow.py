
"""Run with: python3 -B -m unittest discover -s tools -p test_workflow.py -v

All writes are inside temporary fixtures. Git inventory/revision reads are mocked;
no Git initialization, index updates, network, models or Cargo are used. The lint
fixtures intentionally do not depend on concurrently edited parent-owned files.
"""

import contextlib
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

if __package__:
    from . import workflow, workflow_lint
else:
    import workflow
    import workflow_lint

TOOLS = Path(__file__).resolve().parent
HEAD = "a" * 40
REAL_GIT_READ = workflow.git_read


class TemporaryFixture(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="oxide-workflow-test-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.root = self.base / "repo"
        self.root.mkdir()

    def write(self, path, text):
        destination = self.root / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")
        return destination


class RunTests(TemporaryFixture):
    def wrapper(self, *args):
        code = ("import sys; from pathlib import Path; sys.path.insert(0, {!r}); "
                "import workflow; raise SystemExit(workflow.main(sys.argv[1:], root=Path({!r})))").format(
                    str(TOOLS), str(self.root))
        return [sys.executable, "-B", "-c", code, *args]

    def environment(self):
        env = os.environ.copy()
        env.pop(workflow.NESTING_ENV, None)
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        return env

    def invoke(self, command, seconds: str | int | float = 3):
        return subprocess.run(self.wrapper("run", "--timeout", str(seconds), "--", *command),
                              cwd=self.root, env=self.environment(), capture_output=True,
                              text=True, timeout=8, shell=False)

    def python(self, code, *args):
        return [sys.executable, "-B", "-c", code, *args]

    def wait_for(self, path, process):
        until = time.monotonic() + 3
        while not path.exists() and time.monotonic() < until:
            if process.poll() is not None:
                self.fail("wrapper exited before fixture was ready: " + str(process.communicate()))
            time.sleep(0.01)
        self.assertTrue(path.exists(), "fixture did not become ready")

    def spawn(self, command, seconds=4):
        process = subprocess.Popen(self.wrapper("run", "--timeout", str(seconds), "--", *command),
                                   cwd=self.root, env=self.environment(), stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, shell=False)

        def cleanup():
            if process.poll() is None:
                process.terminate()
            try:
                process.communicate(timeout=7)
            except subprocess.TimeoutExpired:
                process.kill()
                process.communicate(timeout=2)
        self.addCleanup(cleanup)
        return process

    def test_lock_contention_times_out_preserves_owner_then_releases(self):
        lock = workflow.HeavyLock(self.root)
        self.addCleanup(lock.release)
        lock.acquire(time.monotonic() + 1, ["fixture-owner"])
        original = lock.path.read_bytes()
        result = self.invoke(self.python("raise SystemExit(0)"), seconds=0.15)
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertIn("timeout waiting", result.stderr)
        self.assertIn("PID " + str(os.getpid()), result.stderr)
        self.assertEqual(lock.path.read_bytes(), original)
        lock.release()
        result = self.invoke(self.python("raise SystemExit(0)"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(lock.path.exists())

    def test_live_wrappers_wait_and_serialize(self):
        ready = self.root / "ready"
        first = self.spawn(self.python(
            "from pathlib import Path; import time; Path('ready').write_text('ready'); time.sleep(0.4)"))
        self.wait_for(ready, first)
        result = self.invoke(self.python("print('second job')"))
        stdout, stderr = first.communicate(timeout=4)
        self.assertEqual(first.returncode, 0, stdout + stderr)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("waiting for", result.stderr)
        self.assertIn("second job", result.stdout)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    def test_stale_or_malformed_lock_is_never_deleted(self):
        lockpath = self.write("target/workflow-heavy.lock", '{"pid": 99999999, "host": "fixture"}')
        for text in (lockpath.read_text(), "not valid JSON"):
            lockpath.write_text(text)
            result = self.invoke(self.python("raise SystemExit(0)"), seconds=0.08)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("no stale lock removed", result.stderr)
            self.assertEqual(lockpath.read_text(), text)

    def test_release_does_not_delete_replacement_lock(self):
        lock = workflow.HeavyLock(self.root)
        self.addCleanup(lock.release)
        lock.acquire(time.monotonic() + 1, ["owner"])
        lock.path.unlink()
        lock.path.write_text("replacement")
        with contextlib.redirect_stderr(io.StringIO()):
            lock.release()
        self.assertEqual(lock.path.read_text(), "replacement")

    def test_windows_release_closes_descriptor_before_unlink(self):
        lock = workflow.HeavyLock(self.root)
        self.addCleanup(lock.release)
        lock.acquire(time.monotonic() + 1, ["owner"])
        assert lock.fd is not None
        descriptor = lock.fd
        original_unlink = Path.unlink

        def unlink_after_close(path, *args, **kwargs):
            with self.assertRaises(OSError):
                os.fstat(descriptor)
            return original_unlink(path, *args, **kwargs)

        with mock.patch.object(workflow.os, "name", "nt"), mock.patch.object(
                Path, "unlink", autospec=True, side_effect=unlink_after_close):
            lock.release()
        self.assertFalse(lock.path.exists())

    def test_windows_release_keeps_changed_token(self):
        lock = workflow.HeavyLock(self.root)
        self.addCleanup(lock.release)
        lock.acquire(time.monotonic() + 1, ["owner"])
        data = json.loads(lock.path.read_text())
        data["token"] = "different-owner"
        lock.path.write_text(json.dumps(data))
        with mock.patch.object(workflow.os, "name", "nt"), contextlib.redirect_stderr(io.StringIO()):
            lock.release()
        self.assertTrue(lock.path.exists())

    def test_argv_is_literal_not_shell_evaluated(self):
        arguments = ["two words", "$(touch injected)", "; touch injected", "$HOME", "*", '"quotes"', "--flag"]
        result = self.invoke(self.python("import json,sys; print(json.dumps(sys.argv[1:]))", *arguments))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), arguments)
        self.assertFalse((self.root / "injected").exists())

    def test_failure_and_launch_error_release_lock(self):
        result = self.invoke(self.python("raise SystemExit(17)"))
        self.assertEqual(result.returncode, 17)
        self.assertIn("failed with exit 17", result.stderr)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())
        result = self.invoke([str(self.root / "does-not-exist")])
        self.assertEqual(result.returncode, 127)
        self.assertIn("cannot launch", result.stderr)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    def test_execution_timeout_is_nonzero_and_releases_lock(self):
        result = self.invoke(self.python("import time; time.sleep(30)"), seconds=0.15)
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertIn("terminating subprocess tree", result.stderr)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    def test_nested_wrapper_rejected_without_deadlock(self):
        nested = self.wrapper("run", "--timeout", "1", "--", *self.python("raise SystemExit(0)"))
        result = self.invoke(nested)
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("nested workflow run rejected", result.stderr)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    def test_invalid_timeout_or_missing_separator_never_launches(self):
        for timeout in ("0", "-1", "nan", "inf"):
            result = self.invoke(self.python("raise SystemExit(0)"), seconds=timeout)
            self.assertEqual(result.returncode, 2, result.stderr)
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(workflow.main(["run", "--timeout", "1", "echo"], root=self.root), 2)
            self.assertEqual(workflow.main(["run", "--timeout", "1", "--"], root=self.root), 2)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    @unittest.skipUnless(os.name == "posix", "POSIX signal/process-group behavior")
    def test_sigterm_cleans_up_and_releases(self):
        process = self.spawn(self.python(
            "from pathlib import Path; import time; Path('ready').write_text('ready'); time.sleep(30)"))
        self.wait_for(self.root / "ready", process)
        process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=4)
        self.assertEqual(process.returncode, 128 + signal.SIGTERM, stdout + stderr)
        self.assertIn("interrupted", stderr)
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    @unittest.skipUnless(os.name == "posix", "POSIX descendant cleanup")
    def test_timeout_kills_term_ignoring_grandchild_after_leader_exits(self):
        descendant = ("import signal,time; from pathlib import Path; "
                      "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
                      "Path('grandchild-ready').write_text('ready'); "
                      "time.sleep(1.3); Path('survived').write_text('bad')")
        leader = "import subprocess,sys,time; subprocess.Popen([sys.executable, '-B', '-c', {!r}]); time.sleep(30)".format(descendant)
        result = self.invoke(self.python(leader), seconds=0.4)
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertTrue((self.root / "grandchild-ready").exists())
        time.sleep(0.6)
        self.assertFalse((self.root / "survived").exists())
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    def check_separate_group_writer_cleanup(self, interrupt):
        writer = """import json, os, signal, time
from pathlib import Path
signal.signal(signal.SIGTERM, signal.SIG_IGN)
Path('writer-ready').write_text(json.dumps({'pid': os.getpid(), 'pgid': os.getpgrp(), 'sid': os.getsid(0)}))
end = time.monotonic() + 5
with open('writer-ticks', 'ab', buffering=0) as stream:
    while time.monotonic() < end and not Path('writer-stop').exists():
        stream.write(b'tick\\n')
        time.sleep(0.02)
"""
        leader = ("import json,os,subprocess,sys,time; from pathlib import Path; "
                  "subprocess.Popen([sys.executable, '-B', '-c', {!r}], process_group=0, "
                  "stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL); "
                  "Path('leader-ready').write_text(json.dumps({{'pid': os.getpid(), 'sid': os.getsid(0)}})); "
                  "time.sleep(30)").format(writer)

        def stop_writer_if_cleanup_failed():
            (self.root / "writer-stop").touch()
            time.sleep(0.06)
        self.addCleanup(stop_writer_if_cleanup_failed)
        process = self.spawn(self.python(leader), seconds=1)
        self.wait_for(self.root / "writer-ready", process)
        self.wait_for(self.root / "writer-ticks", process)
        self.wait_for(self.root / "leader-ready", process)
        child = json.loads((self.root / "writer-ready").read_text())
        parent = json.loads((self.root / "leader-ready").read_text())
        self.assertEqual(child["pid"], child["pgid"])
        self.assertNotEqual(child["pgid"], parent["pid"])
        self.assertEqual(child["sid"], parent["sid"], "fixture must not detach its session")
        ticks = self.root / "writer-ticks"
        initial_size = ticks.stat().st_size
        time.sleep(0.08)
        self.assertGreater(ticks.stat().st_size, initial_size, "writer should be live before cleanup")
        if interrupt:
            process.send_signal(signal.SIGTERM)
        stdout, stderr = process.communicate(timeout=5)
        self.assertEqual(process.returncode, 128 + signal.SIGTERM if interrupt else 124, stdout + stderr)
        final_size = ticks.stat().st_size
        time.sleep(0.15)
        self.assertEqual(ticks.stat().st_size, final_size, "separate-group descendant writer survived cleanup")
        self.assertFalse((self.root / "target/workflow-heavy.lock").exists())

    @unittest.skipUnless(os.name == "posix" and shutil.which("ps"), "requires POSIX ps ancestry snapshot")
    def test_timeout_stops_live_descendant_writer_in_separate_group(self):
        self.check_separate_group_writer_cleanup(interrupt=False)

    @unittest.skipUnless(os.name == "posix" and shutil.which("ps"), "requires POSIX ps ancestry snapshot")
    def test_sigterm_stops_live_descendant_writer_in_separate_group(self):
        self.check_separate_group_writer_cleanup(interrupt=True)

    def test_windows_cleanup_uses_taskkill_and_fallback(self):
        process = mock.Mock(pid=4321)
        process.poll.return_value = None
        with mock.patch.object(workflow.os, "name", "nt"), mock.patch.object(
                workflow.subprocess, "run", side_effect=FileNotFoundError) as run:
            workflow.terminate_tree(process)
        self.assertEqual(run.call_args.args[0], ["taskkill", "/PID", "4321", "/T", "/F"])
        self.assertFalse(run.call_args.kwargs["shell"])
        process.kill.assert_called_once()
        process.wait.assert_called_once_with(timeout=5)


@unittest.skipUnless(os.name == "posix", "POSIX process-group safety checks")
class PosixCleanupTests(unittest.TestCase):
    # Includes nested owned groups, a sibling, unrelated orphans, unsafe groups,
    # a descendant joining an outside group, and an owned leader sharing a group.
    TABLE = """900 1 900
100 900 100
101 100 101
102 101 102
103 102 101
104 100 800
105 100 900
106 100 0
107 100 107
201 900 201
300 1 300
800 1 800
801 1 107
"""

    def setUp(self):
        self.process = mock.Mock(pid=100, returncode=None)
        for name, value in (("getpid", 900), ("getpgrp", 900)):
            patch = mock.patch.object(workflow.os, name, return_value=value)
            patch.start()
            self.addCleanup(patch.stop)

    def snapshot(self, text=None):
        return subprocess.CompletedProcess([], 0, stdout=self.TABLE if text is None else text, stderr="")

    def test_collects_only_exclusively_owned_descendant_groups(self):
        with mock.patch.object(workflow.subprocess, "run", return_value=self.snapshot()) as run:
            with contextlib.redirect_stderr(io.StringIO()) as output:
                groups = workflow.invocation_groups(self.process)
        self.assertEqual(groups, {100, 101, 102})
        self.assertIn("exclusive invocation ownership unverified", output.getvalue())
        self.assertEqual(run.call_args.args[0], ["ps", "-axo", "pid=,ppid=,pgid="])
        self.assertFalse(run.call_args.kwargs["shell"])
        self.assertEqual(run.call_args.kwargs["timeout"], 1)

    def test_snapshot_precedes_signals_and_descendant_groups_outlive_leader(self):
        events = []

        def ps(*args, **kwargs):
            events.append("snapshot")
            return self.snapshot()

        def killpg(group, signum):
            events.append((group, signum))
            if group == 100:
                raise ProcessLookupError("leader group exited after snapshot")

        with mock.patch.object(workflow.subprocess, "run", side_effect=ps) as run, mock.patch.object(
                workflow.os, "killpg", side_effect=killpg), mock.patch.object(
                workflow.time, "monotonic", side_effect=[0, 1]), contextlib.redirect_stderr(io.StringIO()):
            workflow.terminate_tree(self.process)
        self.assertEqual(events, ["snapshot", (101, signal.SIGTERM), (102, signal.SIGTERM),
                                  (100, signal.SIGTERM), (101, signal.SIGKILL), (102, signal.SIGKILL)])
        run.assert_called_once()
        self.process.wait.assert_called_once_with(timeout=5)

    def test_ps_failures_warn_and_fall_back_only_to_original_group(self):
        failures = [FileNotFoundError("ps unavailable"), subprocess.TimeoutExpired(["ps"], 1),
                    subprocess.CompletedProcess([], 1, stdout="", stderr="ps failed"),
                    self.snapshot("malformed ps row"), self.snapshot("100 900 100\n100 900 100\n")]
        for failure in failures:
            with self.subTest(failure=failure), mock.patch.object(
                    workflow.subprocess, "run", side_effect=[failure]), contextlib.redirect_stderr(io.StringIO()) as output:
                self.assertEqual(workflow.invocation_groups(self.process), {100})
            self.assertIn("descendant-group collection failed", output.getvalue())
            self.assertIn("separate/reparented descendants may survive", output.getvalue())

    def test_missing_leader_does_not_adopt_reparented_or_unrelated_groups(self):
        table = "900 1 900\n101 1 101\n102 1 102\n201 900 201\n"
        with mock.patch.object(workflow.subprocess, "run", return_value=self.snapshot(table)):
            self.assertEqual(workflow.invocation_groups(self.process), {100})

    def test_mismatched_or_reused_leader_is_not_signalled(self):
        for returncode, table in ((None, "100 1 100\n101 100 101\n"), (0, self.TABLE)):
            self.process.returncode = returncode
            with self.subTest(returncode=returncode), mock.patch.object(
                    workflow.subprocess, "run", return_value=self.snapshot(table)), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(workflow.invocation_groups(self.process), set())

    def test_never_signals_zero_or_wrapper_group(self):
        with mock.patch.object(workflow.os, "killpg") as killpg, mock.patch.object(
                workflow.subprocess, "run") as ps, contextlib.redirect_stderr(io.StringIO()):
            for group in (-1, 0, 1, 900):
                self.assertFalse(workflow.signal_group(group, signal.SIGTERM))
                self.process.pid = group
                self.assertEqual(workflow.invocation_groups(self.process), set())
        killpg.assert_not_called()
        ps.assert_not_called()

    def test_signal_failures_are_reported(self):
        with mock.patch.object(workflow.os, "killpg", side_effect=PermissionError("denied")), contextlib.redirect_stderr(
                io.StringIO()) as output:
            self.assertFalse(workflow.signal_group(101, signal.SIGKILL))
        self.assertIn("could not signal POSIX group 101", output.getvalue())


class ScopeTests(TemporaryFixture):
    def setUp(self):
        super().setUp()
        self.paths = {"tracked.txt", "deleted.txt", "untracked.txt", "dir/nested.txt"}
        for path in self.paths:
            self.write(path, "before " + path)
        self.write("target/ignored", "not in Git inventory")
        self.head = HEAD
        self.git_calls = []
        patch = mock.patch.object(workflow, "git_read", side_effect=self.git)
        patch.start()
        self.addCleanup(patch.stop)
        self.baseline = self.root / "target/baseline.json"

    def git(self, root, *args):
        self.assertEqual(Path(root), self.root)
        self.git_calls.append(args)
        if args == ("rev-parse", "--show-toplevel"):
            return os.fsencode(str(self.root) + "\n")
        if args == ("rev-parse", "--verify", "HEAD"):
            return self.head.encode("ascii") + b"\n"
        self.assertEqual(args, ("ls-files", "--cached", "--others", "--exclude-standard", "--full-name", "-z"))
        return b"\0".join(os.fsencode(p) for p in sorted(self.paths)) + b"\0"

    def snapshot(self):
        with contextlib.redirect_stdout(io.StringIO()):
            workflow.write_snapshot(self.root, self.baseline)
        return json.loads(self.baseline.read_text())

    def scope(self, *allowed):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = workflow.check_scope(self.root, self.baseline, list(allowed))
        return status, output.getvalue()

    def test_snapshot_includes_untracked_and_omits_ignored(self):
        data = self.snapshot()
        self.assertEqual(set(data["files"]), self.paths)
        self.assertEqual(data["source_root"], str(self.root))
        self.assertEqual(data["revision"], HEAD)
        self.assertEqual(data["files"]["untracked.txt"]["sha256"], hashlib.sha256(
            b"before untracked.txt").hexdigest())
        self.assertEqual(self.scope("tracked.txt")[0], 0)

    def test_added_modified_deleted_and_union_write_sets(self):
        self.snapshot()
        self.write("tracked.txt", "changed")
        (self.root / "deleted.txt").unlink()
        self.write("dir/new.txt", "new untracked")
        self.paths.add("dir/new.txt")
        status, output = self.scope("tracked.txt", "deleted.txt", "dir/")
        self.assertEqual(status, 0, output)
        for kind in ("modified", "deleted", "added"):
            self.assertIn(kind, output)
        status, output = self.scope("dir/")
        self.assertEqual(status, 1)
        self.assertIn("VIOLATION modified 'tracked.txt'", output)
        self.assertIn("VIOLATION deleted 'deleted.txt'", output)
        self.assertIn("aggregate delta", output)

    def test_content_delta_helpers_include_type_changes(self):
        file = {"kind": "file", "sha256": "a" * 64}
        link = {"kind": "symlink", "sha256": "a" * 64}
        self.assertEqual(workflow.content_delta({"same": file, "gone": file, "type": file},
                                               {"same": file, "new": file, "type": link}),
                         {"gone": "deleted", "new": "added", "type": "modified"})

    def test_exact_files_and_directory_boundaries(self):
        allowed = workflow.allowed_paths(self.root, ["tracked.txt", "dir/"], {})
        for path in ("tracked.txt", "dir/a", "dir/deep/a"):
            self.assertTrue(workflow.path_allowed(path, allowed))
        for path in ("tracked.txt.bak", "tracked.txt/child", "directory/a", "Dir/a", "other/dir/a"):
            self.assertFalse(workflow.path_allowed(path, allowed), path)
        inferred = workflow.allowed_paths(self.root, ["gone"], {"gone/a": {}})
        self.assertTrue(workflow.path_allowed("gone/a", inferred))
        self.assertFalse(workflow.path_allowed("gone-other/a", inferred))

    def test_file_replaced_by_directory_does_not_widen_exact_allow(self):
        before = self.snapshot()
        (self.root / "tracked.txt").unlink()
        self.write("tracked.txt/child", "unassigned")
        self.paths.remove("tracked.txt")
        self.paths.add("tracked.txt/child")
        allowed = workflow.allowed_paths(self.root, ["tracked.txt"], before["files"])
        self.assertFalse(workflow.path_allowed("tracked.txt/child", allowed))
        self.assertEqual(self.scope("tracked.txt")[0], 1)

    def test_literal_scope_paths_reject_escape_and_globs(self):
        for value in ("../escape", "/absolute", ".", "dir/../file", "dir//file", "C:/outside",
                      "tools/*", "tools/[ab]", "file?", "dir\\file"):
            with self.subTest(value=value), self.assertRaises(workflow.WorkflowError):
                workflow.allowed_paths(self.root, [value], {})

    @unittest.skipUnless(hasattr(os, "symlink"), "requires symlinks")
    def test_symlinks_hash_link_text_not_external_contents(self):
        outside = self.base / "outside"
        outside.write_text("secret")
        link = self.root / "link"
        try:
            link.symlink_to(outside)
        except OSError as exc:
            self.skipTest("symlink creation unavailable: " + str(exc))
        expected = hashlib.sha256(os.fsencode(str(outside))).hexdigest()
        self.paths.add("link")
        data = self.snapshot()
        self.assertEqual(data["files"]["link"], {"kind": "symlink", "sha256": expected})
        outside.write_text("different secret")
        self.assertEqual(self.scope("tracked.txt")[0], 0)
        link.unlink()
        link.symlink_to("missing-destination")
        self.assertEqual(self.scope("tracked.txt")[0], 1)
        self.assertEqual(self.scope("link")[0], 0)

    @unittest.skipUnless(os.name == "posix", "POSIX symlink fixture")
    def test_symlink_ancestor_is_not_followed(self):
        outside = self.base / "outside-dir"
        outside.mkdir()
        (outside / "file").write_text("outside content")
        (self.root / "link-dir").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(workflow.WorkflowError, "symlink directory"):
            workflow.hash_path(self.root, "link-dir/file")

    def test_deleted_tracked_paths_are_absent_not_errors(self):
        (self.root / "deleted.txt").unlink()
        self.assertNotIn("deleted.txt", self.snapshot()["files"])

    def test_missing_invalid_stale_and_incompatible_baselines(self):
        with self.assertRaisesRegex(workflow.WorkflowError, "missing or invalid"):
            self.scope("tracked.txt")
        good = self.snapshot()
        for content in ("not json", '{"version":1,"version":1}'):
            self.baseline.write_text(content)
            with self.assertRaisesRegex(workflow.WorkflowError, "invalid baseline"):
                self.scope("tracked.txt")
        for key, value, message in (("version", 99, "incompatible"), ("algorithm", "md5", "incompatible"),
                                    ("source_root", str(self.base), "source root"),
                                    ("revision", "b" * 40, "stale"), ("files", {"../bad": {}}, "invalid")):
            changed = copy.deepcopy(good)
            changed[key] = value
            self.baseline.write_text(json.dumps(changed))
            with self.subTest(key=key), self.assertRaisesRegex(workflow.WorkflowError, message):
                self.scope("tracked.txt")
        self.baseline.write_text(json.dumps(good))
        self.head = "c" * 40
        with self.assertRaisesRegex(workflow.WorkflowError, "stale"):
            self.scope("tracked.txt")

    def test_invalid_hash_entry_and_revision_race_fail_closed(self):
        data = self.snapshot()
        data["files"]["tracked.txt"]["sha256"] = "not-a-hash"
        self.baseline.write_text(json.dumps(data))
        with self.assertRaisesRegex(workflow.WorkflowError, "hash entry"):
            self.scope("tracked.txt")
        with mock.patch.object(workflow, "revision", side_effect=[HEAD, "b" * 40]):
            with self.assertRaisesRegex(workflow.WorkflowError, "revision changed"):
                workflow.capture(self.root)

    def test_snapshot_output_cannot_escape_source_root(self):
        with self.assertRaisesRegex(workflow.WorkflowError, "inside the source root"):
            workflow.write_snapshot(self.root, self.base / "outside.json")

    def test_git_read_is_read_only_argv_with_required_flags(self):
        completed = subprocess.CompletedProcess([], 0, stdout=b"files\0", stderr=b"")
        with mock.patch.object(workflow.subprocess, "run", return_value=completed) as run:
            self.assertEqual(REAL_GIT_READ(self.root, "ls-files", "-z"), b"files\0")
        argv = run.call_args.args[0]
        self.assertEqual(argv, ["git", "--no-pager", "--no-optional-locks", "-C", str(self.root), "ls-files", "-z"])
        self.assertFalse(run.call_args.kwargs["shell"])
        self.assertEqual(run.call_args.kwargs["timeout"], 30)


HAS_YAML = importlib.util.find_spec("yaml") is not None


@unittest.skipUnless(HAS_YAML, "optional PyYAML unavailable; missing-dependency behavior tested separately")
class LintTests(TemporaryFixture):
    def setUp(self):
        super().setUp()
        self.write("AGENTS.md", "Read docs/GOAL.md and docs/WORKFLOW.md.\n")
        self.write("docs/GOAL.md", "Current project goals.\n")
        self.write("docs/WORKFLOW.md", "Shared policy: AGENTS.md, docs/GOAL.md.\n")
        self.write("rust-toolchain.toml", '[toolchain]\nchannel = "1.99.1"\n')
        self.config = {
            "default_agent": "oxide-orchestrator", "small_model": "opencode-go/deepseek-v4.1-flash",
            "permissions": [self.rule("read", "*", "allow")],
            "plugins": [{"package": "opencode-runtime-fallback", "options": {
                "fallback_models": ["provider/conservative-same-cost"], "max_fallback_attempts": 2,
                "notify_on_fallback": True, "timeout_seconds": 120, "cooldown_seconds": 1800}}],
        }
        self.save_config()
        self.metadata = {}
        for name, model in workflow_lint.MODELS.items():
            primary = name.startswith("oxide-orchestrator")
            rules = [self.rule(action, "*", "allow") for action in ("read", "glob", "grep")]
            if name == "oxide-reviewer":
                rules += [self.rule(action, "*", "deny") for action in (
                    "edit", "shell", "subagent", "webfetch", "websearch")]
            elif not primary:
                rules += [self.rule("edit", "*", "deny")]
                rules += [self.rule("edit", pattern, "allow") for pattern in ("crates/*", "xtask/*", "tools/*")]
                rules += [self.rule("edit", "tests/fixtures/**", "deny"), self.rule("shell", "*", "allow")]
                rules += [self.rule(action, "*", "deny") for action in ("subagent", "webfetch", "websearch")]
                rules += [self.rule("shell", pattern, "deny") for pattern in ("git*", "sudo*", "rm -rf /*")]
            self.metadata[name] = {"description": "Any wording is acceptable.", "mode": "primary" if primary else "subagent",
                                   "model": model, "steps": 80, "permissions": rules}
            body = "AGENTS.md" if name == "oxide-orchestrator" else ".opencode/agents/oxide-orchestrator.md"
            self.save_agent(name, body)
        for name in ("oxide-kickoff", "oxide-continue"):
            self.save_markdown(workflow_lint.COMMAND_DIR + name + ".md",
                               {"description": "Workflow command", "agent": "oxide-orchestrator"},
                               "Follow docs/WORKFLOW.md. $ARGUMENTS")

    @staticmethod
    def rule(action, resource, effect):
        return {"action": action, "resource": resource, "effect": effect}

    def save_config(self):
        self.write("opencode.json", json.dumps(self.config))

    def save_markdown(self, path, metadata, body):
        # JSON mappings are valid YAML; fixtures need no YAML emitter dependency.
        self.write(path, "---\n" + json.dumps(metadata, indent=2) + "\n---\n" + body + "\n")

    def save_agent(self, name, body="AGENTS.md"):
        self.save_markdown(workflow_lint.AGENT_DIR + name + ".md", self.metadata[name], body)

    def errors(self):
        return workflow_lint.lint(self.root)

    def test_complete_v2_fixture_and_static_only_output(self):
        self.assertEqual(self.errors(), [])
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            result = workflow_lint.main(["--root", str(self.root)])
        self.assertEqual(result, 0, output.getvalue())
        for message in ("STATIC LINT ONLY", "NOT been verified", "not an OS sandbox", "PASS"):
            self.assertIn(message, output.getvalue())

    def test_reviewer_global_denies_and_no_exceptions(self):
        for action in ("edit", "shell", "subagent", "webfetch", "websearch"):
            original = copy.deepcopy(self.metadata["oxide-reviewer"])
            self.metadata["oxide-reviewer"]["permissions"] = [r for r in original["permissions"] if r["action"] != action]
            self.save_agent("oxide-reviewer")
            with self.subTest(action=action):
                self.assertTrue(any("global " + action + " deny" in e for e in self.errors()))
            self.metadata["oxide-reviewer"] = original
        self.metadata["oxide-reviewer"]["permissions"].append(self.rule("shell", "git diff*", "allow"))
        self.save_agent("oxide-reviewer")
        self.assertTrue(any("must not have allow/ask exceptions" in e for e in self.errors()))

    def test_implementation_defaults_write_scopes_and_protected_fixtures(self):
        original = copy.deepcopy(self.metadata["oxide-impl-rust"])
        for mutation, expected in (("broad", "edit allows"), ("default", "default deny"),
                                   ("fixtures", "tests/fixtures/**"), ("websearch", "websearch deny"),
                                   ("git", "shell deny git*")):
            self.metadata["oxide-impl-rust"] = copy.deepcopy(original)
            rules = self.metadata["oxide-impl-rust"]["permissions"]
            if mutation == "broad":
                rules.append(self.rule("edit", "docs/*", "allow"))
            elif mutation == "default":
                rules[:] = [r for r in rules if not (r["action"] == "edit" and r["resource"] == "*")]
            elif mutation == "fixtures":
                rules[:] = [r for r in rules if r["resource"] != "tests/fixtures/**"]
            elif mutation == "websearch":
                rules[:] = [r for r in rules if r["action"] != "websearch"]
            else:
                rules[:] = [r for r in rules if r["resource"] != "git*"]
            self.save_agent("oxide-impl-rust")
            with self.subTest(mutation=mutation):
                self.assertTrue(any(expected in e for e in self.errors()), self.errors())

    def test_implementation_shell_exception_is_flagged_statically(self):
        self.metadata["oxide-impl-rust"]["permissions"].append(self.rule("shell", "git status*", "allow"))
        self.save_agent("oxide-impl-rust")
        self.assertTrue(any("exception matches denied sample" in e for e in self.errors()))

    def test_primary_models_and_toolchain_cannot_drift(self):
        self.metadata["oxide-orchestrator-deep"]["model"] = "provider/different"
        self.save_agent("oxide-orchestrator-deep")
        self.write("rust-toolchain.toml", '[toolchain]\nchannel = "stable"\n')
        errors = self.errors()
        self.assertTrue(any("model/mode" in e for e in errors))
        self.assertTrue(any("exactly 1.99.1" in e for e in errors))

    def test_fallback_identifiers_flexible_but_notify_attempts_bounded(self):
        options = self.config["plugins"][0]["options"]
        options["fallback_models"] = ["other/provider-a", "other/provider-b#max"]
        self.save_config()
        self.assertEqual(self.errors(), [])
        options["notify_on_fallback"] = False
        options["max_fallback_attempts"] = 100
        options["fallback_models"] = []
        self.save_config()
        errors = self.errors()
        for key in ("notify_on_fallback", "max_fallback_attempts", "fallback_models"):
            self.assertTrue(any(key in e for e in errors), key)

    def test_invalid_json_and_legacy_permissions_fail(self):
        self.write("opencode.json", '{"broken": true,}')
        self.assertTrue(any("invalid JSON" in e for e in self.errors()))
        self.config["permission"] = {"shell": "allow"}
        del self.config["permissions"]
        self.save_config()
        self.assertTrue(any("V2 permissions[]" in e for e in self.errors()))
        self.write("opencode.json", '{"permissions": [], "permissions": []}')
        self.assertTrue(any("duplicate JSON key" in e for e in self.errors()))

    def test_unknown_yaml_fields_invalid_types_and_globs_fail(self):
        self.metadata["oxide-reviewer"]["permission"] = {"shell": "deny"}
        self.metadata["oxide-reviewer"]["steps"] = "many"
        self.metadata["oxide-reviewer"]["permissions"].append(self.rule("read", "bad[glob", "allow"))
        self.save_agent("oxide-reviewer")
        errors = self.errors()
        for expected in ("unsupported V2", "steps must", "resource glob"):
            self.assertTrue(any(expected in e for e in errors), errors)
        self.write(workflow_lint.AGENT_DIR + "oxide-reviewer.md", "---\ndescription: first\ndescription: second\n---\nbody")
        self.assertTrue(any("duplicate" in e for e in self.errors()))

    def test_shared_reference_graph_and_required_filenames(self):
        self.write("AGENTS.md", "No canonical references here.\n")
        self.save_agent("oxide-orchestrator", "An unrelated prompt without policy filenames.")
        errors = self.errors()
        self.assertTrue(any("missing required policy reference" in e for e in errors))
        (self.root / ".opencode/commands/oxide-continue.md").unlink()
        self.assertTrue(any("oxide-continue.md" in e for e in self.errors()))

    def test_obsolete_goal_reference_is_rejected_without_prose_matching(self):
        for old_path in ("~/Documentos/OxideEngine/GOAL.md", "../GOAL.md"):
            self.save_agent("oxide-orchestrator", "AGENTS.md\nGoal: " + old_path)
            with self.subTest(path=old_path):
                self.assertTrue(any("active goal reference" in e for e in self.errors()))

    def test_direct_fnmatch_checks_only(self):
        rules = [self.rule("shell", "git*", "deny"), self.rule("edit", "crates/*", "allow")]
        self.assertEqual(workflow_lint.matching(rules, "shell", "git status"), rules[:1])
        self.assertEqual(workflow_lint.matching(rules, "shell", "echo git status"), [])
        self.assertEqual(workflow_lint.matching(rules, "edit", "crates/a/src/lib.rs"), rules[1:])
        self.assertFalse(workflow_lint.valid_glob("bad[pattern"))
        self.assertTrue(workflow_lint.valid_glob("tools/[ab]*"))


class MissingYamlTests(unittest.TestCase):
    def test_missing_optional_yaml_reports_actionable_nonzero(self):
        original = __import__

        def import_without_yaml(name, *args, **kwargs):
            if name == "yaml":
                raise ImportError("fixture: unavailable")
            return original(name, *args, **kwargs)

        output = io.StringIO()
        with mock.patch("builtins.__import__", side_effect=import_without_yaml), contextlib.redirect_stdout(output):
            result = workflow_lint.main([])
        self.assertEqual(result, 1)
        self.assertIn("PyYAML is unavailable", output.getvalue())
        self.assertIn("NOT been verified", output.getvalue())


if __name__ == "__main__":
    unittest.main()
