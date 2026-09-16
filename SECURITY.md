# Security policy

`sandme` is a security boundary. A bug that lets the sandboxed command read a path it was not
given, reach a network destination the profile denies, or escape the sandbox altogether is a
vulnerability in the product, not an inconvenience. Report it privately.

## Supported versions

`sandme` is pre-1.0. Only the latest release receives fixes. There are no backports and no
maintained release branches. Upgrading is the remedy.

## Reporting a vulnerability

Use GitHub private vulnerability reporting: open the repository's **Security** tab and choose
**Report a vulnerability**. The report stays private to the maintainer until a fix is published.

Do not open a public issue, a discussion or a pull request for a suspected escape. A pull request
describing the fix describes the hole.

Fallback contact if GitHub reporting is unavailable: lpepefreitas@gmail.com

## What a useful report contains

A sandbox report is reproducible only when the profile inputs are known. The profile is built
from the configuration, so the configuration is part of the bug.

- Platform and version: macOS release, or Linux distribution and `uname -r` (Landlock ABI depends
  on the kernel).
- `sandme --version`.
- The exact command, verbatim — including whether it was the multi-operand form or a single
  quoted operand routed through `/bin/bash -c`. The two build different invocations.
- The `~/.sandme/config.toml` in effect, and every `SANDME_*` environment variable that was set.
  The environment overrides the file, so both matter.
- What was observed: the path read or written, the destination reached, the process that escaped
  — and what was expected instead.
- A minimal reproduction, if you have one.

A report that shows the sandbox allowing something it documents as denied is actionable. A report
that says "the sandbox seems weak" is not.

## What to expect

One maintainer, no security team. The commitment is deliberately modest and meant to be kept:

- Acknowledgement of a report within 7 days.
- An initial assessment — accepted, needs more information, or out of scope — within 14 days.
- Coordinated disclosure after that: a fix, a release, and then public details. If a fix is going
  to take long, you will be told so rather than left waiting.

Credit in the release notes is offered to reporters who want it.

## Out of scope

`sandme` documents its accepted weaknesses rather than hiding them. The **Known limitations**
table at the end of [`README.md`](README.md#known-limitations) is that list, and each row links
to the issue tracking it. It covers, among others:

- `mach-lookup` still allowed with no service allowlist.
- The proxy running unsandboxed, with no destination allowlist.
- App-bundle wrapper redirection only applying to the first word of a command, and losing the
  wrapper's own flags.
- `allow_private_egress` re-opening loopback, RFC1918 and link-local by design.
- git SSH remotes denied by design, because egress goes only through the proxy.

A report that describes a documented limitation is not a vulnerability. It becomes one the moment
it demonstrates something beyond what is documented — an escape the limitation was not known to
permit, a wider reach than the table claims, or a working exploit where the table records only a
theoretical gap. Show that step, and the report is in scope.

Also out of scope: findings against a configuration that grants what is being reported
(`shared_paths = ["~/"]` exposing the home directory is the configuration working), and reports
about dependencies that have no path to affecting `sandme`'s sandbox.
