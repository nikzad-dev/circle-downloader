# Circle Downloader — Implementation & Verification Record

Comprehensive consolidated verification record covering Phases N0 through N4 on the macOS baseline.

---

## 1. Quality Baseline & Remote CI (N0)

### Local Quality Gates
- **Rust Toolchain:** Rust 1.91.0 with Cargo, rustfmt, and strict Clippy (all warnings denied).
- **Frontend Quality:** Bun 1.4+, TypeScript `tsc --noEmit`, ESLint (flat config), Prettier.
- **Contract Integrity:** Shared JSON IPC contract tests validate Rust DTO serialization against TypeScript schemas (`tests/fixtures/ipc-contract.json`).

### Remote CI Verification
- **Run ID:** [Quality #38071582041](https://github.com/nikzad-dev/circle-downloader/actions/runs/38071582041) (commit `d63fd61`).
- **Runner:** GitHub-hosted `macos-14` (Apple Silicon).
- **Results:** Clean execution across frozen Bun install, typecheck, lint, Vitest, Vite production bundle, rustfmt, clippy, and cargo test.

---

## 2. Transfer Correctness, File Safety & Integrity (N1)

### Streaming Verification & Checksums
- **Bounded Buffer:** Streaming full-file SHA-256 calculation executes via a bounded 64 KiB read buffer on the retained file handle, preventing memory scaling with file size.
- **Validator Preservation:** Strong ETags (`If-Range`) and response validators are persisted prior to partial body writes. Content-Range start, end, and total bounds are verified on HTTP 206 responses; Range -> 200 responses safely restart rather than corrupting partials.
- **Result Classification:** Integrity states distinguish `computed` (untrusted origin), `matched` (cryptographically verified against user-provided SHA-256), and `mismatch` (`CHECKSUM_MISMATCH`). Partial files are preserved on mismatch for user inspection without publication.

### Durability & Atomic Publication
- **Protocol Boundary:**
  1. Partial sync and complete stream hash on open handle.
  2. Commit `finalization=ready` and actual digest to SQLite (`synchronous=FULL`).
  3. Publish via atomic no-replace filesystem link.
  4. Commit `Completed` with `finalization=cleanup`.
  5. Unlink partial file and sync directory entries.
- **Crash Recovery:** Deterministic startup reconciliation recovers ready-with-partial, both-name, and final-only states without overwriting destination conflicts.

### Security & Directory Hardening
- **Network Boundaries:** Release builds strictly require public HTTPS. Transport-level DNS resolution rejects loopback, RFC 1918, link-local, carrier-grade NAT, and special-use address spaces. Resolved IP addresses are passed directly to TLS transport to prevent DNS rebinding.
- **Filesystem Hardening (`cap-std`):** File operations are anchored to a retained directory capability. Directory symlinks and ambient parent traversals (`../`) are denied with `NOFOLLOW` semantics.
- **Webview Isolation:** Tauri CSP denies remote scripts, objects, and frames. IPC redacts sensitive query parameters, auth tokens, and basic-auth credentials from UI surfaces.

---

## 3. Durable Queue, Persistence & Process Leases (N2)

### Process Exclusivity
- OS-held directory lock leases precede SQLite initialization. Split-brain multi-process ownership is blocked; killed-process stale locks recover automatically upon restart.

### Persistence & Ordered Migrations
- Ordered migration framework (v1 -> v6) with automatic `.bak` pre-upgrade database backups.
- SQLite operates in WAL mode with `busy_timeout` contention handling.

### Queue Dispatch & Retry Policies
- Bounded dispatcher enforces global concurrency and per-host fairness caps (maximum 2 active transfers per host).
- Retry budget (5 attempts) with exponential backoff, jitter, and adherence to HTTP `Retry-After` headers.
- User-paused intent is preserved across restarts; quit drains and cancels in-flight workers safely while window close minimizes to system tray.

---

## 4. Everyday Desktop UI & Navigation (N3)

### Interface Architecture
- Replaced sidebar navigation with a compact shadcn Menubar (File, Edit, View, Window, Help) and top tab navigation.
- Responsive category/queue sidebar with collapsible accordion controls.
- Light, Dark, and System appearance preferences synced with OS theme listeners.

### Table Virtualization & Performance
- Downloads table virtualizes rows when filtered list exceeds 100 items (`@tanstack/react-virtual`).
- Progress updates batch at 10 Hz over a single disposable Tauri channel, avoiding UI thread saturation.
- Deep-linked task details Sheet (`/downloads/$downloadId`) displays redacted sources, transfer rates, retry counters, and verification digests.

### Native Integrations
- Destination selection uses native folder picker dialogs returning opaque grant IDs.
- Native Open File and Reveal in Finder/Explorer/File Manager with path existence validation.

---

## 5. Multi-Segment Transfers (N4A)

### Acceleration & Fallback Invariants
- Default `maxConnectionsPerDownload = 1` preserves single-connection engine safety. Settings allow opt-in 1–4 connections for files $\ge$ 8 MiB.
- Origin qualification: Requires verified total size, strong ETag, `Accept-Ranges: bytes`, identity encoding, and successful 1-byte head and midpoint GET probes.
- Incompatible origins automatically fall back to single-connection streaming without data loss.

### Assembly & Durability
- Segment extents and checkpoints persist in SQLite (schema v5).
- Assembly streams contiguous verified segment parts into canonical destination using a 64 KiB buffer, verifying total SHA-256 before removing temporary segment parts.

### Benchmark Results (Controlled 8 MiB Payload, macOS arm64)
Seven paired samples alternating connection counts on local test server:

| Run | 1 Connection (ms) | 4 Connections (ms) | Speedup |
|:---:|:-----------------:|:------------------:|:-------:|
| 1   | 1351.56           | 883.55             | 1.53x   |
| 2   | 1298.73           | 916.91             | 1.42x   |
| 3   | 1299.16           | 885.36             | 1.47x   |
| 4   | 1238.90           | 887.61             | 1.40x   |
| 5   | 1247.54           | 896.20             | 1.39x   |
| 6   | 1503.82           | 890.78             | 1.69x   |
| 7   | 1320.82           | 896.08             | 1.47x   |
| **Median** | **1299.16** | **896.08**         | **1.46x** |

---

## 6. Advanced Controls & Organization (N4B)

### Capabilities Delivered
- **Queues & Bandwidth:** Named queues with custom worker caps, FIFO bandwidth rate limiters, and pause-new-starts toggles.
- **Priority Aging:** Priority levels weighted against wait time (1 priority step $\approx$ 2 minutes of queue wait) to prevent starvation.
- **Scheduling:** One-shot schedules with explicit IANA time zones and daylight saving time (DST) gap rejection / fold disambiguation.
- **Bulk Import:** Batch URL imports (up to 200 items) with deduplication, syntax validation, and atomic batch ID idempotency.
- **OS Credential Vault:** Proxy and download credentials stored in native platform keychains (`keyring`) with scoped grant IDs and strict cross-origin redaction.
- **Durable Statistics:** Historical throughput and byte measurements aggregated from actual transfer attempt logs; zero synthetic or mock graph data.

### Large-History Benchmarks (20,000 Tasks)
Measured on macOS arm64, Rust 1.91.0, 7 samples:

| Benchmark Workload | Median (ms) | Variance (ms²) | Notes |
|:-------------------|------------:|---------------:|:------|
| History retrieval (20k tasks) | 120.37 | 203.72 | Full SQLite WAL decode |
| Runnable query check (200 queued) | 1.74 | 0.26 | Dispatcher scan |
| Statistics aggregation (20k attempts) | 13.89 | 13.63 | Date-bucketed SQL query |
| Client-side filter & sort (20k array) | 0.61 | 0.49 | Vitest unit environment |

Raw machine-readable benchmark data: [docs/assets/N4B_BENCHMARKS.json](assets/N4B_BENCHMARKS.json).
