# Contributing to Circle Downloader

Thank you for your interest in contributing to Circle Downloader! We welcome contributions that align with our focus on transfer correctness, security, file safety, and a fast, accessible desktop experience.

Please review this guide before submitting issues or pull requests.

---

## Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free environment for all contributors. Please treat everyone with respect and professionalism.

---

## Architecture Principles & Invariants

When contributing code, keep our core architectural boundaries in mind:

1. **Rust Owns the Engine:** All network requests, file writes, SQLite storage, queue dispatch, retry policies, and integrity checks are owned and executed by the Rust backend (`src-tauri/`).
2. **Frontend is Presentation & Control:** The React/Tauri frontend (`src/`) interfaces with Rust via typed oRPC contracts and Tauri channels. Do not implement ad-hoc network transfers or background business logic in the webview.
3. **No Silent Overwrites or Ambiguous States:** File operations require explicit destination validation, directory capability anchoring (`cap-std`), and atomic no-replace publication.
4. **Honest User States:** UI displays actual status, real transfer rates, and verified integrity. Never display synthetic progress or claim a checksum is verified unless cryptographically matched.
5. **Theme & Component Consistency:** We use Base UI and Tailwind CSS with a neutral/lime theme. Reuse existing primitives in `src/components/ui/` rather than adding competing component libraries.

---

## Development Prerequisites

Ensure you have the following installed locally:

- **Bun:** Current stable (v1.2+) for frontend package management and script execution.
- **Rust & Cargo:** Rust **1.91.0** (pinned in `rust-toolchain.toml`), along with `rustfmt` and `clippy`.
- **Node.js:** Node 20.19+ or 22.12+ (compatible with Vite 8).
- **Tauri Prerequisites:**
  - **macOS:** Xcode Command Line Tools (`xcode-select --install`).
  - **Linux:** System development libraries (`libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `build-essential`, `curl`, `wget`, `file`, `libssl-dev`).
  - **Windows:** Microsoft C++ Build Tools and Microsoft Edge WebView2 runtime.

---

## Local Development Workflow

### 1. Install Dependencies
```sh
bun install --frozen-lockfile
```

### 2. Run Desktop App in Development Mode
```sh
bun run tauri dev
```
This starts the Vite dev server on port `1420` and launches the desktop window.

### 3. Isolated QA Data Directory
To run without affecting your regular user application data:
```sh
CIRCLE_DOWNLOADER_DATA_DIR="$(mktemp -d)" bun run tauri dev
```

---

## Verification & Quality Gates

All pull requests must pass the following checks locally and in GitHub Actions CI:

### Frontend Gates
```sh
bun run typecheck        # TypeScript validation
bun run lint             # ESLint check
bun run test             # Vitest frontend tests
bun run build            # Production Vite bundle build
```

### Rust Backend Gates
```sh
bun run fmt:rust         # cargo fmt --check
bun run clippy:rust      # Strict clippy check with all warnings denied
bun run test:rust        # cargo test --locked --all-targets
```

---

## Submitting Pull Requests

1. **Branch Naming:** Use clear branch names like `feature/short-desc`, `fix/issue-desc`, or `chore/task-desc`.
2. **Keep Commits Clean:** Write concise commit messages explaining the *why* behind your change.
3. **Add Tests:** If adding a feature or fixing a bug, include corresponding Vitest or Cargo tests.
4. **Document Changes:** If updating architecture, configuration, or user-facing behavior, update relevant documentation in `README.md` or `ROADMAP.md`.
5. **Check CI:** Verify all quality gates pass in the GitHub Actions runner before requesting review.

---

## Reporting Issues

- **Bugs:** Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.yml) with OS version, reproduction steps, and error messages.
- **Feature Requests:** Use the [Feature Request template](.github/ISSUE_TEMPLATE/feature_request.yml) describing the problem, proposed solution, and UX considerations.
- **Security Vulnerabilities:** Follow our [Security Policy](SECURITY.md) — do **not** file public issues for security vulnerabilities.
