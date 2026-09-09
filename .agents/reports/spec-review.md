## Spec review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ 78 tests ran (38 unit + 40
integration; 1 ignored with a cited reason) · doc ✓ 0 warnings — all run this session, after the
last edit.
**Specs indexed:** 5 (Accepted 1 · Review 4)
**Requirements indexed:** 37 (FR 31 · NFR 6), of which SPEC-0001/NFR-003 is withdrawn → 36 active
**Deferred:** SPEC-0003/NFR-202 (no connection attempt, timing), SPEC-0005/NFR-302 (measurement
set) — both carry numbers and a procedure; `performance-review` owns re-measuring them.

### Coverage

| Requirement | Spec | Status | Implementation | Verified by | Evidence |
| --- | --- | --- | --- | --- | --- |
| FR-001 | SPEC-0001 | Implemented | `src/main.rs:36-42` | `passes_multi_word_arguments_unchanged` | passed this session |
| FR-002 | SPEC-0001 | Implemented | `src/sandbox.rs:88-135` | `prints_child_output`, `reports_signal_deaths_as_128_plus_n` | passed this session. The row also names `propagates_child_exit_status`, hedged as "-equivalent"; no test of that name exists |
| FR-003 | SPEC-0001 | Untested | `src/profile.rs:52-53` (`process-exec`, `process-fork`) | — | Verification says "verified manually"; no test names it |
| FR-004 | SPEC-0001 | Implemented | `src/profile.rs:156-171` | `denies_writes_outside_shared_paths`, `shares_configured_paths_read_write` | passed this session |
| FR-005 | SPEC-0001 | Untested | `src/main.rs:65-78`, `src/proxy.rs:49-68` | — | Verification says "Manual". `proxy_lifetime_follows_command` (`tests/cli.rs`) in fact covers it and passed — the row is stale |
| FR-006 | SPEC-0001 | Implemented | `src/sandbox.rs:85-90`, `src/profile.rs:80-84` | `routes_http_egress_through_the_proxy`, `routes_network_only_to_the_proxy` | passed this session |
| FR-007 | SPEC-0001 | Implemented | `src/config.rs:77-89` | `keeps_defaults_for_absent_settings` | passed this session |
| FR-008 | SPEC-0001 | Implemented | `src/config.rs:105-127` | `environment_overrides_previous_values` | passed this session |
| FR-009 | SPEC-0001 | Implemented | `src/config.rs:105-127` | `environment_overrides_previous_values` | passed this session |
| NFR-001 | SPEC-0001 | Implemented | `src/sandbox.rs:108-121` | `uses_macos_seatbelt_framework` | passed this session |
| NFR-002 | SPEC-0001 | Implemented | `src/sandbox.rs:85-90` | `no_manual_proxy_configuration_needed` | passed this session |
| NFR-003 | SPEC-0001 | — | — | — | Withdrawn 2026-08-08 |
| FR-101 | SPEC-0002 | Not due (Implemented) | `src/profile.rs:173-192` | `gui_mode_allows_temp_writes`, `grants_the_home_library_only_in_gui_mode` | Spec is `Review`. Both passed this session. Verification locates them in `src/sandbox.rs`; they moved to `src/profile.rs` in #37 |
| FR-102 | SPEC-0002 | Not due (Implemented) | `src/profile.rs:196-217` | `denies_writing_a_launch_agent_even_when_the_whole_home_is_shared`, `denies_reading_the_keychains_even_when_the_whole_home_is_shared` | Spec is `Review`. Both passed this session |
| FR-103 | SPEC-0002 | Not due (Implemented) | `src/profile.rs:220-229` | the same two tests | Spec is `Review`. Passed this session |
| FR-104 | SPEC-0002 | Not due (Implemented) | `src/profile.rs:180-190` | `denies_the_home_library_when_gui_mode_is_off`, `grants_the_home_library_only_in_gui_mode` | Spec is `Review`. Passed this session |
| NFR-101 | SPEC-0002 | Not due (Implemented) | `src/profile.rs:196-217` | `denies_the_launchd_and_keychain_directories_after_every_grant` | Spec is `Review`. Passed this session; same stale file reference as FR-101 |
| FR-201 | SPEC-0003 | Not due (Implemented) | `src/egress.rs:18-91` | `refuses_to_relay_to_a_service_on_host_loopback`, `refuses_to_relay_to_the_cloud_metadata_address`, `restricts_every_range_the_sandbox_denies_directly`, `relays_to_ordinary_public_addresses` | Spec is `Review`. All passed this session. ID collides with SPEC-0004/FR-201 — see C1 |
| FR-202 | SPEC-0003 | Not due (Implemented) | `src/proxy.rs:197-211` | `refuses_to_relay_to_a_service_on_host_loopback` | Spec is `Review`. Passed. Collides — C1 |
| FR-203 | SPEC-0003 | Not due (Implemented) | `src/credential.rs:24-121`, `src/proxy.rs:73`, `src/sandbox.rs:87` | `refuses_a_local_client_without_the_invocation_credential`, `publishes_a_secret_the_child_can_present`, `expects_what_a_client_reading_the_proxy_url_sends`, `encodes_base64_at_every_padding_length` | Spec is `Review`. All passed. Collides — C1 |
| FR-204 | SPEC-0003 | Not due (Implemented) | `src/proxy.rs:233` | `relays_to_host_loopback_when_private_egress_is_allowed` | Spec is `Review`. Passed. Collides — C1 |
| FR-205 | SPEC-0003 | Not due (Implemented) | `src/config.rs:31-40`, `src/egress.rs:24` | `relays_to_host_loopback_when_private_egress_is_allowed`, `opens_private_egress_only_when_the_environment_asks_for_it`, `keeps_defaults_for_keys_a_config_file_omits` | Spec is `Review`. All passed. Collides — C1 |
| FR-206 | SPEC-0003 | Not due (Implemented) | `src/egress.rs:30-145`, `src/proxy.rs` | `restricts_a_hostname_that_resolves_into_a_restricted_range`, `reads_the_destination_a_request_names` | Spec is `Review`. Passed |
| NFR-201 | SPEC-0003 | Not due (Implemented) | `src/credential.rs:24-69` | `publishes_a_secret_the_child_can_present` | Spec is `Review`. Passed. Collides with SPEC-0004/NFR-201 — C1 |
| NFR-202 | SPEC-0003 | Not due (Untested) | `src/egress.rs:18-91` | — | Spec is `Review`. Verification argues from a timeout, not a measurement; deferred to `performance-review` |
| FR-201 | SPEC-0004 | Not due (Implemented) | `src/main.rs:27`, `src/main.rs:103-111` | `reports_a_malformed_config_with_the_reserved_status`, `reports_a_proxy_that_cannot_bind_with_the_reserved_status` | Spec is `Review`. Both passed. Collides — C1 |
| FR-202 | SPEC-0004 | Not due (Implemented) | `src/main.rs:53-55` | the same two tests | Spec is `Review`. Passed. Collides — C1 |
| FR-203 | SPEC-0004 | Not due (Implemented) | `src/main.rs:33`, `src/executable.rs:26-31` | `reports_a_command_it_cannot_find_as_127`, `reports_a_missing_command_in_a_shell_string_as_127`, `reports_a_name_nothing_answers_to` | Spec is `Review`. Passed. Collides — C1 |
| FR-204 | SPEC-0004 | Not due (Implemented) | `src/main.rs:30`, `src/sandbox.rs:136-141` | `reports_a_command_that_is_not_executable_as_126`, `reports_a_non_executable_command_in_a_shell_string_as_126`, `reports_a_path_that_is_not_executable` | Spec is `Review`. Passed. Collides — C1 |
| FR-205 | SPEC-0004 | Not due (Implemented) | `src/main.rs:86-97` | `passes_multi_word_arguments_unchanged`, `reports_signal_deaths_as_128_plus_n`, `propagates_a_childs_own_reserved_status`, `propagates_an_unexplained_exec_status` | Spec is `Review`. All passed. Collides — C1 |
| NFR-201 | SPEC-0004 | Not due (Implemented) | `src/executable.rs:26-117` | `reports_no_failure_for_an_executable` | Spec is `Review`. Passed. Collides — C1 |
| FR-301 | SPEC-0005 | Not due (Implemented) | `src/profile.rs:74`, `src/profile.rs:97-134` | `narrows_sysctl_reads_to_an_allowlist`, `denies_a_sysctl_the_allowlist_does_not_name`, `grants_the_sysctls_ordinary_programs_read` | Spec is `Review`. All three run by exact name this session, green; the two integration tests fail against the pre-change profile |
| FR-302 | SPEC-0005 | Not due (Implemented) | `src/profile.rs:54` | `grants_process_information_only_inside_the_sandbox`, `denies_reading_another_process_environment` | Spec is `Review`. Both green by exact name this session |
| FR-303 | SPEC-0005 | Not due (Implemented) | `src/profile.rs:204` | `denies_every_ungranted_process_information_operation` | Spec is `Review`. Green by exact name |
| FR-304 | SPEC-0005 | Not due (Implemented) | `src/profile.rs:204-206` (denial precedes the `$HOME` early return) | `denies_process_information_without_a_home` | Spec is `Review`. Green by exact name |
| NFR-301 | SPEC-0005 | Not due (Implemented) | `src/profile.rs:54`, `src/profile.rs:204` | `denies_reading_another_process_environment` | Spec is `Review`. Green. The process read from is a concurrent sandme run, so the cross-run clause is the same test; the unsandboxed control leaks, which is what makes the assertion meaningful |
| NFR-302 | SPEC-0005 | Not due (Untested) | `src/profile.rs:97-115` | — | Spec is `Review`. Carries a re-runnable procedure and a recorded result (40 commands, 0 status differences, Darwin 25.6.0); no test asserts it. Deferred to `performance-review` |

### Gaps

#### G1 — SPEC-0001/FR-003 · `Untested`

**Requirement:** "**FR-003**: WHEN a sandboxed command runs a subcommand or starts a new process,
THE SYSTEM SHALL ensure that process also runs under the sandboxed environment."

**Searched:** `rg -n 'FR-003' src/ tests/` → `src/profile.rs` only, in the profile's own doc;
`git log --grep='FR-003'` → the PoC commit; the Verification row names no test, only "verified
manually".

**What exists instead:** `(allow process-exec)` and `(allow process-fork)`
(`src/profile.rs:52-53`) with no `no-sandbox` flag, so Seatbelt applies the profile to the whole
tree. `denies_subprocess_access_outside_shared_paths` (`tests/cli.rs`) proves a *child* of the
sandboxed command is confined, which is the requirement's behaviour — but the Verification row
does not name it.

**Next action:** Add `denies_subprocess_access_outside_shared_paths` to SPEC-0001's FR-003
Verification row. No code needed; this is a stale row, not a missing test.

#### G2 — SPEC-0001/FR-005 · `Untested`

**Requirement:** "**FR-005**: WHEN the user invokes sandme, THE SYSTEM SHALL start a proxy server
as a concurrent task and run the sandboxed command in a separate process, in parallel."

**Searched:** `rg -n 'FR-005' src/` → `src/main.rs:65`, `src/proxy.rs:2`. Verification row says
"Manual".

**What exists instead:** `proxy_lifetime_follows_command` (`tests/cli.rs`) exercises exactly this
and passed this session.

**Next action:** Name `proxy_lifetime_follows_command` in SPEC-0001's FR-005 Verification row.

### Conflicts

| # | Class | Specs / IDs | Detail | Resolution |
| --- | --- | --- | --- | --- |
| C1 | ID collision | SPEC-0003/FR-201…FR-205, NFR-201 vs SPEC-0004/FR-201…FR-205, NFR-201 | Both specs number their requirements in the 2xx block. `spec-driven-development.md` §4: "Requirement IDs are never reused and never renumbered." Every unqualified citation in the tree is therefore ambiguous: `rg` finds 9 × `FR-201`, 5 × `FR-202`, 13 × `FR-203`, 7 × `FR-204`, 5 × `FR-205`, 5 × `NFR-201` in `src/` and `tests/`, and exactly one of them (`src/main.rs:67`) says which spec it means. Both specs are at `Review`, so the block is still cheap to move | Renumber SPEC-0004 into an unused block (4xx) while it is still `Review`, and qualify the citations it owns as `SPEC-0004/FR-4NN`. Renumbering after `Accepted` is forbidden by §4, so this is the last moment it is free |
| C2 | Unverifiable | SPEC-0005 | The spec had no **Open questions** section. The template makes it mandatory and §3.3 requires it to be empty before `Status` leaves `Draft` — an absent section cannot be read as an empty one, and the other four specs all carry an explicit "None." **Closed after this review ran:** the section was added, reading "None." | — |
| C3 | Unapproved implementation | SPEC-0002, SPEC-0003, SPEC-0004, SPEC-0005 | §8: "MUST NOT write implementation code for a change that has no spec at `Accepted`." Four of five specs are at `Review` with their behaviour built, three of them merged to `main`. Requirements already built under a `Review` spec: FR-101…FR-104, NFR-101; FR-201…FR-206, NFR-201…NFR-202; FR-201…FR-205, NFR-201; FR-301…FR-304, NFR-301…NFR-302. No open questions were silently answered — all four record "None." (SPEC-0005 records nothing; see C2) | Either advance the four to `Accepted`/`Implemented`, or amend §3.3/§8 to describe the practice the repository actually follows: a spec at `Review` may be presented with its implementation, on a branch, and approved as one unit. Repeating this per change is what makes the guideline dead text |
| C4 | Stale verification reference | SPEC-0002/FR-101, FR-104, NFR-101 | The rows locate `gui_mode_allows_temp_writes`, `grants_the_home_library_only_in_gui_mode` and `denies_the_launchd_and_keychain_directories_after_every_grant` in `src/sandbox.rs`. All three moved to `src/profile.rs` in #37. The tests exist and pass, so coverage is intact; the pointers are wrong | Repoint the three rows at `src/profile.rs` |

No `Contradiction`, `Duplicate`, `Orphan delta`, `Unrecorded change`, `Broken supersession`,
`Premature status` or `Adjective NFR` found. Checked in particular:

- **SPEC-0005 against SPEC-0001.** SPEC-0001 states no requirement about kernel state or process
  visibility, so FR-301…FR-304 add rather than modify, and the spec's `Related specs` line says
  so. No delta section is owed.
- **SPEC-0005 against SPEC-0003.** SPEC-0003/FR-203 requires the proxy credential in the child's
  environment; SPEC-0005/NFR-301 stops another process reading that environment. Complementary,
  not contradictory — SPEC-0005 names the relationship in its Summary.
- **SPEC-0005 against SPEC-0002/NFR-101.** NFR-101 requires the FR-102 denials to be the last
  rules in the profile. SPEC-0005 adds `(deny process-info*)` *before* them
  (`src/profile.rs:204`), and `denies_the_launchd_and_keychain_directories_after_every_grant`
  still passes: the new rule is an operation rule, not a path rule, so it does not sit between a
  path grant and a path denial. NFR-101's "last rules" is about path resolution and remains true
  of the path denials.
- **Every NFR carries a number:** NFR-001 (macOS/Seatbelt), NFR-101 (rule position),
  NFR-201/0003 (128 bits), NFR-202/0003 (no connection attempt), NFR-201/0004 (filesystem only),
  NFR-301 (no process outside the instance), NFR-302 (40 commands, 0 differences).

### Unspecified behaviour

| Location | Behaviour | Requirement |
| --- | --- | --- |
| `src/profile.rs:55-70` | The blanket grants `mach-lookup`, `mach-register`, `mach-bootstrap`, `iokit-open`, `lsopen`, `ipc-posix-shm*`, `file-read-metadata` | none — SPEC-0005 declares them a non-goal and issue #12 part 4 tracks them |
| `src/app_bundle.rs:40-110` | An app-bundle CLI wrapper is redirected to the bundle's own executable, and an unreadable bundle is reported on stderr | none — behaviour from issue #13, no spec states it |
| `src/sandbox.rs:37-49` | A single operand is treated as a shell command string and routed through `/bin/sh -c` | SPEC-0001 FR-001 covers the CLI shape; the shell-string path and its first-word rewriting are unspecified |
| `src/main.rs:67` | Comment cites `SPEC-0003/FR-302`, an ID that does not exist (SPEC-0003 numbers it FR-203) | none — a wrong citation, one word to fix |

### Result

Implemented 11 · Untested 2 · Partial 0 · Missing 0 · Contradicted 0 · Not due 23 (of which 21
are `Implemented` and 2 `Untested` on approval; none becomes `Missing`) · Conflicts 4 — nothing
in `docs/specs/` is unimplemented or contradicted by the code, and the four conflicts are all
bookkeeping: one ID block to move before it is frozen (C1), one missing section in SPEC-0005
(C2, since closed), the standing `Review`-with-code practice to settle once (C3), and three
stale file pointers (C4).
