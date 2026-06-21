# The Last Camp Launcher

**EverQuest, native on your Apple Silicon Mac. One click into Norrath.**

The Last Camp Launcher runs the EverQuest **RoF2** client on Apple Silicon (M1–M4)
through a Metal-accelerated translation stack — no Boot Camp, no CrossOver license,
no hand-wiring Wine. Download it, open it, press **Play**, and you're in
[The Last Camp](https://thelastcamp.net): a free, Planes-of-Power-locked EverQuest
server. It keeps your client patched automatically and updates itself, so your time
goes into Norrath instead of into setup.

## Why it exists

EverQuest never had a clean path onto a modern Mac, and Apple Silicon made it harder
— until the translation tooling matured. This launcher packages that work into
something a player can actually use: the RoF2 client rendered through
**DXVK → MoltenVK → Metal**, running on the real GPU of an M-series chip. The goal is
simple — bring EverQuest to every player, whatever they're sitting in front of.

## Features

- **One-click play** — checks for client updates, patches them, and launches the game.
- **Smart auto-patcher** — diffs your client against the live manifest (SHA-256 verified)
  and downloads only what changed.
- **Bring your own client** — point it at an existing RoF2 EverQuest install; the
  launcher handles patching and launch.
- **First-run setup wizard** — walks macOS players through the one-time runtime setup.
- **Self-updating** — the launcher updates itself when a new version ships.
- **Player dashboard** — live server status, optional mod toggles, and patch notes, in-app.

## Platforms

- **macOS — Apple Silicon (M1–M4)** — the primary target; native Metal performance.
- **Windows** — supported from the same codebase.

## Install

1. Download the latest build from the [Releases](../../releases) page.
2. **macOS first launch (beta builds are unsigned):** macOS will say the developer
   can't be verified. Open **System Settings → Privacy & Security**, scroll to the
   prompt for The Last Camp Launcher, and click **Open Anyway**. (Or, in Terminal:
   `xattr -dr com.apple.quarantine "/Applications/The Last Camp Launcher.app"`.)
3. **Windows first launch:** if SmartScreen appears, click **More info → Run anyway**.

Once it opens, the setup wizard takes you the rest of the way.

## The server

The Last Camp is a free, hand-tuned, Planes-of-Power-locked EverQuest server — classic
through PoP, any race, any class, any deity. → **[thelastcamp.net](https://thelastcamp.net)**

## Credits

**Lead Developer — Brother**, The Last Camp.

Built with [Tauri](https://tauri.app) (Rust + web frontend).

## Contributing

This is an open project (MIT) — improvements are welcome. If you make the launcher
better — a fix, a new server profile, a platform tweak — **open a pull request** and
we'll review it. Bug reports and suggestions via Issues are welcome too. Please keep
the credit line intact (it's all the license asks).

## Development

```bash
pnpm install
pnpm tauri dev      # run the launcher locally
pnpm tauri build    # produce a release artifact
```

- Client/patcher logic: `src-tauri/src/lib.rs`
- Frontend: `src/`
- Icons: `src-tauri/icons/` (regenerate with `pnpm tauri icon <1024px.png>`)

Release builds are signed by the Tauri updater (minisign). The signing key is provided
to CI via the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets;
it is never committed.
