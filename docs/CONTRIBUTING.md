# Contributing to WhimprFlow

## Dev setup

### Prerequisites

- Rust stable (see workspace `rust-version`)
- Node 20+
- Platform native deps:
  - **macOS:** Xcode CLT
  - **Windows:** MSVC + LLVM (for bindgen / whisper-rs); optional `scripts/win-cargo.cmd`
  - **Linux:** see `docs/LINUX-WAYLAND.md` apt packages (`libwebkit2gtk-4.1-dev`, `libx11-dev`, …)

### Run

```bash
# UI only (browser preview)
cd ui && npm ci && npm run dev

# Full Tauri app (from repo root)
cd ui && npm exec tauri dev
```

Windows helpers: `dev.ps1`, `scripts/win-cargo.cmd`.

## Tests

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test -p whimpr-tauri --test smoke
cd ui && npm test && npm run build
```

## PR checklist

- [ ] `cargo fmt` / `clippy -D warnings` / `cargo test --workspace` pass
- [ ] UI `npm test` + `npm run build` pass
- [ ] New Tauri commands return `Result` where I/O or locks can fail
- [ ] Settings fields use `#[serde(default)]` for back-compat
- [ ] Docs updated when behavior changes (`docs/`, README links)
- [ ] No secrets committed (`.env`, private keys, signing material)
