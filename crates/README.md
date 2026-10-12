# crates

Rust workspace of the `tmgr` command line tool. It is a separate Cargo workspace from `src-tauri` (the desktop app): both ship an executable named `tmgr`, so they cannot share one workspace. `src-tauri` keeps its own `Cargo.lock` and target directory.

- `tmgr-core` - shared library (output envelope `tmgr.v1`, version info).
- `tmgr-cli` - the `tmgr` binary (clap).

## Build and test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tmgr-cli -- version --json
```

## Releases

Tag `cli-vX.Y.Z` (must equal the `tmgr-cli` version and have a `## X.Y.Z` section in `tmgr-cli/CHANGELOG.md`) runs `.github/workflows/cli-release.yml`: musl Linux and macOS archives, `SHA256SUMS`, minisign signatures, build provenance attestation, macOS codesign and notarization. When a signing secret is missing the release is published as a prerelease and says what was skipped. `workflow_dispatch` is a dry run that publishes nothing.
