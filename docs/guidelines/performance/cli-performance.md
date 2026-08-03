# CLI performance

How `sandme` stays fast enough that a user coding inside the sandbox never notices it is there.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

`sandme` has one job: run the HTTP proxy and the Seatbelted command concurrently, and stay out of
the way. Every millisecond it adds sits between the user and their editor.

Performance is a requirement, not a virtue. It is stated as an `NFR-` with a number in
`docs/specs/` and proven by measurement. "Feels fast" is not evidence — see
`docs/guidelines/code/simplicity.md` §4.

## 2. Concurrency

- The proxy and the sandboxed command MUST run concurrently. Neither may block the other's
  startup.
- Independent work MUST run concurrently. Work is independent when neither side reads what the
  other writes.
- Dependent work MUST stay sequential. Parallelising a dependent chain buys nothing and adds a
  channel, a lock, and a race.
- The child's lifetime governs the process. When the child exits, the proxy shuts down; `sandme`
  MUST NOT outlive it.

## 3. Async and I/O

- I/O that overlaps other work MUST be async, on the `tokio` runtime already in `Cargo.toml`.
- Async is a concurrency tool, not a speed-up. A call chain that never awaits concurrently MUST
  be a synchronous `fn` — see `docs/guidelines/code/simplicity.md` §5, *Premature async*.
- MUST NOT block the runtime. No `std::fs`, `std::net`, `std::thread::sleep` or CPU-bound loop
  inside an `async fn`. Use the `tokio` equivalent, or `spawn_blocking`.
- The child's stdout and stderr MUST stream as they are produced. Buffering them to completion
  turns an interactive session into a hang — see `docs/guidelines/architecture/posix.md` §3.

## 4. Existing libraries for low-level work

- Use an existing, well-maintained crate for I/O, async and concurrency primitives. `tokio`,
  `tokio-util` and `hyper` are already here; use them.
- A hand-rolled scheduler, thread pool, lock or event loop is a Blocker unless no crate covers
  the need and an ADR records why.
- A new dependency still has to clear the bar in `AGENTS.md`: well known, well maintained, and
  satisfying a requirement the standard library and current crates cannot.

## 5. Measure before optimising

- Profile with a flamegraph before changing code for speed. The bottleneck is where the profile
  says it is, not where it feels like it is.
- A change made for performance MUST cite its measurement — flamegraph, benchmark or timing — in
  the commit message or the spec.
- Re-measure after the change. An optimisation with no before-and-after is speculation.
- An optimisation that costs readability MUST cite the `NFR-` it serves. Absent that, the simpler
  code wins.

## 6. Startup cost

- The startup path — parse arguments, read config, build the sandbox profile, spawn — is on the
  user's critical path. Keep it allocation-light and I/O-light.
- MUST NOT do work at startup that the requested command may not need. Defer it to first use.
- MUST NOT read the network at startup.

## 7. Review checklist

- [ ] Proxy and sandboxed command start concurrently; neither blocks the other.
- [ ] Everything parallelised is genuinely independent; nothing dependent is parallelised.
- [ ] Every `async fn` awaits concurrently; nothing else is `async`.
- [ ] No blocking call inside the async runtime.
- [ ] Child output streams; nothing buffers it to completion.
- [ ] No hand-rolled concurrency primitive without an ADR.
- [ ] Every performance change cites a before-and-after measurement.
- [ ] Startup does no work the requested command may not need.

## 8. Related

- `docs/guidelines/code/simplicity.md` — premature `async`, evidence over anticipation.
- `docs/guidelines/architecture/posix.md` — stream pass-through and exit-status propagation.
- `docs/guidelines/sdd/spec-driven-development.md` — `NFR-` requirements must carry numbers.
