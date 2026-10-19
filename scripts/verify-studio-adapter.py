"""Exercise the real local Code Studio daemon from an isolated fixture home.

Requires the published Studio LSP daemon branch, an installed TypeScript language
server, and the compiled studio_adapter_probe example. Uses no model or mock HTTP.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--daemon-bin", required=True, type=Path)
    parser.add_argument("--probe-bin", required=True, type=Path)
    parser.add_argument("--lsp-bin", required=True, type=Path)
    parser.add_argument("--typescript-package", required=True, type=Path)
    parser.add_argument("--fixture-files", type=int, default=0, help="Generated TypeScript files for a bounded scale smoke (0..10000)")
    args = parser.parse_args()
    if not 0 <= args.fixture_files <= 10000:
        parser.error("--fixture-files must be in 0..10000")
    for executable in (args.daemon_bin, args.probe_bin):
        if not executable.is_absolute() or not executable.is_file():
            parser.error("executables must be existing absolute paths")
    if not (args.typescript_package / "lib/tsserver.js").is_file():
        parser.error("--typescript-package must contain an installed lib/tsserver.js")
    fixture = Path(tempfile.mkdtemp(prefix="aos-studio-adapter-real-"))
    data = fixture / "data"
    data.mkdir()
    binary = fixture / args.daemon_bin.name
    shutil.copyfile(args.daemon_bin, binary)
    sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    origin = f"http://127.0.0.1:{port}"
    (data / "tools_policy.yaml").write_text(json.dumps({
        "allowed_read_paths": [str(data)], "allowed_write_paths": [str(data)],
        "allowed_commands": ["git", "node", "typescript-language-server"],
        "command_timeout_secs": 15, "run_command_default_cwd_workspace": True,
    }), encoding="utf-8")
    (data / "permissions_mode.json").write_text('{"mode":"allow_all"}', encoding="utf-8")
    (data / "llm_router.yaml").write_text(json.dumps({
        "global": {"enable_fallback": False, "default_timeout_secs": 1, "default_max_retries": 0},
        "providers": {"ollama": {"base_url": "http://127.0.0.1:1"}},
        "task_types": {"conversation": {"primary": {"provider": "ollama", "model": "unavailable"},
                                          "fallback": []}},
    }), encoding="utf-8")
    environment = {**os.environ, "AKASHA_DATA_DIR": str(data), "AKASHA_BIND": "127.0.0.1",
                   "AKASHA_PORT": str(port), "AKASHA_EMBEDDED_PRELOAD": "0",
                   "AKASHA_PLAYWRIGHT_AUTO_INSTALL": "0",
                   "PATH": str(args.lsp_bin) + os.pathsep + os.environ.get("PATH", "")}
    process = None
    job = None
    project = None
    log = (fixture / "daemon.log").open("ab")
    kernel = None
    if os.name == "nt":
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateJobObjectW.restype = wintypes.HANDLE
        kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
        kernel.TerminateJobObject.argtypes = [wintypes.HANDLE, wintypes.UINT]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]

    def api(path, method="GET", body=None):
        request = urllib.request.Request(origin + path, method=method,
            headers={"Content-Type": "application/json"},
            data=None if body is None else json.dumps(body).encode())
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)

    def start():
        nonlocal process, job
        process = subprocess.Popen([str(binary)], cwd=fixture, env=environment,
            stdout=log, stderr=log, stdin=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
            start_new_session=os.name != "nt")
        if kernel:
            job = kernel.CreateJobObjectW(None, None)
            if not job or not kernel.AssignProcessToJobObject(job, int(process._handle)):
                process.kill()
                raise OSError(ctypes.get_last_error(), "cannot supervise fixture daemon")
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"fixture daemon exited: {fixture}")
            try:
                return api("/api/status")
            except Exception:
                time.sleep(0.25)
        raise TimeoutError(f"fixture daemon startup: {fixture}")

    def stop():
        nonlocal process, job
        if process:
            if project:
                try:
                    api(f"/api/studio/projects/{project}/lsp/sessions", "DELETE")
                except Exception:
                    pass
            if kernel and job:
                kernel.TerminateJobObject(job, 1)
                kernel.CloseHandle(job)
                job = None
            elif os.name != "nt":
                import signal
                os.killpg(process.pid, signal.SIGTERM)
            else:
                process.terminate()
            process.wait(timeout=10)
            process = None

    try:
        status = start()
        created = api("/api/studio/projects", "POST", {"name": "Akasha OS adapter fixture"})
        project = created["id"]
        root = Path(created["path"])
        assert root.resolve().parent == (data / "studio-projects").resolve()
        (root / ".akasha-os-adapter-fixture").write_text("fixture only", encoding="utf-8")
        (root / "main.ts").write_text("export const answer = 42;\nconsole.log(answer);\n", encoding="utf-8")
        (root / "tsconfig.json").write_text('{"compilerOptions":{"strict":true},"include":["main.ts"]}', encoding="utf-8")
        # The real language server requires a real workspace TypeScript SDK.
        # Copy the existing installed package; do not download during verification.
        shutil.copytree(args.typescript_package, root / "node_modules/typescript")
        (root / ".gitignore").write_text("node_modules/\n", encoding="utf-8")
        generated = root / "generated"
        generated.mkdir()
        for number in range(args.fixture_files):
            (generated / f"part_{number:05}.ts").write_text(
                f"// Bounded generated scale fixture {number}\nexport function part_{number}(value: number): number {{ return value + {number}; }}\n", encoding="utf-8")
        started = time.monotonic()
        completed = subprocess.run([str(args.probe_bin), origin, project, str(root)],
            capture_output=True, text=True, timeout=120)
        (fixture / "probe.log").write_text(completed.stdout + completed.stderr, encoding="utf-8")
        if completed.returncode:
            raise RuntimeError(f"adapter probe failed ({completed.returncode}): {fixture}")
        elapsed_ms = round((time.monotonic() - started) * 1000)
        result = json.loads(completed.stdout.splitlines()[-1])
        assert result["index"]["status"] == "ready"
        assert result["index"]["files_indexed"] >= args.fixture_files + 1
        stop()
        restart_status = start()
        persisted = api(f"/api/studio/projects/{project}")
        assert persisted["project_summary"] == result["memory"]
        report = {"ok": True, "fixture": str(fixture), "daemon_sha256": sha256,
                  "daemon_status": status, "restart_status": restart_status,
                  "probe": result, "memory_survived_restart": True,
                  "fixture_files": args.fixture_files, "probe_elapsed_ms": elapsed_ms,
                  "scope": "real daemon/index/TypeScript LSP on a generated fixture; no LLM task or user data"}
        (fixture / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(json.dumps(report))
    finally:
        stop()
        log.close()


if __name__ == "__main__":
    main()
