## Summary

<!-- Briefly explain the motivation and summarize the changes made in this PR. -->

## Changes Made

-
-

## Verification & Testing

<!-- Detail the tests and commands you ran to verify this change. -->

- [ ] `bun run typecheck` passes
- [ ] `bun run lint` passes
- [ ] `bun run test` passes (Vitest)
- [ ] `bun run build` passes
- [ ] `bun run fmt:rust` passes
- [ ] `bun run clippy:rust` passes (Strict, -D warnings)
- [ ] `bun run test:rust` passes (Cargo test suite)

### Manual / Native Smoke Evidence

<!-- If applicable, describe manual testing with isolated data directory or desktop app. -->

## Security & Architectural Checklist

- [ ] Does not bypass Rust-authoritative transfer, network, or filesystem boundaries.
- [ ] Respects `cap-std` directory capabilities and atomic no-replace publication rules.
- [ ] Does not log or leak unredacted credentials or private URLs in IPC/UI.
- [ ] Conforms to neutral/lime Base UI theme styling and responsive patterns.
