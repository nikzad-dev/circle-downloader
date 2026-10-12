# Circle Downloader — implementation audit and next roadmap

Audited **2026-10-10**, against repository commit **`9d93789`** and the supplied `circle-downloader-blueprint` design pack.

**Starting audit position (2026-10-10):** a functional desktop MVP UI, single-connection transfer engine, SQLite persistence and basic task controls. Parts of blueprint Phase 2 already exist. Transfer correctness, scheduling and recovery remain incomplete against the full blueprint.

The blueprint is a target design, not implementation evidence or a command to execute every proposal. Its checked boxes and developer-reported logs are not treated as verified results. Implementation has now started in N0/N1. Checked items below include source and test evidence; unchecked items remain planned work.

The verified baseline and native smoke record are preserved in the [consolidated verification record](docs/VERIFICATION.md).

## Blueprint phase comparison

| Blueprint phase | Actual position                              | Remaining                                                                                            |
| --------------- | -------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| 0 — Foundation  | Mostly implemented                           | Local and remote macOS CI verified; wider platform/release checks remain N5 (GPL-3.0-or-later added) |
| 1 — Useful MVP  | Everyday UI complete on macOS baseline       | Wider accessibility/release QA remains N5                                                            |
| 2 — Durable     | N2 queue/retry/shutdown implemented on macOS | Windows/Linux tray/release QA                                                                        |
| 3 — Advanced    | Not implemented                              | Real scheduler, segmentation, advanced list/settings/localization                                    |
| 4 — Desktop 1.0 | Not implemented                              | Browser integration, signed installers/updater, cross-platform release QA and public docs            |
| 5 — Expansion   | Deferred                                     | Media, torrents, mobile, remote companion and plugin SDK                                             |

No percentage is assigned: a screen, stored setting and enforced engine behavior represent different levels of completion.

## What we have implemented

| Capability                                | Status                  | Source evidence and boundary                                                                                          |
| ----------------------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Tauri/React, Router/Query, local oRPC/Zod | Implemented             | `src/components/providers.tsx`, `src/lib/orpc/*`, `src/lib/tauri/*`; Rust owns transfers, no separate server          |
| Shell and main routes                     | Implemented             | `src/components/app-shell.tsx`, `src/routes/*`; Home, Downloads, minimal Settings, Health                             |
| Existing shadcn design                    | Implemented             | Base UI primitives, neutral/lime tokens, Figtree/Geist; System/Light/Dark persistence and OS listener                 |
| Add Download                              | Implemented             | Validated URL/name, destination Select, Start now, request ID, pending/error/focus handling                           |
| Table and task actions                    | Implemented             | Search/filter/sort/bulk; pause/resume/cancel/retry/open/reveal/remove; loading/empty/error states                     |
| Live progress                             | Implemented             | Single disposable Channel bridge, revision-safe snapshots/events, batching, failure refresh, StrictMode cleanup tests |
| Home summaries                            | Implemented             | Actual active/completed/failed counts and recent tasks/failures                                                       |
| Streamed single-connection transfer       | Implemented with limits | HTTP body streams to disk; resumed-file SHA-256 uses a bounded 64 KiB buffer                                          |
| Temp-file completion                      | Partial                 | Reserved saved-task names and atomic no-replace publication; full crash recovery still pending                        |
| SQLite task/settings persistence          | Implemented through v4  | Ordered migrations, backup/restore, WAL/busy_timeout; queue/retry + destinations                                      |
| Task controls and deferred start          | Implemented             | Deferred starts persist Paused; controls drain writers; create/resume/retry go through the dispatcher                 |
| Restart recovery                          | Implemented baseline    | Active→Failed; paused retained; publication checkpoints + queue re-dispatch after reconcile                           |
| Settings APIs                             | Implemented             | Settings feature: appearance, auto-start, concurrency, default destination; retries/host caps read-only               |
| Queue API                                 | Durable dispatch        | Default queue membership/order, global + per-host fairness; list still reports non-terminal counts                    |
| Destination grants                        | Implemented             | Schema v4 opaque folder grants via dialog; resolved in Rust with N1 no-follow / no-replace writes                     |
| Search/filter/sort/bulk/details           | Implemented             | Typed Router search, bulk actions, deep-linked Sheet; virtualize ≥100 filtered rows                                   |
| Open/reveal downloaded file               | Implemented             | Path-validated Rust open/reveal; UI on completed/partial terminal states                                              |

- [x] Inspect blueprint requirements against current source.
- [x] Distinguish complete features, partial behavior and scaffolding.
- [x] Preserve the previous UI checklist.
- [x] Re-run tests and inspect build/formatter/Clippy baseline.
- [x] Define ordered future phases with tests and exit criteria.

## Verification baseline

| Check                                       | Current result                                                                                                                                                 |
| ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bun run test`                              | Passed: **81 tests in 16 files**                                                                                                                               |
| `bun run test:rust`                         | Passed: **86 passed; one subprocess fixture invoked by parent tests**                                                                                          |
| Rust formatter check                        | Passed                                                                                                                                                         |
| Strict Clippy, all targets, warnings denied | Passed: all targets, warnings denied; the seven audit diagnostics are resolved                                                                                 |
| Frontend lint                               | Passed after formatting the new roadmap and archived UI checklist; initial audit found Markdown formatting only                                                |
| Production build                            | Passed: TypeScript + production Vite build, no chart CSS warnings                                                                                              |
| Native smoke                                | Prior session: isolated macOS debug app, local fixtures, pause/resume/cancel/retry, restart persistence, long names, 800×600 dialog, byte-identical completion |
| Release/accessibility matrix                | Incomplete; previous macOS smoke does not verify Windows/Linux, signing or a full accessibility audit                                                          |

Passing tests prove the covered cases, not the complete blueprint network/failure matrix. The earlier workflow smoke is historical; a new isolated native Health check returned `Status: ok` during N0. See the [consolidated verification record](docs/VERIFICATION.md). The latest local gates cover the current changes; remote CI evidence covers the earlier commit `d63fd61`.

## Gaps found in the original audit

This table preserves the starting audit; N1 checkboxes and the verification record describe fixes made since then. These findings explain the recommended order. Potential race outcomes need dedicated reproduction tests; they are not all claimed as reproduced defects.

| Priority | Gap                                                 | Evidence / implication                                                                                                                               |
| -------- | --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| P0       | Resume accepts 206 without validating Content-Range | `infrastructure/http.rs` appends without proving offset/range/representation; rejection of Range → 200 alone is insufficient                         |
| P0       | Validators saved only after successful transfer     | ETag/Last-Modified return at EOF and are persisted later; paused/failed first attempts may lack the validator needed for safe resume                 |
| P0       | Final path allocation is not reserved               | `unique_final_path` checks existing files; concurrent tasks can choose the same path, and rename may replace another file on some platforms          |
| P0       | Worker lifecycle is not fully coordinated           | Cancel removes a handle without awaiting worker exit; immediate resume can launch another worker. Cancellation is checked after a body chunk arrives |
| P0       | Concurrency setting has no enforcement              | Addressed in N2: dispatcher enforces global + per-host caps                                                                                          |
| P1       | Resumed verification can buffer the full file       | Addressed in N1: bounded 64 KiB streaming hash                                                                                                       |
| P1       | Crash durability/finalization incomplete            | Addressed in N1: ready/cleanup checkpoints + recovery tests                                                                                          |
| P1       | Migrations are schema initialization                | Addressed in N2: ordered v1→v3 runner with backup/WAL/contention tests                                                                               |
| P1       | Native/network access policy incomplete             | Initial URL check, redirect count limit, CSP null, default permissions; no per-hop/DNS policy, destination grant registry or symlink-safe writer     |
| P1       | Manual retry only; waits not explicitly bounded     | N1 added deadlines; N2 added retry budget/backoff/Retry-After. Some failures may still map broadly                                                   |
| P1       | Destination IDs do not resolve approved locations   | Manager accepts an ID but writes to the same app folder; settings accepts nonempty IDs without verifying a grant                                     |

## New roadmap — dependency order

Keep the existing Rust-authoritative architecture, feature folders, Query cache and shadcn theme. These N0–N6 labels are the new work sequence; original blueprint numbering is mapped above.

### N0 — repeatable quality baseline

**Priority: immediate. Dependency: none.**

- [x] Resolve strict Clippy without global suppressions; add explicit typecheck, Rust fmt and Clippy scripts.
- [x] Add CI using frozen Bun/Cargo lockfiles, frontend lint/build/tests and Rust fmt/Clippy/tests. Permit controlled localhost fixtures and expand supported desktop hosts progressively.
- [x] Replace the template README with setup, usage, app-data locations, isolated QA, actual URL/destination/recovery limitations and verification commands. Completed 2026-10-10; checked against source/configuration and documentation checks.
- [x] Review runtime/database ignore rules consistently for future custom data directories and fixture output.
- [x] Add a Rust-to-TypeScript DTO contract test or binding strategy; retain safe-integer byte transport bounds.
- [x] Record real native system.health → ok verific131ation. Native QA returned `Status: ok`, 2026-10-10.
- [x] Review dependency license metadata and add owner-selected GPL-3.0-or-later. See [dependency review](docs/DEPENDENCIES.md); release SBOM/notices remain N5.
- [x] Confirm the workflow passes on a fresh remote CI checkout. Verified 2026-10-11: [Quality run #38071582041](https://github.com/nikzad-dev/circle-downloader/actions/runs/38071582041), exact commit `d63fd61`, GitHub-hosted `macos-14`, every required gate passed. See [remote CI evidence](docs/VERIFICATION.md#1-quality-baseline--remote-ci-n0).

**Tests / exit:** every baseline gate passes from a fresh checkout and CI reproduces it. Record failures honestly; a running debug app is not release verification.

N0 implementation evidence: pinned actions/toolchains, frozen install, strict gates and shared JSON DTO tests; [consolidated verification record](docs/VERIFICATION.md). All N0 checklist items are complete for the current macOS baseline.

### N1 — transfer correctness and file safety

**Priority: P0. Dependency: existing engine and N0 checks.**

- [x] Coordinate in-process lifecycle transitions and worker ownership: one writer per task, stop/await before resume/retry/remove, and guard against late writes/status changes.
- [x] Interrupt response/body/verification waits on cancellation; response headers and body inactivity have 30-second deadlines. Flush pending file writes before the worker exits.
- [x] Prevent two app processes from owning the same data directory/tasks. OS-held leases precede database startup and follow manager/worker ownership; normal exit, killed-owner recovery and blocked reconciliation are tested. Completed 2026-10-11; see [verification record](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1).
- [x] Finish actionable HTTP/TLS/storage error classification and automated deadline/verification-cancellation coverage. Structured redacted errors, HTTP retryability, real TLS rejection, header/body deadlines and stalled/always-ready hashing cancellation are tested. Completed 2026-10-11; see [verification record](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1).
- [x] Persist accepted response validators before body checkpoints. Validate Content-Range start/end/total, length, encoding and stable identity. Require a syntactically valid strong ETag; Last-Modified is saved but not trusted for resume. When safety cannot be proven, leave the partial intact and direct the user to add a fresh download.
- [x] Reserve final names across active tasks; use no-replace finalization unless overwrite was explicitly authorized. Handle external files appearing during a transfer.
- [x] Stream full-file SHA-256 calculation in a bounded 64 KiB buffer; test full resumed-file identity.
- [x] Retain actual SHA-256/result and compare an expected checksum when supplied. The UI distinguishes calculated, verified and mismatched results; length checks use a separate error code. Completed 2026-10-11; see [verification record](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1).
- [x] Define synced partial / ready-to-publish / completed-cleanup checkpoints and durability policy. Recover publication-before-DB-commit, both-name/final-only states and partial/final inconsistencies without overwriting conflicts. Completed 2026-10-11; see [protocol and crash-state tests](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1).
- [x] Enforce public HTTPS, per-hop and transport DNS/address policy in Rust; redact signed-query/auth data; restrict CSP/capabilities and anchor no-follow writes/publication/removal to a retained directory. Loopback fixtures are debug-only; HTTP/LAN remains disabled in release. Completed 2026-10-11; see [security tests and limits](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1).

**Tests:** malformed/missing/wrong-offset 206, changed/missing/weak validators, Range → 200, 416, truncated/unknown-length bodies, zero-byte/random binary files, stalled cancellation, immediate pause→resume, retry/remove races, same-name concurrent creates, symlink/path replacement and finalization crash points. Compare resulting full-file SHA-256 with fixtures; measure resumed-file peak memory.

**Exit:** incompatible bytes cannot silently append, no unintended overwrite or competing writers, cancellation is bounded and verification does not scale RAM with file size. N1 implementation checklist is complete for the covered macOS baseline; large resumed-file peak-RSS measurement and Windows/Linux durability/security validation remain verification gaps. See [verification record](docs/VERIFICATION.md#2-transfer-correctness-file-safety--integrity-n1). Retain these platform and large-file verification gates for release; N4a has local integrity and bounded-buffer coverage.

### N2 — durable queue and recovery

**Priority: P0/P1. Dependency: N1 lifecycle and persistence protocol.**

- [x] Extend the transactional v1→v2 checksum migration into an ordered migration framework with backup/recovery policy and tested SQLite contention behavior. Schema v3 queue/retry columns; WAL + busy_timeout; `.bak` before upgrade. Completed 2026-10-11; see [verification record](docs/VERIFICATION.md#3-durable-queue-persistence--process-leases-n2).
- [x] Introduce bounded dispatch enforcing global concurrency; persist queue membership/order/queued intent and add per-host fairness (cap 2). Completed 2026-10-11.
- [x] Route create/resume/retry through the coordinator; retain user-paused intent and reconcile actual files on restart. Completed 2026-10-11.
- [x] Add retry budget (5), exponential backoff/jitter and Retry-After behavior. Do not retry permanent failures; expose `retryCount` / `nextRetryAt` honestly. Completed 2026-10-11.
- [x] Define real quit vs window close: hide-to-tray on close; Quit cancels/drains then exits. macOS dock reopen shows the window. Windows/Linux tray smoke remains a verification gap. Completed 2026-10-11.
- [x] Handle disk/permission failures and partial deletion errors consistently; concurrent request-ID retries return the original task rather than an internal constraint error. Completed 2026-10-11.

**Tests:** jobs exceeding global/per-host limits, fairness and slot release, paused intent, reopen/migration from version 1, kill/restart checkpoints, missing/truncated files, retry timing/budgets, storage failures and simultaneous idempotent creates.

**Exit:** bounded workers, durable queue order/user intent and tested recovery for each finalization stage. N2 checklist complete for the covered macOS baseline; see [verification record](docs/VERIFICATION.md#3-durable-queue-persistence--process-leases-n2).

### N3 — complete the everyday UI/UX

**Priority: P1. Dependencies: N1 safe paths; N2 for scheduler/settings behavior. Search/details/accessibility can proceed independently sooner.**

- [x] Replace the sidebar with the existing shadcn Menubar, visible top navigation, current-page indication and View appearance controls. Preserve global Add/paste shortcuts and dialog focus return. See [verification record](docs/VERIFICATION.md#4-everyday-desktop-ui--navigation-n3).
- [x] Add search, status filters and explicit sort with typed Router search params; progress must not reorder rows automatically. Add bulk selection/actions with per-task results. Completed 2026-10-11; see [verification record](docs/VERIFICATION.md#4-everyday-desktop-ui--navigation-n3).
- [x] Add task details Sheet on `/downloads/$downloadId`: redacted source, destination, errors, retries and verification data. Completed 2026-10-11.
- [x] Add native folder grants returning opaque IDs, resolve them in Rust, and show approved destination/collision policy in Add and download settings. Schema v4. Completed 2026-10-11.
- [x] Add explicit Open file/Reveal folder actions with native path/existence validation. Completed 2026-10-11.
- [x] Move settings pages into a feature and expose General/Downloads sections, auto-start, concurrency, default destination and read-only retry/host policy. Completed 2026-10-11.
- [x] Add global Add shortcut/paste flow without unsolicited clipboard reads; display only working shortcuts. Improve interrupted-transfer recovery copy. Completed 2026-10-11.
- [x] Centralize strings and locale formatting; keyboard/SR roles, contrast, zoom and reduced-motion smoke across Light/Dark. Full VoiceOver/Windows matrix remains N5. Completed 2026-10-11.
- [x] Benchmark 1,000 synthetic tasks for filter/sort stability; virtualize Downloads at ≥100 filtered rows. Preserve byte batching; Home stays capped. Completed 2026-10-11.

**Tests:** filter/sort stability (including 1k), selection/Open actions, destination grants/open-path checks, settings clamp/unknown destination, shortcut and a11y role coverage. Record native CPU/memory/frame-time measurements separately from deterministic batching tests.

**Exit:** users can choose a folder, find/manage tasks, inspect failures, open results and configure working preferences with the existing coherent shadcn design. N3 checklist complete for the covered macOS baseline; see [verification record](docs/VERIFICATION.md#4-everyday-desktop-ui--navigation-n3).

### N4 — advanced performance and organization

**Priority: P1; gated by N1/N2 correctness and measured demand.**

- [x] Segmentation for compatible origins only: stable validators, identity encoding, verified range coverage, bounded connections and single-connection fallback. N4a completed 2026-10-11; default one connection, opt-in 1–4 for compatible files ≥8 MiB.
- [x] Persist segment extents/checkpoints, verify assembly and benchmark controlled acceleration before making speed claims. N4a completed 2026-10-11; restart/cancellation tests, seven paired benchmark samples and native checksum smoke recorded in [verification record](docs/VERIFICATION.md#5-multi-segment-transfers-n4a).
- [x] Add rate limits, schedules/time-zone behavior, priorities and real queues UI. N4b verified 2026-10-11.
- [x] Add categories/tags and bulk URL import. Add proxy/network settings and OS credential storage with scoped authorization/redaction tests. Isolated macOS vault roundtrip verified.
- [x] Add statistics from actual persisted measurements; no sample graphs in normal product flows. Durable attempt measurements and real empty states verified.

**Tests:** range coverage, out-of-order segments, changed origin, segment restart, fairness/rate limits, DST scheduling, proxy/TLS failures and large-history performance. Report repeated benchmark medians/variance under equivalent conditions.

**Exit:** measurable advanced behavior that preserves integrity; acceleration stays optional. Covered macOS implementation verified in [verification record](docs/VERIFICATION.md#6-advanced-controls--organization-n4b), including seven repeated benchmark samples/medians/variance; cross-platform release verification remains N5.

### N5 — supported desktop release

**Priority: release gate. Platform smoke/build work begins during N0–N3. Optional N4 features need not delay a smaller advertised release.**

- [ ] Verify Windows/macOS/Linux installers on actual supported platforms; define OS versions, filesystem cases, close/quit and upgrade/uninstall behavior.
- [ ] Sign/notarize where required; add signed updater validation/recovery and keep signing secrets outside source control.
- [ ] Add browser integration with documented capture limits, consent/host permissions and safe URL/header/cookie handling; no universal interception promise.
- [x] Finish license/dependency review, SBOM, secret/vulnerability scans, private disclosure contact, contributing/issue templates and release notes. Completed 2026-10-12; SPDX 2.3 SBOM generator, CHANGELOG, CONTRIBUTING, issue/PR templates, and release workflow added.
- [x] Add [SECURITY.md](SECURITY.md) with supported-version policy, private reporting guidance, current security boundaries and verification limits. Completed 2026-10-12; dedicated disclosure channels and security architecture boundaries documented.
- [ ] Add repeatable native smoke automation where supported; complete external HTTPS, actual removal, accessibility, installer and update QA. Separate mocked UI checks from native evidence.

**Tests:** clean install/upgrade, artifact signatures, rejected/tampered updates, native workflow on each OS, filesystem/collision cases and browser permission/redaction behavior.

**Exit:** publish only verified platform/feature combinations. A macOS debug bundle is not a cross-platform production release.

### N6 — optional expansion

**Priority: deferred until desktop quality is established.**

- [ ] Media adapter and reviewed sidecar licensing/distribution.
- [ ] Torrents/Metalink and additional protocols.
- [ ] CLI or authenticated headless/LAN companion when a second engine consumer is needed.
- [ ] Mobile-native scheduling/permissions with OS-specific background limits.
- [ ] Versioned plugin SDK/custom actions with a clear permission boundary.

**Tests / exit:** independently review/test each adapter, permissions and distribution. No default remote listener or automatic execution of downloaded files; do not imply desktop background guarantees on mobile.

## UI/component rules for each phase

Reuse existing Base UI shadcn primitives and semantic tokens before adding wrappers. Preserve default System appearance, Light/Dark support, typography, square shapes and neutral/lime styling. Put business composites in features, not primitives or route files.

Every screen needs honest loading/empty/error/pending states. Statuses include text, icon actions have names, and progress is accessible without byte-by-byte announcements. Use optional columns/details for advanced data; show unknown totals honestly and add ETA only after a tested estimator exists.

Keep Rust authoritative, Query as metadata cache and local/router state for UI. Avoid a second task store, unnecessary server, empty future routes or a theme rewrite.

## Next slice and definition of done

**Current work:** N0–N3 are implemented and locally verified for the covered macOS baseline. N4 optional segmentation and advanced queue/organization/network/statistics controls are implemented and locally verified. See the [consolidated verification record](docs/VERIFICATION.md). Next: finish N5 release gates. Windows/Linux durability and native QA, large-file peak-RSS measurement, a private disclosure contact and remote CI for the current changes remain unverified. Earlier remote macOS CI covers commit `d63fd61`.

For each slice:

1. Define one concrete outcome and its engine invariants.
2. Implement the smallest coherent change using existing architecture/components.
3. Test success, failure, cleanup and applicable concurrency/restart behavior.
4. Run frontend lint/build/tests and Rust fmt/Clippy/tests; use isolated data for native QA.
5. Check completed tasks here and attach dated source/test evidence. Keep partial APIs distinct from complete user workflows.

Phases are dependency order, not calendar promises. Track implementation, native and release verification separately.

## Reference inputs

Read from `/Users/circle/Downloads/circle-downloader-blueprint/`: docs/ROADMAP, PRODUCT, UI_UX, ARCHITECTURE, API, DATA_MODEL, SECURITY, TESTING, DEVELOPMENT, REPOSITORY and ADR-0001.

Where the pack disagrees on release timing (UI_UX labels bulk/search MVP, PRODUCT stages them later), this plan puts engine correctness first and places those list tools in N3.

Resume planning follows [RFC 9110 range semantics](https://www.rfc-editor.org/rfc/rfc9110.html#section-14). Native hardening uses [Tauri security guidance](https://v2.tauri.app/security/); release update planning uses [Tauri updater signature requirements](https://v2.tauri.app/plugin/updater/). These are design references, not claims that the features already exist.

### Link drag-and-drop (2026-10-12)

Implemented external browser-link and URL-text drops in the main workspace with an overlay, copy feedback, URI-list/Firefox parsing, deduplication, import limits and existing download-window draft protection. Invalid drops cannot navigate the workspace. Inputs, dialogs and internal drags retain their behavior. Evidence: `dropped-links.test.ts`, `use-download-drop.test.tsx`, and shell integration tests. Frontend build and tests verified; physical cross-app drops on Windows/Linux remain part of release QA.

### Native progress window (2026-10-12)

Implemented a reusable toolbar progress window with an active-transfer count, automatic opening for immediately started downloads/imports, live transfer cards, aggregate speed, state-aware actions, remaining-time estimates and recent completed history. Hidden windows dispose their IPC progress listener and polling; reopening refreshes and reconnects. Evidence: progress-window UI/lifecycle/API tests, transfer-summary tests, toolbar and download-creation tests. Verified frontend build, 164 frontend tests, 130 Rust tests, strict Clippy, debug macOS app packaging, light/dark/minimum-width rendering, and a real isolated localhost transfer with pause/resume and close/reopen continuity. Windows/Linux native smoke remains release QA.

### Tray progress shortcut (2026-10-12)

Added an event-driven Download progress tray item between Show and Quit while transfers are active. It reuses the native progress window and works with hidden app windows. Revision-aware tracking ignores stale events and keeps the shortcut until the last active transfer stops. Evidence: native tracker tests for concurrent transfers, pause/resume, verification, completion, cancellation, failure and stale revisions.

### Paused queue guidance (2026-10-12)

Progress cards identify a paused queue and offer Resume queue while retaining its concurrency and bandwidth settings. Public download payloads include the queue ID, with the shared IPC fixture updated. Unknown totals use one readable size-pending message; queued tasks show a stationary progress track without empty speed/ETA fields. Verified 188 frontend tests, 135 native tests and frontend build; lint has no errors and two existing library warnings.
