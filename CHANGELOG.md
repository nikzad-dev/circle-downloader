# Changelog

All notable changes to Circle Downloader will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Multi-platform GitHub Actions quality matrix (`macos-14`, `windows-latest`, `ubuntu-22.04`).
- Automated SPDX 2.3 Software Bill of Materials (SBOM) generator (`scripts/generate-sbom.mjs`).
- Production release and code-signing workflow (`.github/workflows/release.yml`).
- Security policy (`SECURITY.md`) and vulnerability disclosure procedure.
- Contributor guide (`CONTRIBUTING.md`), bug report, and feature request templates.

## [0.1.0] - 2026-10-11

### Added
- **Core Engine (N0–N2):**
  - Streaming transfer engine with bounded 64 KiB memory hashing for SHA-256 verification.
  - Capability-based directory isolation (`cap-std`) and atomic no-replace publication.
  - Multi-connection segmentation (opt-in 2–4 connections for compatible files >= 8 MiB).
  - SQLite persistence with ordered schema migrations (v1 through v4), WAL mode, and busy timeout handling.
  - Durable priority queue dispatch with global concurrency and per-host fairness caps.
  - Exponential backoff with jitter and `Retry-After` header adherence for transient failures.
  - Process exclusivity leases preventing split-brain database corruption.
- **Everyday Desktop UI (N3):**
  - Modern shadcn/ui interface with neutral and lime accents, supporting Light, Dark, and System appearance.
  - Menubar navigation with responsive accordion sidebar for transfer filters and queue views.
  - Virtualized downloads table (>= 100 items) with instant search, status filtering, and sorting.
  - Deep-linked task details Sheet (`/downloads/$downloadId`) with redacted source URLs.
  - Native folder grants and native Open/Reveal integration.
- **Advanced Capabilities (N4):**
  - Bandwidth rate limits and scheduled queues with IANA time zone and DST handling.
  - Bulk URL import with deduplication and validation.
  - Categories and tag management.
  - Scoped proxy configurations and OS credential vault integration (`keyring`).
  - Durable historical statistics without synthetic mock data.
