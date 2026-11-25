# TMGR frontend

Source of the [TMGR](https://tmgr.dev) web app, the TMGR Desktop app and the `tmgr` command line tool. TMGR is a task manager that also supervises AI coding agents.

- Web app: Vue 3 and Vite, served at https://tmgr.dev.
- Desktop app: Tauri, with local workspaces that never touch the cloud, a built-in updater and a plugin system.
- `tmgr` CLI: work on tasks from the terminal (`tmgr start`, `tmgr done`).
- Agent runner: runs Claude Code and Codex sessions in an OS sandbox. You can watch a session (`tmgr attach`), stop it and roll back its changes.
- Manager chat: one chat on your desktop and your phone that sees and drives your agent sessions. The chat is end-to-end encrypted; the server relays ciphertext only.
- Decisions: agents send a question card with options. You answer once, on the desktop or the phone.

Trust point: runner execution is local only. The TMGR server and the web app never send commands to a runner.

The backend is a separate repository and is not public, so the web app needs a TMGR API to talk to. Chat and Decisions on a phone need the TMGR iOS app, which is also separate; there is no Android app.

## Install

### Web

Open https://tmgr.dev. No install is needed.

### Desktop

1. Open the [latest `desktop-v*` release](https://github.com/tmgr-dev/frontend/releases).
2. Download the file for your system:
   - macOS (Apple silicon): `.dmg`
   - Windows: `-setup.exe` or `.msi`
   - Linux: `.AppImage`, `.deb`, `.rpm` or `.snap` (x86_64 and aarch64; the aarch64 `.snap` can be missing from a release)
3. Install it. The app updates itself.

Intel Mac: use the CLI or the web app. The desktop app is built for Apple silicon only.

To check a download, see [Verify a download](crates/README.md#verify-a-download).

### CLI

On Linux and macOS:

```bash
curl -fsSL https://tmgr.dev/install.sh | sh
```

The script installs `tmgr` into `~/.local/bin` without sudo. It always checks the SHA-256 of the archive. It also verifies the minisign signature when `minisign` is installed. Run `tmgr self-update` to update.

Other ways:

```bash
nix profile install github:tmgr-dev/frontend#tmgr-cli
cargo install --locked --features runner --git https://github.com/tmgr-dev/frontend tmgr-cli
```

On Windows, use WSL2 and run the Linux installer inside it. Full CLI reference: [crates/README.md](crates/README.md).

### NixOS

The Linux desktop app is packaged as a flake that wraps the release `.deb`. It is bumped automatically after each desktop release, so it can be a few versions behind.

```
nix profile install github:tmgr-dev/frontend#tmgr
nix run github:tmgr-dev/frontend#tmgr
```

NixOS configuration:

```nix
{
  inputs.tmgr.url = "github:tmgr-dev/frontend";

  outputs = { nixpkgs, tmgr, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        { environment.systemPackages = [ tmgr.packages.x86_64-linux.default ]; }
      ];
    };
  };
}
```

The package is available for `x86_64-linux` and `aarch64-linux`. The built-in updater is disabled in this package; update with `nix flake update tmgr`.

The desktop app also installs a `tmgr` executable. The `tmgr-cli` package and the desktop package conflict in one Nix profile.

## Repository layout

| Path | Contents |
|---|---|
| `src/` | Web and desktop UI (Vue 3, Vite) |
| `src-tauri/` | Desktop app (Tauri, Rust). Own `Cargo.lock`; changelog in `src-tauri/CHANGELOG.md` |
| `crates/` | Rust workspace: `tmgr` CLI, runner, attach view, chat relay client, local workspaces, usage engine |
| `plugin-sdk/` | SDK, templates and examples for desktop plugins |
| `docs/` | Developer docs: plugins, local access API, localization |
| `nix/`, `flake.nix` | Nix packages for the desktop app and the CLI |
| `snap/`, `packaging/` | Snap packaging and the Homebrew formula template (no Homebrew tap is published yet) |
| `public/` | Static files, including `install.sh` |
| `pwa/` | Service worker code |
| `scripts/` | Build and release helper scripts |
| `tests/` | Performance, theme and visual tests. Most Jest tests are in `src/**/__tests__/` |
| `.github/workflows/` | CI and release workflows |

## Development

### Web app

You need Node.js `^20.19.0` or `>=22.12.0`.

```bash
npm install
cp .env.example .env     # set VITE_API_BASE_URL
npm run dev              # http://localhost:5173
```

`VITE_API_BASE_URL` is the URL of the TMGR API, for example `http://localhost:8080/api/` for a local backend. Not verified: whether the hosted API accepts requests from a `localhost` origin. The Pusher variables in `.env.example` are needed only for real-time features.

Other commands:

```bash
npm test                 # Jest
npm run typecheck        # vue-tsc
npm run format           # Prettier (CI runs prettier --check)
npm run build            # production build
```

### Desktop app

You need the Rust toolchain and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system.

```bash
npm install
npm run desktop:dev      # tauri dev
npm run desktop:build    # scripts/build-desktop.sh
```

### CLI and runner

The `crates/` workspace needs Rust 1.89 or newer. Run these from the repository root:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p tmgr-cli -- version --json
```

`src-tauri` is not part of this workspace: it and `tmgr-cli` both build an executable named `tmgr`.

## Releases

Releases are made by pushing a tag. Each tag runs its own workflow in `.github/workflows/`.

| Tag | Release | Notes |
|---|---|---|
| `desktop-vX.Y.Z` | TMGR Desktop | Needs a `## X.Y.Z — YYYY-MM-DD` section in `src-tauri/CHANGELOG.md`. The version must match `tauri.conf.json`, `src-tauri/Cargo.toml` and `Cargo.lock` |
| `cli-vX.Y.Z` | `tmgr` CLI | Needs a `## X.Y.Z` section in `crates/tmgr-cli/CHANGELOG.md`. Archives are signed with minisign. Without the signing secret the release is a prerelease |

Assets of both release types carry a GitHub build provenance attestation.

## License

[GNU Affero General Public License v3.0](LICENSE).
