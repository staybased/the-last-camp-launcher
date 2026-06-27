# Changelog

## v0.7.0 — Windows crash self-heal (LAA + dGPU)

- **Windows installs now self-heal the two fixes a RoF2 client needs to stop
  `c0000005` crashes**, automatically — on every patch *and* every launch:
  - **LargeAddressAware**: the 32-bit `eqgame.exe` is PE-patched so it can use
    >2 GB, eliminating the ~10-minute memory-exhaustion crash (original backed
    up to `eqgame.exe.bak`).
  - **High-performance GPU**: `eqgame.exe` is pinned to the discrete GPU via the
    Windows graphics preference, so hybrid-GPU laptops stop access-violating on
    the integrated GPU.
- Both are idempotent and best-effort (a failure is logged, never blocks play),
  and only the discrete-GPU preference is written if you haven't set one. This
  brings the launcher to parity with the standalone `Crushbone-Patcher.bat`
  self-heal. macOS/Linux unaffected.

## v0.6.1 — Bring-your-own-client wording

- The first-run setup wizard now states **bring-your-own-client** plainly at the "Locate
  your EverQuest client" step, instead of pointing you to Discord to obtain a client. The
  launcher runs the RoF2 client you already have; it doesn't distribute the client. (README
  updated to match.) No functional changes.

## v0.6.0 — One-click Dock shortcut

- **Add Dock Shortcut** (Server Status panel) creates a standalone app that launches the
  game directly, **skipping the launcher**. macOS: drops "The Last Camp" in
  `~/Applications` — drag it to your Dock for true one-click play. Windows: a shortcut on
  the Desktop. The shortcut uses the same Apple-Silicon launch and connects to whatever
  server you currently have selected.

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
