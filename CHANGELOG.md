# Changelog

## v0.5.0 — Play any RoF2 server

The launcher now works with **any** EverQuest RoF2 server, not just The Last Camp —
while keeping TLC as the default, featured, one-click experience.

- **Custom server option.** In *Server Status → Change*, enter any server's login
  `host:port`. The launcher pins it into `eqhost.txt` and launches your RoF2 client on
  Apple Silicon through the same Metal-accelerated stack. Leave it on The Last Camp for
  the full turnkey experience.
- **The Last Camp stays the only auto-patched server** (with live status + mods). For
  any other server the launcher just sets your login server and launches your existing,
  already-patched client.
- No server presets baked in — one field, works with every RoF2 server.
- Open source (MIT) — improvements welcome via pull request.

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
