## Performance review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ 150 tests ran · doc ✓
**Scope:** main working tree (`git diff HEAD`, staged/unstaged/untracked), restricted by the task to SPEC-0019 implementation — 13 task files, +852/-333 lines at review time, including the 278-line untracked spec. Pre-existing agent tooling, AGENTS.md, IDE/GEMINI files and earlier reports are excluded. No PR is being opened.
**Budgets:** none stated for this functional change. NFR-1901 concerns unchanged authorization and zero new Cargo dependencies, with no timing or memory budget. This review makes no optimization claim and does not infer an old-revision regression delta from a bare-command comparison.
**Environment:** release profile · macOS 27.0.1 arm64 · Python `time.perf_counter` · 3 warmups and 30 measured runs per case · hyperfine unavailable.

The reviewer independently ran `cargo fmt --check`, `cargo clippy --all-targets`, `cargo build`, unrestricted `cargo test`, and `cargo doc --no-deps`, in order. Tests: 81 unit, 66 CLI and 3 tunnel passed. The sole ignored `probe` is a documented fixture, exercised by other tests. No compiler, Clippy or rustdoc warning printed. Tests and measurements ran outside the agent sandbox so loopback sockets and `sandbox-exec` worked.

### Verdicts

| # | Check | Verdict | Finding |
| --- | --- | --- | --- |
| P1 | Blocking call in async fn | Pass | No new production async function or blocking operation inside an async function. `bind_loopback` remains synchronous (`src/proxy/server.rs:95`), using the same local socket operations as the previous startup path; fallback adds at most one failed setup attempt. Existing synchronous config/profile startup is unchanged. |
| P2 | Async that never awaits concurrently | N/A | No production async function is added or changed; the shared accept loop still spawns independent per-connection tasks. |
| P3 | Child output buffered | Pass | Production probe found no child-stream `read_to_end`, `read_to_string`, `.output()` or `wait_with_output`. Existing `child_command.spawn()` preserves inherited streaming at `src/sandbox/mod.rs:163`. Test-only capture is outside the product path. |
| P4 | Startup serialized | Pass | `serve` stops at the first fully prepared listener and spawns its accept task at `src/proxy/server.rs:85`. `src/main.rs:95` obtains the required address before sandbox launch; there is no readiness sleep, probe or await past setup. Fallback depends on failure, so sequential attempts are required. |
| P5 | Lifetime | Pass | `Server::drop` aborts its one accept task at `src/proxy/server.rs:40`; `src/main.rs:105` retains the handle until child completion. `proxy_lifetime_follows_command` passed for both selected families. |
| P6 | Hand-rolled primitive | Pass | Production searches for `thread::spawn`, `Mutex::new` and `mpsc::` found none. The change uses Tokio's existing task handle and spawn. |
| P7 | Speculative parallelism | Pass | One selected listener task replaces two tasks. No new channel, lock or parallel dependent stage is introduced. |
| P8 | Startup work | Pass | Real setup determines family availability. No separate DNS, interface, connectivity or readiness probe, and no opposite-family setup follows preferred success (`src/proxy/server.rs:63`). Disabled-proxy startup still skips setup. |
| P9 | Unmeasured optimization | N/A | This is an availability/configuration behavior change, not a speed optimization. Measurements document actual cost without claiming improvement. |

Static probes executed: `rg -n` across `src/` for `std::fs::`, `std::net::`, `thread::sleep`, `.blocking_`, `block_on`, `read_to_end`, `read_to_string`, `.output(`, `wait_with_output`, `thread::spawn`, `Mutex::new`, and `mpsc::`, followed by checking the diff and containing functions. Existing config file reads and socket setup are synchronous functions; they were already in the runtime's startup call path before this change.

### Measurements

All numbers are medians in milliseconds, 30 measured runs per case after 3 warmups. Each round executed cases in the listed order. Every process had status 0 and empty stdout. Preferred-family runs had empty stderr; forced fallback runs had exactly one expected `continuing on` diagnostic. No aborted startup was timed as a successful launch.

| What | Command / setup | Bare baseline | Release median | Overhead versus bare | Budget | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| Bare command | `/usr/bin/true` | 1.739 ms | 1.739 ms | 0 ms | — | Baseline |
| Default IPv4 | `SANDME_PROXY_PORT=0 target/release/sandme -- /usr/bin/true` | 1.739 ms | 9.875 ms | 8.137 ms | None | Measured; no budget verdict |
| Preferred IPv6 | `SANDME_PROXY_PORT=0 SANDME_PROXY_IP_VERSION=ipv6 target/release/sandme -- /usr/bin/true` | 1.739 ms | 9.927 ms | 8.189 ms | None | Measured; no budget verdict |
| IPv4 → IPv6 fallback | Hold IPv4 loopback on a fixed port; configure that port and `ipv4` preference | 1.739 ms | 9.871 ms | 8.133 ms | None | Measured; no budget verdict |
| IPv6 → IPv4 fallback | Hold IPv6 loopback on a fixed port; configure that port and `ipv6` preference | 1.739 ms | 9.901 ms | 8.163 ms | None | Measured; no budget verdict |

Overheads use unrounded medians; displayed rounding can differ by 0.001 ms. Differences between modes are observations from one local run, not evidence of improvement. No timing budget was missed, so no flamegraph was warranted. Linux startup was not measured or executed on this macOS host.

The exact build command was `cargo build --release`. Timing used `python3 - <<'PY'` from the repository root with the following in-process harness (the fallback sockets were created by a helper that bound both families to one OS-chosen port, retrying up to 20 times if the opposite family was occupied):

```python
import os, socket, statistics, subprocess, tempfile, time
from pathlib import Path
exe = str(Path('target/release/sandme').resolve())

def pair():
    for _ in range(20):
        v4 = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        v4.bind(('127.0.0.1', 0)); v4.listen()
        v6 = socket.socket(socket.AF_INET6, socket.SOCK_STREAM)
        try:
            v6.bind(('::1', v4.getsockname()[1])); v6.listen()
            return v4, v6, v4.getsockname()[1]
        except OSError:
            v4.close(); v6.close()
    raise RuntimeError('could not reserve loopback pair')

with tempfile.TemporaryDirectory(prefix='sandme-performance-', dir='/private/tmp') as temp:
    env = {k: v for k, v in os.environ.items() if not k.startswith('SANDME_')
           and k not in ['HTTP_PROXY', 'HTTPS_PROXY', 'http_proxy', 'https_proxy', 'ALL_PROXY', 'all_proxy']}
    env['HOME'] = temp
    a4, a6, port_a = pair(); a6.close()
    b4, b6, port_b = pair(); b4.close()
    cases = [
        ('bare_true', ['/usr/bin/true'], env),
        ('preferred_ipv4', [exe, '--', '/usr/bin/true'], dict(env, SANDME_PROXY_PORT='0')),
        ('preferred_ipv6', [exe, '--', '/usr/bin/true'], dict(env, SANDME_PROXY_PORT='0', SANDME_PROXY_IP_VERSION='ipv6')),
        ('fallback_to_ipv6', [exe, '--', '/usr/bin/true'], dict(env, SANDME_PROXY_PORT=str(port_a), SANDME_PROXY_IP_VERSION='ipv4')),
        ('fallback_to_ipv4', [exe, '--', '/usr/bin/true'], dict(env, SANDME_PROXY_PORT=str(port_b), SANDME_PROXY_IP_VERSION='ipv6')),
    ]
    samples = {name: [] for name, _, _ in cases}
    try:
        for run in range(33):
            for name, cmd, case_env in cases:
                start = time.perf_counter()
                result = subprocess.run(cmd, cwd=temp, env=case_env, capture_output=True, timeout=5)
                elapsed = (time.perf_counter() - start) * 1000
                assert result.returncode == 0 and not result.stdout, name
                if name.startswith('fallback'):
                    assert len(result.stderr.splitlines()) == 1 and b'; continuing on ' in result.stderr, name
                else:
                    assert not result.stderr, name
                if run >= 3:
                    samples[name].append(elapsed)
        bare = statistics.median(samples['bare_true'])
        for name, values in samples.items():
            median = statistics.median(values)
            print(name, 'median_ms=', median, 'overhead_vs_bare_ms=', median - bare)
    finally:
        a4.close(); b6.close()
```

The harness ran from stdin; no script file was added. Its temporary input/work directory was removed automatically and sockets closed. Release outputs remain normal Cargo build artifacts. Only this requested report was added by the reviewer; no source/config/spec/test edits were made.

### Findings

None in the task diff. These measurements establish overhead against the stated bare-command baseline, not a before/after revision delta or compliance with a nonexistent SPEC-0019 latency budget.

### Defects noticed

None. Every measured command completed successfully, including both forced fallback directions.

### Result

Blockers: 0 · Advisories: 0 · Not measured: 1 (Linux) — the reviewed change introduces no observed performance-contract violation; release startup overhead was approximately 8.1–8.2 ms against bare `true` on this host. No numeric-budget or regression claim is made.
