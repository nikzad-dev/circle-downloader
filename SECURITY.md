# Security Policy

## Supported Versions

Only the latest release and active release line receive security updates.

| Version | Supported          | Status                                      |
| ------- | ------------------ | ------------------------------------------- |
| 0.1.x   | :white_check_mark: | Current active development release          |
| < 0.1.0 | :x:                | Unsupported historical development versions |

## Reporting a Vulnerability

We take the security and integrity of Circle Downloader seriously. If you believe you have discovered a vulnerability, please report it responsibly.

**Please do not report security vulnerabilities via public GitHub issues, discussions, or pull requests.**

### How to report

1. **GitHub Private Vulnerability Reporting (Preferred):**
   Use GitHub's [Private Vulnerability Reporting](https://github.com/nikzad-dev/circle-downloader/security/advisories/new) to submit details directly to the maintainers.

2. **Email Disclosure:**
   If GitHub advisory reporting is unavailable, email security details to:
   `security@circle-downloader.local` (or open an advisory through the repository owner's GitHub profile `nikzad-dev`).

### What to include in your report

To help us triage and resolve the issue quickly, please provide:
- A clear description of the vulnerability, potential impact, and affected components (frontend, Rust backend, IPC bridge, storage, or network transport).
- Specific version or commit SHA tested.
- Step-by-step reproduction steps or a minimal proof-of-concept (PoC).
- Relevant platform details (macOS, Windows, Linux, architecture).
- Any proposed remediation or patch if available.

### Response timeline

- **Initial acknowledgement:** Within 48 hours of receipt.
- **Triage and assessment:** Within 7 business days with confirmation of severity and scope.
- **Coordinated disclosure:** We will coordinate release timing and CVE assignment with you before publication. Please allow reasonable time to release a patch before public disclosure.

---

## Security Architecture & Boundaries

Circle Downloader is a local desktop application that handles untrusted network input and writes files to the local filesystem. To mitigate risks, the following boundaries are enforced:

### 1. Network Transport Policy
- **Public HTTPS Enforced in Release:** Release builds strictly require public HTTPS. Plain HTTP and loopback URLs are refused.
- **Private Address Deny-List:** Transport-level DNS resolution rejects loopback, private RFC 1918, link-local, carrier-grade NAT, multicast, and reserved/special-use IPv4 and IPv6 ranges.
- **Redirect Validation:** Redirects are independently evaluated before following. Downgrading from HTTPS to HTTP, public-to-private redirects, credential/fragment leakage, and excessive redirect hops are rejected.
- **Transport Pinning:** DNS resolution passes verified IP addresses directly to the TLS transport, preventing time-of-check to time-of-use (TOCTOU) DNS rebinding attacks.

### 2. Filesystem Safety & Directory Capabilities
- **Capability-Based Writes (`cap-std`):** File operations are anchored to a retained folder capability. Ambient paths and path traversal (`../`) are rejected.
- **Symlink Protection:** Directory resolution requires `NOFOLLOW` semantics. Symbolic links are rejected as destination folders or write targets.
- **Atomic Publication:** Transfers stream to temporary partial files with exclusive process locks. Completed files are published via atomic no-replace filesystem links, preventing concurrent write collisions and unexpected file overwrites.
- **Bounded Memory Verification:** Checksum hashing (SHA-256) streams in a bounded 64 KiB buffer regardless of file size, preventing memory exhaustion (OOM) denial-of-service.

### 3. IPC, IPC Redaction & Webview Isolation
- **Strict Content Security Policy (CSP):** The webview environment denies remote scripts, stylesheets, frames, objects, and form submissions. Only local bundled assets and Tauri IPC channels are permitted.
- **Tauri v2 Scoped Capabilities:** The webview has no ambient shell, network, or arbitrary filesystem permissions. All native actions pass through audited Rust commands.
- **Credential & URL Redaction:** Source URLs in IPC events and the UI redact sensitive queries, tokens, and basic-auth credentials. Original URLs are stored privately in local SQLite only when necessary for resume validators.
- **OS Credential Storage:** User proxy and site credentials are stored in platform-native credential managers (macOS Keychain, Windows Credential Manager, Linux Secret Service via the `keyring` crate) rather than plaintext files.

### 4. Known Out-of-Scope Items
- Same-user local compromise: If an attacker already has local execution privileges as the running desktop user, local data files and database contents are within that user's security boundary.
- Hardware, physical machine access, or root/administrator level access.
- Vulnerabilities in third-party download origin servers.
