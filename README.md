# Circle Downloader

A local desktop download manager built with Tauri, Rust, React and shadcn/ui. Rust owns transfers and persisted tasks; the interface shows real status, bytes and speed through native progress channels.

**Status: development MVP, version 0.1.0.** The core workflow is implemented. Supported-platform durability, security/release QA and installer verification remain before a production release. See the [implementation audit and roadmap](ROADMAP.md) for completed work, known gaps and the next phases.

## Current features

- Compact shadcn Pages/Appearance menubar and resizable sidebar with accordion sections for transfer states, queues, categories and tags.
- Home overview with real transfer counts, recent downloads and failures.
- Durable queues with per-queue concurrency/rate caps, round-robin dispatch, aged priorities and one-shot schedules with explicit IANA time zones and DST handling.
- Categories/tags, filtered history and retry-safe bulk URL import (up to 200 unique lines).
- Explicit proxy/timeout settings and scoped credentials stored in the OS credential vault.
- Statistics from durable transfer measurements; empty history shows an empty state.
- Separate compact download window with Single/Multiple URL modes, visible destination, optional organization/scheduling and additional features revealed on demand.
- Downloads table with search/filter/sort, bulk actions, progress, bytes, speed and task actions.
- Task details Sheet via `/downloads/$downloadId` (redacted source, retries, verification).
- Pause, resume, cancel, manual retry, Open/Reveal and removal confirmation.
- Native folder grants (opaque IDs) plus the always-available app-managed default folder.
- Settings: appearance, auto-start, task concurrency, optional connections per download and default destination; retries/host caps are read-only.
- Global shortcuts: ⌘/Ctrl+N Add download; ⌘/Ctrl+V pastes a URL into Add when focus is not in an editable field.
- SQLite task/settings/destination persistence, exclusive process ownership and verified publication recovery.
- Retained SHA-256 digests with optional expected-checksum comparison.
- Rust-enforced public HTTPS, safe redirect/DNS policy and anchored no-follow file operations.
- Light, Dark and System appearance using the existing neutral/lime shadcn theme.

Downloads use one connection by default. Settings can opt into 2–4 connections for compatible files of at least 8 MiB; incompatible origins fall back to one connection. Browser integration and full cross-platform release QA remain planned.

## Development setup

Install Bun, Rust 1.91.0 with Cargo, rustfmt, Clippy, rust-analyzer and rust-src (pinned by `rust-toolchain.toml`), and Node.js compatible with Vite 8 (20.19+ or 22.12+ within its supported version ranges). Native development also needs the [Tauri 2 prerequisites for your operating system](https://v2.tauri.app/start/prerequisites/): Xcode Command Line Tools for desktop macOS, the required WebKitGTK/build packages on Linux, or Microsoft C++ Build Tools and WebView2 on Windows.

From the repository root:

```sh
bun install --frozen-lockfile
bun run tauri dev
```

Tauri starts Vite and the native app together. Vite uses port **1420** with strict port checking; stop another process using that port if startup fails. Keep `bun.lock` and `src-tauri/Cargo.lock` in version control. Use Bun for the documented frontend workflow.

For browser-only layout work:

```sh
bun run dev
```

The browser has no Tauri runtime, so native download/health calls will fail there. Use the native app for real transfers; UI tests mock IPC explicitly.

### Isolated native development

Debug builds support `CIRCLE_DOWNLOADER_DATA_DIR` to keep QA downloads and the database separate from your regular app data. For macOS/Linux shells:

```sh
circle_qa_dir="$(mktemp -d)"
CIRCLE_DOWNLOADER_DATA_DIR="$circle_qa_dir" bun run tauri dev
```

Reuse that directory to test persistence across restarts. On Windows, set the same environment variable to a fresh temporary directory before launching development. The override is excluded from release builds. The app and any fixture server must be stopped before removing their test data.

## Using the app

1. Open the separate **Add download** window from the toolbar, Home, or ⌘/Ctrl+N (⌘/Ctrl+V outside text fields detects one or multiple clipboard URLs and prefills the matching mode). Reopening focuses the existing window and preserves its draft.
2. Paste a direct public HTTPS file URL without embedded credentials or fragments. Private/special-use addresses, unsafe redirects and remote plain HTTP are rejected. Debug builds additionally accept loopback fixture links; release builds do not enable HTTP/LAN access.
3. Choose an approved destination. Enable **Organize download** for queue/priority/category/tags or **Schedule start** for a timed start. **More options** contains optional filename, saved credentials, multiple connections and a trusted 64-character SHA-256 checksum. Circle compares the complete file with that checksum before publishing; without one it reports a calculated digest rather than verified integrity. Leave **Start automatically** selected to begin, or clear it to save the task as Paused and choose **Resume** later. A disabled backend auto-start setting also saves it paused.
4. Follow progress in **Downloads**. Use search, status and sort controls; select rows for bulk Pause/Resume/Retry/Cancel/Remove. Open a row (or deep-link `/downloads/$id`) for details. Failed tasks offer manual **Retry**. Completed tasks offer **Open** and **Reveal**.
5. Choose **Remove** to remove a record. The confirmation offers partial-file deletion for unfinished tasks, enabled by default. Completed files remain on disk.

Appearance, auto-start, task concurrency (1–32), connections per download (1–4) and the default destination are editable in Settings. Connection changes apply when a transfer starts or resumes; existing single-connection partials keep their original mode, and existing segment plans resume with the current connection cap. Approve extra folders there; Circle never replaces an existing file when naming collisions occur. Automatic retries (up to 5) and the per-host limit (2) are shown read-only.

### Queues, schedules and network controls

Open **Pages → Queues** to create queues and change their worker/rate limits. Rate fields use KiB/s; zero means unlimited. Global limits in Settings cover every queue and connection. Pausing a queue blocks new starts; active transfers finish. To stop an active transfer, pause that download. Queues rotate fairly; within each queue priority ranges from Lowest to Highest, with waiting time preventing starvation.

Pause a download and open its details to change its queue, priority, category, tags, schedule or credential grant. **Schedule start** records a local date/time, IANA time zone and explicit earlier/later choice for repeated DST times. Nonexistent local times are rejected. Saved schedules preserve their UTC instant; queued overdue work starts on reopen. The app must be running to start at the scheduled time; it does not wake the OS or run while quit. Changing controls does not automatically resume a paused download.

**Import URLs** opens the same window in Multiple URLs mode and accepts one URL per line, deduplicates lines and applies shared controls. Successful entries persist; retrying an unchanged failed batch does not duplicate or restart existing work. Auto-start settings still apply. Imports use the default approved destination.

Settings accepts an explicit HTTP/HTTPS proxy origin without embedded credentials, plus a connection timeout. System/environment proxies are ignored. Proxy/timeout changes and credential revocation require paused transfers. TLS verification stays enabled; an explicitly configured proxy is trusted to resolve destinations.

Authorize credentials for one exact HTTPS download origin or one proxy origin, then select the opaque grant. Usernames/passwords are stored in the OS credential store, never SQLite. Credential-bearing downloads reject redirects to a different origin; proxy credentials belong only to proxy authentication. Failed vault access fails closed with a redacted error.

**Pages → Statistics** shows body bytes actually received during completed, failed and interrupted attempts, including retries, and elapsed transfer-worker time. It excludes probe bodies and protocol overhead. Worker time includes throttling and verification, is summed across workers, and is not wall-clock usage. Totals survive task removal; the latest 30 days with activity use UTC. Measurements commit when a worker stops; forcibly terminating a process can lose its current uncommitted measurement. No sample graphs appear in product flows.

Local regression and repeated benchmark evidence: [N4b verification](docs/verification/N4B_ADVANCED_2026-10-11.md).

### Where files and history live

By default, the app uses the platform data directory followed by `circle-downloader`:

| Platform | Default app data root                                                     |
| -------- | ------------------------------------------------------------------------- |
| macOS    | `~/Library/Application Support/circle-downloader`                         |
| Linux    | `$XDG_DATA_HOME/circle-downloader`, or `~/.local/share/circle-downloader` |
| Windows  | `%APPDATA%\circle-downloader`                                             |

The root contains `circle-downloader.db`, `.circle-downloader.lock` and a `downloads/` directory. Partial files have task-specific `.part` names in that directory. This is the app-managed folder, not the operating system's Downloads folder. Debug runs with the data-directory override use the specified root instead.

Only one app process can own a data root. A second process exits with a message to close the current owner, before opening the database or reconciling its tasks. The operating system releases ownership when the process exits, including after a crash. The lock file stays on disk; do not delete it while an app process is running. Separate data roots can run independently.

The database stores task URLs and history locally. Treat it as private data when sharing diagnostics, particularly for signed links or URLs containing tokens.

## Verification

Run from the repository root:

```sh
bun run typecheck
bun run lint
bun run build
bun run test
bun run test:rust
bun run fmt:rust
bun run clippy:rust
```

`build` runs TypeScript and creates the production frontend. `test` runs Vitest's Node and jsdom projects; `test:rust` runs native tests, including controlled localhost HTTP fixtures and temporary files. The Unix TLS test fixture also needs the `openssl` command to generate a temporary self-signed certificate; production TLS uses Rustls and does not need that command. Restricted sandboxes must permit fixture socket binding and subprocess execution. Use `bun run test`, not `bun test`, for this suite. `bun run test:watch` enables frontend watch mode.

The 2026-10-11 N4a local checks passed **97 frontend tests** and **107 Rust tests**, plus lint, typecheck, production build, Rust formatting and strict Clippy. Two Rust tests are ignored by the ordinary suite: the ownership subprocess fixture invoked by its parent tests, and the controlled segmentation benchmark run separately. A macOS native debug smoke completed an 8 MiB segmented transfer with a matching expected SHA-256 and cleaned checkpoints/partials. See the [N4a decisions, tests and measured benchmark](docs/verification/N4A_SEGMENTATION_2026-10-11.md), [N3 everyday UI verification](docs/verification/N3_EVERYDAY_UI_2026-10-11.md), [N2 durable queue verification](docs/verification/N2_DURABLE_QUEUE_2026-10-11.md), [ownership/error verification](docs/verification/N1_OWNERSHIP_ERRORS_2026-10-11.md) and [checksum/recovery/security verification](docs/verification/N1_CHECKSUM_RECOVERY_SECURITY_2026-10-11.md). Full accessibility and Windows/Linux release QA remain pending.

### Native builds

```sh
# Native debug build, using the isolated-data override when launched for QA
bun run tauri build --debug

# Native release build and configured platform bundles
bun run tauri build
```

Frontend output goes to `dist/`; native build/bundle output goes under `src-tauri/target/`. Build on the intended platform with its prerequisites. Signing, notarization, updater and production installer verification are not configured as completed release features.

## Architecture and source layout

```text
React features → TanStack Query → local oRPC/Zod → Tauri commands
                                                → Rust DownloadManager
                                                → HTTP + files + SQLite
Native progress Channel → revision-aware batched Query cache updates
```

The oRPC facade runs locally in TypeScript; it is not a Rust HTTP endpoint or an authorization boundary. Sensitive validation and transfer behavior belong in Rust. Route navigation does not own transfer workers. Closing the window hides to the tray and keeps transfers running; Quit (tray or app) cancels workers, drains them, then exits.

| Path                              | Purpose                                                            |
| --------------------------------- | ------------------------------------------------------------------ |
| `src/routes/`                     | Route configuration and page entry points                          |
| `src/features/downloads/`         | Downloads API/cache, forms, actions, progress bridge and pages     |
| `src/components/ui/`              | Editable shadcn Base UI primitives                                 |
| `src/components/theme/`           | Shared appearance state and controls                               |
| `src/lib/orpc/`, `src/lib/tauri/` | Typed local facade and native adapters                             |
| `src-tauri/src/application/`      | Task lifecycle and worker coordination                             |
| `src-tauri/src/domain/`           | Models, transitions, errors and URL/filename policies              |
| `src-tauri/src/infrastructure/`   | HTTP streaming, error classification, SQLite and process ownership |
| `src-tauri/src/ipc/`              | Progress channel hub                                               |
| `src-tauri/src/api_tests.rs`      | Native workflow and HTTP fixture tests                             |

## Current limitations and contributing

Resume requires matching strong ETag and valid Content-Range metadata before appending. Servers without a strong validator require a fresh download after interruption. Controls wait for the old writer; response/header inactivity is bounded to 30 seconds. Saved tasks reserve their final names and completion never replaces an external file; hard-link publication requires filesystem support.

Environment/system proxy use is disabled so the native address checks cannot be bypassed; explicit proxy support needs a separate reviewed policy. Downloads are anchored to a retained directory with no-follow file operations, and the webview CSP allows only app assets and IPC (plus the Vite connection in development).

HTTP, TLS, connection and storage failures have structured codes and actionable UI messages without exposing signed URLs or internal paths. Certificate failures require correcting the clock or server TLS configuration; certificate validation remains enabled. Transient connection/server/storage failures are marked retryable and auto-scheduled up to five attempts with exponential backoff/jitter (and Retry-After when present); permanent failures are not auto-retried. Scheduled retries expose `retryCount` and `nextRetryAt`.

Configured task concurrency is enforced globally (settings 1–32) with a per-host task fairness cap of two. Each segmented task still occupies one task slot and can use up to four connections internally; the host cap counts tasks, not connections. Segmented assembly streams through a bounded buffer, verifies contiguous coverage and hashes the complete file before publication. Separate segments plus the assembled partial temporarily require about twice the file size on disk. Queue membership and order are durable. Approved destination folders are opaque grants resolved only in Rust; writes stay no-follow and never replace an existing final file. Publication uses a synced partial and durable ready/cleanup checkpoints; startup verifies recorded digests and reconciles partial/final files. Full-file hashing uses a bounded 64 KiB buffer; digests and expected-checksum results are retained. Startup rehashing can take time with a large history. Downloads virtualizes at 100+ filtered rows; Home stays capped. Windows directory-entry power-loss durability, Windows/Linux tray smoke and cross-platform release checks remain pending. See the [N3 verification record](docs/verification/N3_EVERYDAY_UI_2026-10-11.md) and [N2 verification record](docs/verification/N2_DURABLE_QUEUE_2026-10-11.md). Remote macOS CI passed for commit `d63fd61`; the current N4a changes have not been verified remotely. See the [remote CI verification](docs/verification/REMOTE_CI_2026-10-11.md).

Follow [ROADMAP.md](ROADMAP.md) for the ordered work: quality baseline, transfer correctness, durable scheduling/recovery, everyday UI, advanced features and supported releases. The [archived UI checklist](docs/archive/MVP_UI_ROADMAP_2026-10-10.md) retains implementation and native smoke evidence.

Keep changes focused, preserve the existing shadcn tokens/components, and add tests for meaningful behavior and failure paths. Update the roadmap with actual evidence when a slice is finished. Do not commit runtime databases, downloaded files, credentials or signing keys.

For vulnerability reporting, security boundaries and known limitations, see [SECURITY.md](SECURITY.md). Request a private reporting channel before sharing sensitive vulnerability details.

**License:** this project is licensed under [GPL-3.0-or-later](LICENSE). Dependencies retain their own licenses; see the [dependency inventory](docs/DEPENDENCIES.md) for release review.

The download window defaults to 600×520 and keeps its action footer visible while options scroll. Switching modes retains both drafts. Disabled acceleration uses one connection for this download, independent of the global preference. Creating a configured download validates options before creation and persists them before dispatch. The app prepares this window in the background after startup. Closing it hides the loaded webview; the next opening starts a fresh form without recreating the window. Opening an already visible window focuses its existing draft. Closing the main workspace still hides it to the tray. Restart the native development app after changing Rust window commands.

Download categories are Compressed, Documents, Music, Pictures, Programs and Videos. The Category dropdown defaults to Automatic and previews the filename-based choice. A manual selection overrides detection; switching back to Automatic restores it. Bulk imports classify each file separately. Unknown extensions stay uncategorized, and existing custom category assignments remain available. Sidebar counts and filters include automatically categorized existing downloads. Classification changes organization only; it never moves a file.

Paste a single HTTP(S) link anywhere in the main workspace outside an editor to open Single URL mode. Paste links separated by newlines, spaces or tabs to open Multiple URLs mode with duplicates removed. Links are prepared for review; pasting never starts transfers. If a download draft is already open, choose Use pasted links or Keep draft. Ordinary text and pastes inside input fields or dialogs retain normal behavior. Imports support up to 200 unique links.
