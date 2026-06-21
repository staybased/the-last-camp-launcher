# Changelog

## v0.4.0 — Beta (first public release)

The first public build of **The Last Camp Launcher** — EverQuest on Apple Silicon,
one click into [The Last Camp](https://thelastcamp.net).

### Highlights
- **EverQuest on Apple Silicon (M1–M4).** Runs the RoF2 client through a
  Metal-accelerated translation stack (DXVK → MoltenVK → Metal) — real M-series GPU
  performance, no Boot Camp or CrossOver.
- **One-click play.** Checks for client updates, patches them, and launches the game
  from a single button.
- **Smart auto-patcher.** Diffs your client against the live manifest (SHA-256
  verified) and downloads only the files that changed.
- **Bring your own client.** Point the launcher at an existing RoF2 EverQuest install;
  it handles patching and launch.
- **First-run setup wizard** for the one-time macOS runtime setup.
- **Self-updating** launcher (Tauri updater, minisign-signed).
- **Player dashboard:** live server status, mod toggles, and patch notes in-app.
- **Windows** supported from the same codebase.

### Notes
- This is an **unsigned beta**. On macOS, first launch needs
  **System Settings → Privacy & Security → Open Anyway** (or
  `xattr -dr com.apple.quarantine "/Applications/The Last Camp Launcher.app"`).
  On Windows, SmartScreen → **More info → Run anyway**. Code signing / notarization
  is planned.

Lead Developer — **Brother**, The Last Camp.
