# Circle Downloader

A local desktop download manager built with Tauri, Rust, React and shadcn/ui. Rust owns transfers and persisted tasks; the interface shows real status, bytes and speed through native progress channels.

**Status: development MVP, version 0.1.0.** The core workflow is implemented. Resume safety, file collision handling, scheduling and crash recovery still need hardening before a production release. See the [implementation audit and roadmap](ROADMAP.md) for completed work, known gaps and the next phases.

## Current features

- Home overview with real transfer counts, recent downloads and failures.
- Add Download with URL validation, optional filename and Start now.
- Downloads table with progress, transferred bytes, speed and task-specific actions.
- Pause, resume, cancel, manual retry and removal confirmation.
- SQLite task/settings persistence and basic interrupted-task recovery.
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
2. Paste a direct HTTPS file URL. HTTP is currently accepted only for loopback hosts such as `localhost` and `127.0.0.1`; remote plain HTTP and other schemes are rejected.
3. Optionally set a filename. Leave **Start now** selected to begin, or clear it to save the task as Paused and choose **Resume** later. A disabled backend auto-start setting also saves it paused.
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

The root contains `circle-downloader.db` and a `downloads/` directory. Partial files have task-specific `.part` names in that directory. This is the app-managed folder, not the operating system's Downloads folder. Debug runs with the data-directory override use the specified root instead.

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

`build` runs TypeScript and creates the production frontend. `test` runs Vitest's Node and jsdom projects; `test:rust` runs native tests, including controlled localhost HTTP fixtures and temporary files. Restricted sandboxes must permit fixture socket binding. Use `bun run test`, not `bun test`, for this suite. `bun run test:watch` enables frontend watch mode.

The 2026-10-10 audit passed **68 frontend tests**, **44 Rust tests**, frontend lint/build and Rust formatting. The seven strict Clippy diagnostics found in that audit have since been resolved; the current checks are tracked in roadmap N0. The audit also records prior isolated macOS native smoke checks; full accessibility and Windows/Linux release QA remain pending. Check the latest results rather than treating this snapshot as a guarantee.

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

| Path                              | Purpose                                                        |
| --------------------------------- | -------------------------------------------------------------- |
| `src/routes/`                     | Route configuration and page entry points                      |
| `src/features/downloads/`         | Downloads API/cache, forms, actions, progress bridge and pages |
| `src/components/ui/`              | Editable shadcn Base UI primitives                             |
| `src/components/theme/`           | Shared appearance state and controls                           |
| `src/lib/orpc/`, `src/lib/tauri/` | Typed local facade and native adapters                         |
| `src-tauri/src/application/`      | Task lifecycle and worker coordination                         |
| `src-tauri/src/domain/`           | Models, transitions, errors and URL/filename policies          |
| `src-tauri/src/infrastructure/`   | HTTP streaming and SQLite                                      |
| `src-tauri/src/ipc/`              | Progress channel hub                                           |
| `src-tauri/src/api_tests.rs`      | Native workflow and HTTP fixture tests                         |

## Current limitations and contributing

Resume requires matching strong ETag and valid Content-Range metadata before appending. Servers without a strong validator require a fresh download after interruption. Controls wait for the old writer; response/header inactivity is bounded to 30 seconds. Saved tasks reserve their final names and completion never replaces an external file; hard-link publication requires filesystem support.

Configured concurrency is still stored but not enforced. Recovery does not yet reconcile every filesystem/database crash window. Resumed-file hashing uses a bounded 64 KiB buffer, but computed digests are not yet retained or exposed as checksum verification. Automatic retries and comprehensive redirect/path hardening remain unfinished. CI is configured; remote execution and Windows/Linux verification remain pending.

Follow [ROADMAP.md](ROADMAP.md) for the ordered work: quality baseline, transfer correctness, durable scheduling/recovery, everyday UI, advanced features and supported releases. The [archived UI checklist](docs/archive/MVP_UI_ROADMAP_2026-10-10.md) retains implementation and native smoke evidence.

Keep changes focused, preserve the existing shadcn tokens/components, and add tests for meaningful behavior and failure paths. Update the roadmap with actual evidence when a slice is finished. Do not commit runtime databases, downloaded files, credentials or signing keys.

**License:** this project is licensed under [GPL-3.0-or-later](LICENSE). Dependencies retain their own licenses; see the [dependency inventory](docs/DEPENDENCIES.md) for release review.
