# Release checklist

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace --all-targets` on macOS, Windows, and Linux
4. `cargo audit`
5. `cd ui && npm test && npm run build && npm audit --audit-level=high`
6. Confirm Apple + Windows + Tauri updater secrets (see [CODE_SIGNING.md](./CODE_SIGNING.md))
7. Tag `vX.Y.Z` and confirm Release workflow publishes installers + `latest.json` + SBOM
8. Install on a clean VM; run [PLATFORM-TEST-MATRIX.md](../PLATFORM-TEST-MATRIX.md)
9. Auto-update to the next RC on the same VM
10. Confirm Privacy/Terms/EULA links and purchase URL
