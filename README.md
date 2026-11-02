# numr desktop

Desktop app for [numr](https://github.com/nasedkinpv/numr), the natural-language text calculator. A thin [Tauri 2](https://tauri.app) shell around the [numr-web](https://github.com/nasedkinpv/numr-web) runtime: the same `app.js` and WebAssembly packages as [numr.cc](https://numr.cc), inside a native window with a tray icon, file dialogs, and `.numr` file association.

- Linux: WebKitGTK, native Wayland and HiDPI. Packages: AUR `numr-desktop-bin` / `numr-desktop-git`, `.deb`, `.rpm`, AppImage.
- macOS: WKWebView, menu-bar app without a Dock icon. Homebrew cask `numr-desktop`, `.dmg`. For a fully native macOS and iOS app see numr-apple.

## Install

```bash
# Arch Linux
yay -S numr-desktop-bin        # release build from GitHub
yay -S numr-desktop-git        # built from main

# macOS
brew tap nasedkinpv/tap
brew install --cask numr-desktop
```

Other Linux distributions: download the `.deb`, `.rpm`, or AppImage from the [latest release](https://github.com/nasedkinpv/numr-desktop/releases/latest). Runtime packages: `webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator`, `librsvg`.

## Develop

Prerequisites: Rust 1.88+, [`cargo-tauri`](https://tauri.app/reference/cli/) 2.x (`pacman -S cargo-tauri` or `cargo install tauri-cli --locked`), and on Linux the WebKitGTK development packages listed in the [Tauri prerequisites](https://tauri.app/start/prerequisites/).

```bash
git clone --recurse-submodules https://github.com/nasedkinpv/numr-desktop.git
cd numr-desktop
cargo tauri dev            # debug build, window opens
cargo tauri build          # release bundles in target/release/bundle/
```

`scripts/build-ui.sh` assembles `dist/` from the `numr-web` submodule before every dev run and build. To work on the web app and the shell side by side, point it at your checkout instead of the pinned submodule:

```bash
NUMR_WEB_DIR=../numr-web cargo tauri dev
```

Shell code lives in `src/main.rs`; the webview side of the bridge is `numr-web/desktop/index.js`. The webview talks to the shell only through the commands and events listed there, and `capabilities/default.json` is the full list of what it may call. The root `package.json` exists only so the Tauri CLI resolves this directory as the project root instead of the submodule.

Known Linux quirk: WebKitGTK renders a blank window on the proprietary NVIDIA driver without `WEBKIT_DISABLE_DMABUF_RENDERER=1`. The app sets it automatically when that driver is loaded.

## Release

1. Bump the `numr-web` submodule to the tagged web release and `version` in `Cargo.toml`, then `cargo build` to refresh `Cargo.lock`.
2. Add `docs/releases/vX.Y.Z.md`.
3. Tag `vX.Y.Z` and push. The release workflow builds Linux x64/arm64 and macOS arm64/x64 bundles with `tauri-action`, publishes the GitHub release, and then updates the AUR package and the Homebrew cask.

Secrets: `NUMR_WEB_DEPLOY_KEY` (read-only deploy key of the private numr-web submodule), `AUR_USERNAME`, `AUR_EMAIL`, `AUR_SSH_PRIVATE_KEY`, `HOMEBREW_TAP_DEPLOY_KEY`. macOS bundles are unsigned until Apple signing secrets are configured for `tauri-action`.

## Repository layout

```text
numr-desktop/
├── src/main.rs            # Tauri shell: window, tray, menu, dialogs, file opening
├── tauri.conf.json        # App, window, CSP, bundle targets, file associations
├── capabilities/          # Webview permissions (ACL)
├── icons/                 # App and tray icons
├── scripts/build-ui.sh    # dist/ from numr-web
├── numr-web/              # Submodule: shared web runtime and desktop HTML/bridge
├── aur/, aur-git/         # PKGBUILDs published by the release workflow
├── homebrew/Casks/        # Cask published to nasedkinpv/homebrew-tap
└── docs/releases/         # Release notes, one file per tag
```

## License

MIT
