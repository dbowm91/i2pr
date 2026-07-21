"""Bounded subprocess ownership for reference-router adapters."""

from __future__ import annotations

import subprocess
import threading
import time
import os
import tempfile
from collections import deque
from pathlib import Path
from typing import Sequence


class ProcessError(RuntimeError):
    """A bounded process operation failed."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


class BoundedProcess:
    """Own one child process and retain only a bounded private log."""

    def __init__(
        self,
        command: Sequence[str],
        log_path: Path,
        max_log_bytes: int = 131_072,
        environment: dict[str, str] | None = None,
    ):
        self.command = tuple(command)
        self.log_path = log_path
        self.max_log_bytes = max_log_bytes
        self.environment = environment
        self.process: subprocess.Popen[bytes] | None = None
        self._reader: threading.Thread | None = None
        self._ready_lines: deque[str] = deque(maxlen=32)
        self._line_event = threading.Event()
        self._line_lock = threading.Lock()
        self._log_bytes = 0
        self.forced = False
        self.pid_path = self.log_path.parent.parent / "pids" / f"{self.log_path.stem}.pid"

    def _write_pid(self) -> None:
        self.pid_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        fd, temporary = tempfile.mkstemp(prefix=f".{self.pid_path.name}.", dir=self.pid_path.parent)
        try:
            with os.fdopen(fd, "w", encoding="ascii") as handle:
                handle.write(f"{self.process.pid}\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.chmod(temporary, 0o600)
            os.replace(temporary, self.pid_path)
        finally:
            if os.path.exists(temporary):
                os.unlink(temporary)

    def _remove_pid(self) -> None:
        try:
            self.pid_path.unlink()
        except FileNotFoundError:
            pass

    def start(self) -> None:
        if self.process is not None:
            raise ProcessError("already-started")
        self.log_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.process = subprocess.Popen(
            list(self.command),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            close_fds=True,
            env=self.environment,
        )
        try:
            self._write_pid()
        except OSError as exc:
            self.process.terminate()
            self.process.wait(timeout=2)
            self.process = None
            raise ProcessError("pid-file-write-failed") from exc
        self._reader = threading.Thread(target=self._drain, name="interop-log-drain", daemon=True)
        self._reader.start()

    def _drain(self) -> None:
        assert self.process is not None and self.process.stdout is not None
        with self.log_path.open("wb") as log:
            while True:
                line = self.process.stdout.readline()
                if not line:
                    self._line_event.set()
                    return
                if self._log_bytes < self.max_log_bytes:
                    remaining = self.max_log_bytes - self._log_bytes
                    log.write(line[:remaining])
                    log.flush()
                    self._log_bytes += min(len(line), remaining)
                decoded = line[:4096].decode("utf-8", errors="replace")
                with self._line_lock:
                    self._ready_lines.append(decoded)
                self._line_event.set()

    def wait_for_record(self, parser, timeout_seconds: float):
        """Wait for one bounded structured record from the child output."""

        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            with self._line_lock:
                lines = tuple(self._ready_lines)
            for line in lines:
                try:
                    value = parser(line)
                except ValueError:
                    continue
                if value is not None:
                    return value
            if self.process is None:
                raise ProcessError("not-started")
            if self.process.poll() is not None:
                raise ProcessError("process-exited-before-status")
            self._line_event.wait(min(0.05, max(0.0, deadline - time.monotonic())))
            self._line_event.clear()
        raise ProcessError("status-timeout")

    def wait_ready(self, tokens: Sequence[str], timeout_seconds: float) -> None:
        deadline = time.monotonic() + timeout_seconds
        while time.monotonic() < deadline:
            if self.process is None:
                raise ProcessError("not-started")
            if self.process.poll() is not None:
                raise ProcessError("process-exited-before-ready")
            with self._line_lock:
                ready = any(token in line for line in self._ready_lines for token in tokens)
            if ready:
                return
            time.sleep(0.02)
        raise ProcessError("readiness-timeout")

    def stop(self, timeout_seconds: float) -> str:
        if self.process is None:
            return "not-started"
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=timeout_seconds)
            except subprocess.TimeoutExpired:
                self.forced = True
                self.process.kill()
                self.process.wait(timeout=timeout_seconds)
        if self._reader is not None:
            self._reader.join(timeout=timeout_seconds)
        self._remove_pid()
        return "forced" if self.forced else "clean"

    def snapshot(self) -> dict[str, int | str]:
        return {
            "running": int(self.process is not None and self.process.poll() is None),
            "exit_code": self.process.returncode if self.process is not None else -1,
            "log_bytes": self._log_bytes,
            "forced": int(self.forced),
        }

    def observed_phrase(self, phrases: Sequence[str]) -> bool:
        """Return whether a bounded, implementation-specific status phrase appeared.

        Checks the bounded ``_ready_lines`` deque first (fast, in-memory), then
        falls back to a bounded scan of the log file. The fallback matters
        because debug-level i2pd logs fill the 32-line deque quickly and
        rotate the authenticated phrase out before ``authenticated_observation``
        is queried.
        """

        with self._line_lock:
            if any(phrase in line for line in self._ready_lines for phrase in phrases):
                return True
        try:
            file_size = self.log_path.stat().st_size
        except OSError as exc:
            import sys
            print(f"[observed_phrase DEBUG] stat failed for {self.log_path}: {exc}", file=sys.stderr)
            return False
        tail_size = min(file_size, 131_072)
        if tail_size <= 0:
            import sys
            print(f"[observed_phrase DEBUG] empty log: {self.log_path}", file=sys.stderr)
            return False
        try:
            with self.log_path.open("rb") as handle:
                handle.seek(-tail_size, os.SEEK_END)
                chunk = handle.read(tail_size)
        except OSError as exc:
            import sys
            print(f"[observed_phrase DEBUG] read failed for {self.log_path}: {exc}", file=sys.stderr)
            return False
        try:
            text = chunk.decode("utf-8", errors="replace")
        except LookupError:
            text = chunk.decode("utf-8", errors="replace")
        result = any(phrase in line for line in text.splitlines() for phrase in phrases)
        import sys
        print(f"[observed_phrase DEBUG] path={self.log_path} size={file_size} tail_size={tail_size} result={result} phrases={phrases}", file=sys.stderr)
        return result
