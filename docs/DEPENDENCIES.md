# Dependency license review

Reviewed 2026-10-10; Rust inventory refreshed 2026-10-11 for the owner-selected GPL-3.0-or-later project license. The root `LICENSE`, `package.json` and `src-tauri/Cargo.toml` declare that license. Dependency code, fonts and notices retain their own terms.

## Inventory scope and findings

- [Rust macOS inventory](licenses/rust-macos.csv): 327 packages from locked Cargo metadata filtered for `aarch64-apple-darwin`, including build and test dependencies and this workspace. All report a license expression. Metadata may include packages that are not linked into the shipped binary.
- [Direct npm inventory](licenses/npm-direct.csv): 43 installed runtime/development package manifests, with exact installed versions. All report a license. This is a direct-dependency review, not a complete transitive npm SBOM.
- Most reviewed packages declare MIT, Apache-2.0, ISC, BSD or other permissive alternatives. Preserve upstream license and attribution files when packaging; do not replace them with the project license.
- Rust includes MPL-2.0 packages `cssparser`, `cssparser-macros`, `dtoa-short`, `option-ext` and `selectors`. Distribution must account for the covered source and its notices; the project license does not relicense those files. See [Mozilla's MPL distribution FAQ](https://www.mozilla.org/en-US/MPL/2.0/FAQ/).
- The Figtree and Geist font packages declare OFL-1.1. Retain their font license/copyright information in distributions and review the font naming terms before modifying them. See the [OFL FAQ](https://openfontlicense.org/ofl-faq/).
- Rust metadata also includes Unicode-3.0 and CDLA-Permissive-2.0 declarations. Include the applicable upstream texts alongside the other bundled notices.

The owner updated the project license to GPL-3.0-or-later on 2026-10-10. The inventory records declared licenses; a complete compatibility and distribution review under the new project license remains N5 work. Generate an artifact-specific SBOM and third-party notices, inspect bundled SQLite/TLS/webview assets and transitive npm dependencies, and repeat the inventory for Windows/Linux before publishing installers. No dependency source was modified in this slice.

## Reproduce the source inventory

```sh
bun install --frozen-lockfile
cargo metadata --manifest-path src-tauri/Cargo.toml --locked \
  --format-version 1 --filter-platform aarch64-apple-darwin
```

Read the Cargo `packages` license fields and installed `node_modules/<direct-package>/package.json` manifests. CSVs record the reviewed snapshot; regenerate them when the lockfiles change. The canonical project license came from the [GNU GPL version 3](https://www.gnu.org/licenses/gpl-3.0.html), with the project declaration allowing version 3 or any later version.

The 2026-10-11 N1 safety slice adds `cap-std`, `cap-fs-ext` and `same-file` as direct dependencies and removes the unused native opener plugin. The refreshed macOS inventory records their locked transitive packages; all report license expressions.

The N4b advanced-controls slice adds `chrono-tz` for IANA/DST resolution and platform-native `keyring` for scoped secret storage. The macOS inventory was refreshed from locked, platform-filtered metadata on 2026-10-11; all included packages declare licenses. Windows/Linux vault integration still requires platform verification.

## Automated SPDX 2.3 SBOM Generation

An automated SPDX 2.3 SBOM generator is provided at `scripts/generate-sbom.mjs` (run via `bun run sbom` or `node scripts/generate-sbom.mjs`). It extracts:
- Complete locked Rust package dependencies, licenses, and repository locations from `cargo metadata`.
- All direct and development npm dependencies and licenses from `node_modules`.
- Generates a machine-readable [SPDX 2.3 JSON document](licenses/sbom.spdx.json) referencing package relationships and declared license terms.

