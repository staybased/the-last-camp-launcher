# Crushbone Launcher — Windows build & release

The launcher is cross-platform Tauri. macOS builds locally; **Windows must build
on Windows** (MSVC toolchain) — we use GitHub Actions. This is the front-to-back
runbook.

## What's already wired

- `src-tauri/src/lib.rs` — native Windows launch (`eqgame.exe patchme`, no Wine),
  platform-aware `eq_dir` + persisted override, manifest `delete`-list parity,
  setup/preflight gated for Windows, `get_platform` / `get_eq_dir` / `set_eq_dir`.
- `src/main.js` — wizard hides Whisky/Wine steps on Windows and shows a
  "use existing folder" path field (so players with a client don't re-download).
- `src-tauri/tauri.conf.json` — `nsis` bundle target + WebView2 download
  bootstrapper + per-user install.
- `.github/workflows/build-windows.yml` — builds + signs the installer.
- `scripts/deploy-windows-launcher.sh` — publishes a build to the patch host.

## One-time setup

1. **Push this repo to a (private) GitHub repo.** Tauri Windows builds need a
   `windows-latest` runner.
2. **Add Actions secrets** (Settings → Secrets and variables → Actions):
   - `TAURI_SIGNING_PRIVATE_KEY` — the minisign private key whose public key is
     baked into `tauri.conf.json` (`plugins.updater.pubkey`). This is the SAME
     key used to sign the macOS launcher; reuse it so one manifest serves both
     platforms.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — that key's password.

   > Lost the key? You can rotate: generate a new pair with
   > `pnpm tauri signer generate`, replace `pubkey` in `tauri.conf.json`, and
   > note that existing installs (Mac) won't auto-update across the key change.

## Cut a release

> **Ship every platform at the same version.** `tauri-plugin-updater` serves one
> `version` for all platforms in a single `manifest.json`. If you bump the
> manifest to a new version but leave a platform's artifact untouched, users on
> that platform get told to update, download the *old* artifact, and loop
> forever. So a release means building **both** Windows (CI) and macOS (local)
> at the new version. `deploy-windows-launcher.sh` refuses a version bump that
> would strand a platform (override: `--allow-stale-platforms`).

1. Bump `version` in `src-tauri/tauri.conf.json`, `package.json`, and
   `src-tauri/Cargo.toml` (run `cargo check` once so `Cargo.lock` follows).
2. Tag and push — this builds **Windows** on CI:
   ```bash
   git tag launcher-v0.4.0 && git push origin launcher-v0.4.0
   ```
   (or run the workflow manually from the Actions tab).
3. When the run finishes, download the **crushbone-launcher-windows** artifact.
   It contains:
   - `Crushbone Launcher_<ver>_x64-setup.exe` — the installer AND self-update
     payload (Tauri v2 NSIS updates by re-running the installer)
   - `..._x64-setup.exe.sig` — its updater signature
4. Build **macOS** locally at the same version (needs the signing env so the
   updater artifact is signed):
   ```bash
   export TAURI_SIGNING_PRIVATE_KEY="$(cat /path/to/minisign.key)"
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="…"
   pnpm tauri build           # -> src-tauri/target/release/bundle/{macos,dmg}/
   ```
5. Publish **both** platforms at parity:
   ```bash
   # Preview first — prints the merged manifest, writes nothing:
   scripts/deploy-windows-launcher.sh --version 0.4.0 \
     --win ~/Downloads/crushbone-launcher-windows \
     --mac src-tauri/target/release/bundle --dry-run

   # Then deploy for real:
   scripts/deploy-windows-launcher.sh --version 0.4.0 \
     --win ~/Downloads/crushbone-launcher-windows \
     --mac src-tauri/target/release/bundle
   ```
   This uploads each platform's installer (+ the stable
   `Crushbone-Launcher-Setup.exe` / `Crushbone-Launcher.dmg` aliases), backs up
   every file it overwrites (timestamped `.bak`), and merges both
   `windows-x86_64` and `darwin-aarch64` entries into the served
   `launcher/manifest.json` at the new version. Windows artifact names are
   space-sanitized so the manifest URL is valid.

## Player distribution

- **Fresh install:** share `https://patch.crushbone.live/patch/launcher/Crushbone-Launcher-Setup.exe`.
  The launcher then either detects an existing client folder, lets the player
  paste one, or downloads + patches a fresh client.
- **Bundle in the client zip:** drop the installer into `crushbone-client-v1.x.zip`
  so new full installs get the launcher.
- **Existing Windows players (no launcher yet):** the standalone
  `Crushbone-Patcher.bat` (in the client folder) covers them until they install
  the launcher.
- **Updates:** once installed, the launcher self-updates from `manifest.json` on
  next launch — no player action.

## SmartScreen / signing

The installer is signed with the Tauri **updater** key (for self-update
integrity) but is **not** Authenticode code-signed, so Windows SmartScreen shows
"Windows protected your PC" on first run. Players click **More info → Run
anyway**. To remove the warning later, Authenticode-sign the `.exe` in CI
(Azure Trusted Signing is the cheap modern option) — no launcher code changes
needed.

## Local sanity checks (macOS dev box)

```bash
# Frontend syntax
node --check src/main.js
# Rust typecheck for the Windows cfg path (needs mingw-w64 + the gnu target)
cd src-tauri && cargo check --target x86_64-pc-windows-gnu --lib
# macOS build still works
cd src-tauri && cargo check
```
