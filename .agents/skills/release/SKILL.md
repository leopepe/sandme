---
name: release
description: Use when the user asks to cut a release, tag a version, publish a release, ship vX.Y.Z, or asks what is needed before releasing. Runs the pre-flight checks, rehearses the pipeline, smoke-tests the packaged binary, and stops for human approval before each irreversible step.
---

# Release

Cut a tagged release of sandme. `.github/workflows/release.yml` does the building and packaging;
this skill is everything around it — what must be true before a tag exists, what to verify after,
and where to stop and ask.

The pipeline is already proven. What goes wrong at a release is not the build: it is tagging the
wrong commit, tagging a version the manifest disagrees with, shipping a binary nobody ran, or
publishing a draft that should have been read first. Those are the four things below.

## Two steps this skill never takes on its own

| Step | Why it needs a human |
| --- | --- |
| `git push origin vX.Y.Z` | A pushed tag is permanent. Anyone who fetched it keeps it, and moving it makes two people's `v0.1.0` different commits. |
| `gh release edit vX.Y.Z --draft=false` | A published release with downloads cannot be unpublished in any way that matters — links, mirrors and caches already have it. |

Stop at each, report what you have, and wait for explicit approval. "The user asked me to release"
is approval for the process, not for these two steps. Ask again at each one.

## 1. Pre-flight

Every row must hold. Report the actual output of each, not a claim that it passed.

| # | Check | Command | Must be |
| --- | --- | --- | --- |
| 1 | On `main`, up to date | `git status -sb && git fetch && git status -sb` | No ahead/behind, clean tree |
| 2 | Version in the manifest | `grep '^version' Cargo.toml` | Exactly the version being released, without the `v` |
| 3 | Lockfile agrees | `git diff --exit-code Cargo.lock` | No diff — see the trap below |
| 4 | Tag is free | `git tag -l vX.Y.Z` and `gh release view vX.Y.Z` | Empty / not found |
| 5 | Quality gate | `docs/guidelines/code/quality-gates.md`, all five in order | Green, and **read the output** — clippy and build exit `0` while printing warnings |
| 6 | CI green on this exact commit | `gh run list --branch main --limit 3` | The run for `HEAD` succeeded. A red `main` is not releasable, whatever the local gate says |
| 7 | Nothing unreleased is missing | `git log --oneline $(git describe --tags --abbrev=0 2>/dev/null)..HEAD` | The commits you expect; nothing half-merged |

**The lockfile trap.** The release build runs `cargo build --release --locked`. Bumping `version`
in `Cargo.toml` without running `cargo build` leaves `Cargo.lock` naming the old version, and
`--locked` then refuses to build — on the runner, after the tag exists. Bump the version and the
lockfile in the same commit, merged before the tag.

If the release needs a version bump, that is an ordinary PR (`chore: release vX.Y.Z`) merged to
`main` first. Never tag a branch.

## 2. Rehearse the pipeline

```shell
gh workflow run release.yml --ref main
gh run watch <run-id> --exit-status
```

`workflow_dispatch` builds and packages all three targets and **skips the publish job**, so it
proves the pipeline without creating a tag or a release. Confirm afterwards that it created
nothing:

```shell
gh release list          # unchanged
gh api repos/{owner}/{repo}/actions/runs/<run-id>/artifacts \
  -q '.artifacts[] | "\(.name)  \(.size_in_bytes)"'
```

Three artifacts, one per target: `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`,
`aarch64-unknown-linux-gnu`. There is no `x86_64-apple-darwin` — Intel macOS was dropped
deliberately (PR #59), so its absence is correct, not a failure.

The dispatch run skips the tag-vs-manifest check (there is no tag on a branch). Verify that step's
logic by hand instead, because on a real tag it runs first and aborts everything:

```shell
awk -F'[ \t]*=[ \t]*' '/^\[/ { s = $0 } s == "[package]" && $1 == "version" { gsub(/"/, "", $2); print $2; exit }' Cargo.toml
```

It must print exactly the version you are about to tag.

## 3. Smoke-test the packaged binary

**This is the step no other gate covers.** CI builds debug and runs `cargo test`; it never
executes the `--release` binary inside the tarball. Download the rehearsal's artifact for the
platform you are on and run it — a build that compiles and a binary that sandboxes are different
claims.

```shell
gh run download <run-id> --name <your-target> --dir /tmp/sandme-smoke
cd /tmp/sandme-smoke
shasum -a 256 -c *.sha256          # must print OK
tar xzf *.tar.gz && cd sandme-*/
```

| Check | Command | Expected |
| --- | --- | --- |
| Version matches the tag | `./sandme --version` | The version being released |
| It runs a command | `./sandme echo hello` | `hello` |
| The shell form works | `./sandme 'echo a-b \| tr - " "'` | `a b` |
| The sandbox confines | `./sandme sh -c 'cat ~/.ssh/known_hosts'` from a directory that is not `~` | Denied, not printed |
| Egress goes through the proxy | `./sandme -- curl -sS https://example.com/` | HTML, not a connection error |
| Exit status passes through | `./sandme sh -c 'exit 3'; echo $?` | `3` |
| Not-found is `127` | `./sandme sandme-no-such-command; echo $?` | `127` |
| Licence travels with it | `ls` | `sandme`, `README.md`, `LICENSE` |

Only the artifact matching the host can be run. Say in the report which target you smoke-tested
and which two you did not — an untested target is a known gap, not a silent pass.

On macOS the release binaries are unsigned and un-notarised, so a tarball downloaded through a
browser carries a quarantine attribute and Gatekeeper refuses it. `gh run download` does not set
that attribute, so the smoke test will not reproduce what a user hits. The README documents
`xattr -d com.apple.quarantine`; check that instruction is still present rather than trying to
reproduce the dialog.

## 4. Tag — **stop and ask first**

Report the pre-flight table, the rehearsal result and the smoke test, then ask for approval,
naming the exact commit the tag will point at:

```shell
git log --oneline -1          # the commit that becomes vX.Y.Z
```

On approval, and not before:

```shell
git tag -a vX.Y.Z -m "<one line: what this release is>"
git push origin vX.Y.Z
```

Annotated (`-a`), not lightweight: an annotated tag carries the tagger, the date and a message,
and that is what a release is.

## 5. Verify the release run

```shell
gh run list --workflow=release.yml --limit 1
gh run watch <run-id> --exit-status
```

Then check the draft actually holds what it should:

```shell
gh release view vX.Y.Z --json isDraft,tagName,assets \
  -q '{draft:.isDraft, tag:.tagName, assets:[.assets[].name]}'
```

Six assets: three `.tar.gz` and three `.tar.gz.sha256`. `isDraft` must be `true` — the workflow
creates drafts on purpose, and a release that arrives published means someone changed that.

## 6. Publish — **stop and ask again**

Show the draft's generated notes and the asset list, then ask. On approval:

```shell
gh release edit vX.Y.Z --draft=false
```

## 7. Report

```markdown
## Release vX.Y.Z

**Pre-flight:** branch ✓ · version ✓ · lockfile ✓ · tag free ✓ · gate ✓ <N> tests · CI ✓ <run-id>
**Rehearsal:** <run-id> — 3/3 targets built, publish skipped, no release created
**Smoke test:** <target> — checksum ✓, <N>/<N> checks passed. Not tested: <the other two targets>
**Tag:** vX.Y.Z → <commit> (approved by <user>)
**Release run:** <run-id> — 6 assets attached, draft
**Published:** <yes, on approval | still a draft>

### Anything that did not hold
<one line each, or "Nothing.">
```

## Rollback

| Situation | What to do |
| --- | --- |
| Draft is wrong, not yet published | `gh release delete vX.Y.Z --yes`, then `git push --delete origin vX.Y.Z` and `git tag -d vX.Y.Z`. Fix, re-tag. |
| Tag pushed, build failed | The tag is fine — fix `main`, delete and re-push the tag only if the commit itself was wrong. |
| Already published | **Do not delete it.** Publish `vX.Y.Z+1` with the fix. A deleted release breaks everyone who has the link, and a re-pushed tag pointing elsewhere is worse than a bad version that is honestly superseded. |

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Pushing the tag because the user said "release it" earlier | Ask at the tag. Approval for the process is not approval for the irreversible step |
| Publishing the draft in the same breath as creating it | Two approvals, two moments. The draft exists to be read |
| Reporting "CI is green" from an older run | Check the run for the exact commit being tagged |
| Skipping the smoke test because the build succeeded | Compiling and running are different claims; this is the only step that runs the artifact |
| Saying "all targets verified" after testing one | Name the one you ran and the two you did not |
| Bumping `Cargo.toml` without `cargo build` | `--locked` will reject the lockfile on the runner, after the tag exists |
| Tagging a branch or an unmerged commit | Tags go on `main`, after the merge |
| Re-tagging to fix a published release | Ship the next patch version instead |
| Treating the rehearsal as optional for a patch release | It is two minutes and it is the only thing that catches a packaging break before the tag |
