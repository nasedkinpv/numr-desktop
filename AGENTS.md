# Project Notes

Maintainer notes for numr-desktop. User-facing documentation lives in `README.md`.

## Repository Layout

- `src/main.rs`: the whole Tauri shell. Window, tray, macOS menu, file dialogs, `.numr` opening (CLI arguments, second instance, drag-and-drop, macOS `Opened`), the open-file state (Save, Save As, last document remembered in the app config directory), the exchange-rate cache shared with the TUI/CLI (`rates.json` in numr-core's format and location, 1-hour expiry), and the commands the webview may call.
- `numr-web/`: git submodule. The frontend (`desktop/index.html`, `desktop/index.js`, shared `app.js`, WASM packages) is owned there; this repository never edits it. `scripts/build-ui.sh` copies what the webview needs into `dist/` before every dev run and build. `NUMR_WEB_DIR=../numr-web` points it at a sibling checkout for side-by-side work.
- `capabilities/default.json`: the complete ACL for the main window. Keep it minimal; file paths never cross the webview boundary because dialogs and file I/O run in Rust.
- `packaging/`: Linux `.desktop` template (adds `%F` so file managers can open documents) and the `text/x-numr` MIME definition. Tauri's default template provides neither.
- `package.json` at the root has no dependencies. It exists only so the Tauri CLI treats this directory, not the submodule, as the project root (it picks the nearest `package.json`).
- `aur/` publishes `numr-desktop-bin` from the release `.deb`; `aur-git/` builds from source with `cargo-tauri` and the deb bundler. `homebrew/Casks/numr-desktop.rb` is pushed to `nasedkinpv/homebrew-tap/Casks`.
- `docs/releases/vX.Y.Z.md`: release notes, one file per tag, used as the GitHub release body.
- `docs/media/`: the documents `scripts/capture-media.sh` loads and types when regenerating `screenshots/`.
- `icons/`: generated with `cargo tauri icon` from a 1024px render of numr-apple's `Numr/AppIcon.icon`, produced on a Mac by `scripts/render-apple-icon.sh` (actool + IconServices via `scripts/render-icon.swift`), so Linux, macOS and the Apple app share one icon. `icons/tray/` is hand-made.

## Conventions

- Rust 1.88 MSRV, edition 2021, `cargo fmt` and `cargo clippy --locked --all-targets -- -D warnings` clean.
- Desktop-only. No mobile targets, no Windows bundles; the icon set is trimmed accordingly.
- The Linux renderer is WebKitGTK on native Wayland. Do not reintroduce X11 forcing or CEF.
- Native application menus are macOS-only; on Linux the shell keeps the frameless window and the webview bridge handles the shortcuts.
- Commit messages: conventional prefixes (`feat:`, `fix:`, `docs:`, `ci:`), no co-author trailers.

## Release Checklist

1. In numr-web, make sure the desktop bridge and web runtime are at the state to ship, then update the submodule pointer here: `git -C numr-web checkout <commit>` and `git add numr-web`.
2. Bump `version` in `Cargo.toml`; run `cargo build` so `Cargo.lock` follows. `tauri.conf.json` has no version field on purpose and inherits this one.
3. Write `docs/releases/vX.Y.Z.md`.
4. Gates: `sh scripts/build-ui.sh`, `cargo fmt --all -- --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo tauri build --bundles deb`, and `makepkg` of `aur-git/PKGBUILD` in a scratch directory.
5. Commit, then tag `vX.Y.Z` and push the tag. `.github/workflows/release.yml` verifies the tag matches `Cargo.toml` and that the notes file exists, builds on `ubuntu-22.04`, `ubuntu-22.04-arm`, `macos-latest` (arm64) and `macos-15-intel` with `tauri-action`, publishes the GitHub release, then updates the AUR package and the Homebrew cask. Pre-release tags (containing `-`) skip AUR and Homebrew.
6. Afterwards check the AUR page and `brew install --cask numr-desktop` on a Mac.

Asset names the packaging relies on: `numr_X.Y.Z_amd64.deb`, `numr_X.Y.Z_arm64.deb`, `numr_X.Y.Z_aarch64.dmg`, `numr_X.Y.Z_x64.dmg`. Changing `productName` changes them and the `aur/`, `homebrew/`, and workflow references.

## Secrets

Repository secrets used by `release.yml`: `AUR_USERNAME`, `AUR_EMAIL`, `AUR_SSH_PRIVATE_KEY` (key registered on the AUR account), `HOMEBREW_TAP_DEPLOY_KEY` (write deploy key of `nasedkinpv/homebrew-tap`). macOS bundles are unsigned and un-notarized until `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, and `APPLE_TEAM_ID` are configured for `tauri-action`.

## README Media

`scripts/capture-media.sh` regenerates `screenshots/finances.png` and `screenshots/typing.gif`. It is Hyprland-specific: it builds an opaque copy of the UI, floats the window at a fixed size, strips compositor rounding, border, blur and shadow (and verifies that by sampling the corners), hides the cursor, loads `docs/media/screenshot.numr` for the PNG, and types `docs/media/demo.numr` while `wf-recorder` records in RGB. Required tools and caveats are in the script header; keep the pointer off other windows while it runs.
