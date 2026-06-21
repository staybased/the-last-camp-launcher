# The Last Camp Launcher

**EverQuest, native on your Apple Silicon Mac. One click into Norrath.**

The Last Camp Launcher runs the EverQuest **RoF2** client on Apple Silicon (M1–M4)
through a Metal-accelerated translation stack — no Boot Camp, no CrossOver license,
no hand-wiring Wine. Open it, press **Play**, and you're in
[The Last Camp](https://thelastcamp.net): a free, Planes-of-Power-locked EverQuest
server. It keeps your client patched, updates itself, and can drop a one-click Dock
shortcut — and it'll happily launch *any* RoF2 server, not just ours.

## Why it exists

EverQuest never had a clean path onto a modern Mac, and Apple Silicon made it harder
— until the translation tooling matured. This launcher packages that work into
something a player can actually use: the RoF2 client rendered through
**DXVK → MoltenVK → Metal**, running on the real GPU of an M-series chip. The goal is
simple — bring EverQuest to every player, whatever they're sitting in front of.

## Features

- **One-click play** — checks for client updates, patches them, and launches the game.
- **Plays any RoF2 server** — defaults to The Last Camp; point it at any server's
  login host to play elsewhere (see [Playing another server](#playing-another-server)).
- **One-click Dock shortcut** — generate a Dock app that launches the game directly,
  skipping the launcher entirely.
- **Smart auto-patcher** (The Last Camp) — diffs your client against the live manifest
  (SHA-256 verified) and downloads only what changed.
- **First-run setup wizard** — walks you through the one-time runtime setup.
- **Self-updating** — the launcher updates itself when a new version ships.
- **Player dashboard** — live server status, optional mod toggles, and patch notes, in-app.

## Requirements

- **A Mac with Apple Silicon (M1–M4)** running a recent macOS (Sonoma or later), **or
  Windows 10/11**.
- **[Whisky](https://getwhisky.app)** (macOS only) — a free Wine/GPTK runtime. The setup
  wizard links you to it; install it once.
- **An EverQuest RoF2 client** — the folder that contains `eqgame.exe`. The launcher does
  **not** distribute the client; you bring your own. Don't have one yet? Ask in the
  [Discord](https://discord.gg/bpv2FWpFg2).

## How to install

### macOS (Apple Silicon)

1. **Download** `The Last Camp Launcher_<version>_aarch64.dmg` from the
   [Releases](../../releases/latest) page and open it; drag the app to **Applications**.
2. **First launch.** This beta is unsigned, so macOS says *"Apple could not verify…"*.
   Open **System Settings → Privacy & Security**, scroll to the prompt for *The Last Camp
   Launcher*, and click **Open Anyway**. (Or run once in Terminal:
   `xattr -dr com.apple.quarantine "/Applications/The Last Camp Launcher.app"`.) This is
   only because we haven't paid for an Apple signing certificate — the app is open source;
   read every line if you like.
3. **Run the setup wizard** (opens automatically on first launch):
   1. **Install Whisky** — the wizard opens [getwhisky.app](https://getwhisky.app); install it, then return.
   2. **Initialize the Wine prefix** — one click.
   3. **Locate your EverQuest client** — point the launcher at your RoF2 folder (the one with `eqgame.exe`). No client yet? Ask in [Discord](https://discord.gg/bpv2FWpFg2).
   4. **Set login server** — one click (pins The Last Camp by default).
4. Press **Enter World** — or **Add Dock Shortcut** for true one-click play next time.

### Windows (10/11)

1. Download the latest installer from [Releases](../../releases/latest) and run it.
2. If SmartScreen appears, click **More info → Run anyway** (unsigned beta).
3. Run the setup wizard (locate your RoF2 client, set the login server) and press **Enter World**.

## One-click Dock shortcut

Click **Add Dock Shortcut** (in the Server Status panel) and the launcher creates a
standalone app that launches the game **directly, skipping the launcher**:

- **macOS:** a *"The Last Camp"* app in `~/Applications` — drag it to your Dock.
- **Windows:** a shortcut on your Desktop.

The shortcut launches your client on Apple Silicon and connects to whichever server you
currently have selected.

## Playing another server

The Last Camp is the default, but the launcher works with **any** RoF2 server. In the
**Server Status** panel, click **Change**, enter the server's login `host:port` (e.g.
`login.example.com:5999`), and **Connect**. The launcher pins it into `eqhost.txt` and
launches your client through the same Apple-Silicon stack. To switch back, use **Use The
Last Camp**.

Note: auto-patching, live status, and the mod toggles are The Last Camp services. For
other servers the launcher simply sets your login server and launches your existing,
already-patched client.

## FAQ

**Do I need to already have EverQuest?**
Yes — a RoF2 client (the folder with `eqgame.exe`). The launcher doesn't distribute the
client; it sets it up to run on your Mac and connects it to the server. Ask in
[Discord](https://discord.gg/bpv2FWpFg2) if you need help getting one.

**Why does macOS say the developer "can't be verified"?**
The beta is unsigned (no paid Apple certificate). It's open source — the code is right
here. Open it via **System Settings → Privacy & Security → Open Anyway** once and you're
set. (The Dock shortcut you create later has no warning at all — it's made on your machine.)

**Does it work on Intel Macs?**
It's built and tested for **Apple Silicon (M1–M4)**. The Metal acceleration is the whole
point on M-series chips.

**Is it really free / open source?**
Yes. Free to play, free to read, MIT-licensed. Improvements welcome via pull request.

**Can I play servers other than The Last Camp?**
Yes — see [Playing another server](#playing-another-server).

**Does it change my EverQuest install?**
It writes `eqhost.txt` (your login server) and, for The Last Camp, patches client files
to keep them current. Mod toggles are optional and revertible.

## The server

The Last Camp is a free, hand-tuned, Planes-of-Power-locked EverQuest server — classic
through PoP, any race, any class, any deity. → **[thelastcamp.net](https://thelastcamp.net)**

## Credits

**Lead Developer — Brother**, The Last Camp.

Built with [Tauri](https://tauri.app) (Rust + web frontend).

## Contributing

This is an open project (MIT) — improvements are welcome. If you make the launcher
better — a fix, a platform tweak, a feature — **open a pull request** and we'll review
it. Bug reports and suggestions via Issues are welcome too. Please keep the credit line
intact (it's all the license asks).

## Development

```bash
pnpm install
pnpm tauri dev      # run the launcher locally
pnpm tauri build    # produce a release artifact
```

- Client / patcher / launch logic: `src-tauri/src/lib.rs`
- Frontend: `src/`
- Icons: `src-tauri/icons/` (regenerate with `pnpm tauri icon <1024px.png>`)

Release builds are signed by the Tauri updater (minisign). The signing key is provided
to CI via the `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets;
it is never committed.
