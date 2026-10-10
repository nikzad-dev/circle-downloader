# Circle Downloader

A local desktop download manager built with Tauri, Rust, React and shadcn/ui. Rust owns transfers and persisted tasks; the interface shows real status, bytes and speed through native progress channels.

**Status: development MVP, version 0.1.0.** The core workflow is implemented. Scheduling, supported-platform durability, security/release QA and installer verification remain before a production release. See the [implementation audit and roadmap](ROADMAP.md) for completed work, known gaps and the next phases.

## Current features

- Home overview with real transfer counts, recent downloads and failures.
- Add Download with URL validation, optional filename and Start now.
- Downloads table with progress, transferred bytes, speed and task-specific actions.
- Pause, resume, cancel, manual retry and removal confirmation.
- SQLite task/settings persistence, exclusive process ownership and verified publication recovery.
- Retained SHA-256 digests with optional expected-checksum comparison.
- Rust-enforced public HTTPS, safe redirect/DNS policy and anchored no-follow file operations.
- Light, Dark and System appearance using the existing neutral/lime shadcn theme.

The app uses a single connection per transfer. Folder selection, search/filter/sort, bulk actions, task details, automatic retries, segmentation and browser integration are planned.

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

1. Open **Add download** from the toolbar or Home.
2. Paste a direct public HTTPS file URL without embedded credentials or fragments. Private/special-use addresses, unsafe redirects and remote plain HTTP are rejected. Debug builds additionally accept loopback fixture links; release builds do not enable HTTP/LAN access.
3. Optionally set a filename and a trusted 64-character SHA-256 checksum. Circle compares the complete file with that checksum before publishing; without one it reports a calculated digest rather than verified integrity. Leave **Start now** selected to begin, or clear it to save the task as Paused and choose **Resume** later. A disabled backend auto-start setting also saves it paused.
4. Follow progress in **Downloads**. Available controls depend on the current task status. Failed tasks offer manual **Retry**.
5. Choose **Remove** to remove a record. The confirmation offers partial-file deletion for unfinished tasks, enabled by default. Completed files remain on disk.

Appearance is available in the sidebar and Settings. System is the initial default; your selection is saved. Transfer preferences and destination selection are not yet editable in Settings.

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

The 2026-10-11 local checks passed **81 frontend tests in 16 files** and **73 Rust tests**, plus typecheck, lint, production build, formatting, strict Clippy and the macOS debug app build. One ignored Rust test is a subprocess fixture explicitly invoked by the process-ownership tests. The built native app also passed duplicate-process rejection and restart ownership checks in an isolated data root. See the [ownership/error verification](docs/verification/N1_OWNERSHIP_ERRORS_2026-10-11.md) and [checksum/recovery/security verification](docs/verification/N1_CHECKSUM_RECOVERY_SECURITY_2026-10-11.md). Full accessibility and Windows/Linux release QA remain pending.

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

The oRPC facade runs locally in TypeScript; it is not a Rust HTTP endpoint or an authorization boundary. Sensitive validation and transfer behavior belong in Rust. Route navigation does not own transfer workers. Quitting the process stops transfers; there is no implemented tray/background-agent policy.

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

HTTP, TLS, connection and storage failures have structured codes and actionable UI messages without exposing signed URLs or internal paths. Certificate failures require correcting the clock or server TLS configuration; certificate validation remains enabled. Transient connection/server/storage failures are marked retryable; automatic retry scheduling remains planned.

Configured concurrency is still stored but not enforced. Publication uses a synced partial and durable ready/cleanup checkpoints; startup verifies recorded digests and reconciles partial/final files. It preserves conflicting files rather than overwriting them. Full-file hashing uses a bounded 64 KiB buffer; digests and expected-checksum results are retained. Startup rehashing can take time with a large history. Windows directory-entry power-loss durability and cross-platform release checks remain pending. Automatic retries and the N2 recovery/scheduling coordinator remain unfinished. Remote macOS CI passed for commit `d63fd61`; see the [verification record](docs/verification/REMOTE_CI_2026-10-11.md). Windows/Linux verification remains pending.

Follow [ROADMAP.md](ROADMAP.md) for the ordered work: quality baseline, transfer correctness, durable scheduling/recovery, everyday UI, advanced features and supported releases. The [archived UI checklist](docs/archive/MVP_UI_ROADMAP_2026-10-10.md) retains implementation and native smoke evidence.

Keep changes focused, preserve the existing shadcn tokens/components, and add tests for meaningful behavior and failure paths. Update the roadmap with actual evidence when a slice is finished. Do not commit runtime databases, downloaded files, credentials or signing keys.

**License:** this project is licensed under [GPL-3.0-or-later](LICENSE). Dependencies retain their own licenses; see the [dependency inventory](docs/DEPENDENCIES.md) for release review.
