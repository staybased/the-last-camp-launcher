use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tar::Archive;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

const STATUS_URL: &str = "https://status.thelastcamp.net/status.json";
const PROBE_URL: &str = "https://patch.thelastcamp.net/";
const MANIFEST_URL: &str = "https://patch.thelastcamp.net/patch/manifest.json";
const FALLBACK_HOST: &str = "play.thelastcamp.net:5999";
const SERVER_NAME: &str = "The Last Camp";

// Whisky paths — Whisky bundles a Wine + GPTK runtime we can drive directly.
// macOS/Linux only; Windows runs eqgame.exe natively.
#[cfg(not(target_os = "windows"))]
const WHISKY_APP: &str = "/Applications/Whisky.app";
#[cfg(not(target_os = "windows"))]
const WHISKY_WINE_REL: &str =
    "Library/Application Support/com.isaacmarovitz.Whisky/Libraries/Wine/bin/wine64";
#[cfg(not(target_os = "windows"))]
const WHISKY_DOWNLOAD_PAGE: &str = "https://getwhisky.app/";

const VERIFY_FILES: &[(&str, u64)] = &[
    ("eqgame.exe", 1_000_000),
    ("eqclient.dll", 100_000),
    ("eqmain.dll", 100_000),
    ("eqhost.txt", 10),
];

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerStatus {
    pub online: bool,
    pub players: u32,
    pub uptime_seconds: u64,
    pub server_name: String,
    pub host: String,
    pub last_check_unix: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModState {
    pub old_models: bool,
    pub classic_spells: bool,
    pub takp_icons: bool,
    pub perf_preset: String,
}

impl Default for ModState {
    fn default() -> Self {
        Self {
            old_models: false,
            classic_spells: false,
            takp_icons: false,
            perf_preset: "medium".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchEntry {
    pub date: String,
    pub title: String,
    pub body: String,
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "could not resolve home directory".to_string())
}

fn data_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_dir()
        .ok_or_else(|| "could not resolve data directory".to_string())?
        .join("Crushbone");
    fs::create_dir_all(&dir).map_err(|e| format!("create data dir: {e}"))?;
    Ok(dir)
}

fn state_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("launcher.json"))
}

// Game-files location. New installs land in the launcher-owned data dir
// (macOS: ~/Library/Application Support/Crushbone/everquest_rof2; Windows:
// %APPDATA%\Crushbone\everquest_rof2). Existing installs elsewhere keep
// working without a forced move/re-download.
fn eq_dir_canonical() -> Result<PathBuf, String> {
    // Matches the top-level directory inside crushbone-client-v1.x.zip so
    // extracting straight into data_dir produces this path.
    Ok(data_dir()?.join("everquest_rof2"))
}

// User-chosen game folder, persisted across runs (plain path in a text file).
// Lets a player point the launcher at a client they already have (e.g. an
// existing C:\Games\Crushbone) instead of re-downloading the base client.
fn eq_dir_override_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("eqdir.txt"))
}

fn eq_dir_override() -> Option<PathBuf> {
    let p = eq_dir_override_file().ok()?;
    let raw = fs::read_to_string(&p).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

// --- Server selection ---------------------------------------------------------
// The launcher pins one login server into eqhost.txt and launches the client
// against it. The Last Camp is the default/featured server; a player can point
// the launcher at ANY RoF2 server by setting a custom "host:port". Persisted as
// plain text (mirrors the eqdir.txt pattern) — empty/missing means The Last Camp.

fn server_host_file() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("server.txt"))
}

/// The login server the launcher pins + connects to. Defaults to The Last Camp.
fn active_host() -> String {
    server_host_file()
        .ok()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| FALLBACK_HOST.to_string())
}

fn is_tlc_server() -> bool {
    active_host() == FALLBACK_HOST
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerSelection {
    pub host: String,
    pub is_tlc: bool,
}

/// Current login server (host:port) + whether it is The Last Camp.
#[tauri::command]
fn get_server() -> ServerSelection {
    let host = active_host();
    ServerSelection {
        is_tlc: host == FALLBACK_HOST,
        host,
    }
}

/// Point the launcher at a server. Empty input (or the TLC host) resets to The
/// Last Camp. Any other "host:port" is validated, persisted, and immediately
/// pinned into eqhost.txt so the next launch connects there.
#[tauri::command]
fn set_server(host: String) -> Result<ServerSelection, String> {
    let trimmed = host.trim();
    let path = server_host_file()?;
    if trimmed.is_empty() || trimmed == FALLBACK_HOST {
        let _ = fs::remove_file(&path);
    } else {
        let (h, p) = trimmed.rsplit_once(':').ok_or_else(|| {
            "Enter the server as host:port (e.g. login.example.com:5999)".to_string()
        })?;
        if h.is_empty() {
            return Err("Missing the host before the ':'".into());
        }
        match p.parse::<u32>() {
            Ok(port) if (1..=65535).contains(&port) => {}
            _ => return Err("Port must be a number between 1 and 65535 (e.g. 5999)".into()),
        }
        fs::write(&path, trimmed).map_err(|e| format!("save server: {e}"))?;
    }
    // Re-pin eqhost.txt now so the switch takes effect. Ignore if the client
    // folder isn't set up yet — eqhost gets written during setup either way.
    let _ = setup_eqhost();
    Ok(get_server())
}

// Platform-specific default search locations (after any explicit override).
#[cfg(target_os = "windows")]
fn eq_dir_platform_candidates() -> Result<Vec<PathBuf>, String> {
    let mut v = vec![eq_dir_canonical()?];
    // Common spots Windows players install to.
    for p in [
        r"C:\Games\Crushbone",
        r"C:\Games\everquest_rof2",
        r"C:\Crushbone",
        r"C:\everquest_rof2",
    ] {
        v.push(PathBuf::from(p));
    }
    if let Ok(home) = home_dir() {
        v.push(home.join("Crushbone"));
        v.push(home.join("Games").join("everquest_rof2"));
    }
    Ok(v)
}

#[cfg(not(target_os = "windows"))]
fn eq_dir_platform_candidates() -> Result<Vec<PathBuf>, String> {
    Ok(vec![
        eq_dir_canonical()?,
        home_dir()?.join("Games/everquest_rof2"),
        home_dir()?.join("Downloads/eq/rof2/everquest_rof2"),
    ])
}

fn eq_dir() -> Result<PathBuf, String> {
    // 1) Honor an explicit user override if it actually holds a client.
    if let Some(over) = eq_dir_override() {
        if over.join("eqgame.exe").exists() {
            return Ok(over);
        }
    }
    // 2) Fall back to platform candidates that already have eqgame.exe.
    for path in eq_dir_platform_candidates()? {
        if path.join("eqgame.exe").exists() {
            return Ok(path);
        }
    }
    // 3) An override pointing at a not-yet-populated folder still wins as the
    //    install target (first-run setup will materialize it there).
    if let Some(over) = eq_dir_override() {
        return Ok(over);
    }
    // 4) Default to the launcher-owned canonical location.
    eq_dir_canonical()
}

/// "windows" | "macos" | "linux" — lets the frontend tailor the setup wizard
/// (e.g. hide the Whisky/Wine steps on Windows).
#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
fn get_eq_dir() -> Result<String, String> {
    Ok(eq_dir()?.display().to_string())
}

/// Persist a user-chosen game folder. Accepts any existing directory; the
/// folder need not contain eqgame.exe yet (fresh-install target).
#[tauri::command]
fn set_eq_dir(path: String) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        // Clear the override -> revert to auto-detection.
        let _ = fs::remove_file(eq_dir_override_file()?);
        return get_eq_dir();
    }
    let p = PathBuf::from(trimmed);
    if !p.is_dir() {
        return Err(format!("folder does not exist: {}", p.display()));
    }
    fs::write(eq_dir_override_file()?, p.display().to_string())
        .map_err(|e| format!("save game folder: {e}"))?;
    Ok(p.display().to_string())
}

#[cfg(not(target_os = "windows"))]
fn whisky_wine_bin() -> Result<PathBuf, String> {
    Ok(home_dir()?.join(WHISKY_WINE_REL))
}

#[cfg(not(target_os = "windows"))]
fn wine_prefix() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("prefix"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[tauri::command]
async fn get_server_status() -> Result<ServerStatus, String> {
    let now = now_unix();
    // Custom (non-TLC) server: the launcher doesn't track live status elsewhere.
    if !is_tlc_server() {
        let host = active_host();
        return Ok(ServerStatus {
            online: false,
            players: 0,
            uptime_seconds: 0,
            server_name: host.clone(),
            host,
            last_check_unix: now,
        });
    }
    let mut players: u32 = 0;
    let mut uptime: u64 = 0;
    let mut host = FALLBACK_HOST.to_string();

    // Try the optional /status endpoint first for richer data.
    if let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        if let Ok(resp) = client.get(STATUS_URL).send().await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    players = json.get("players").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    uptime = json
                        .get("uptime_seconds")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if let Some(h) = json.get("host").and_then(|v| v.as_str()) {
                        host = h.to_string();
                    }
                    return Ok(ServerStatus {
                        online: true,
                        players,
                        uptime_seconds: uptime,
                        server_name: SERVER_NAME.into(),
                        host,
                        last_check_unix: now,
                    });
                }
            }
        }
    }

    // Fall back to an HTTP probe of the Caddy front door — any HTTP
    // status (including redirects) means the box is reachable.
    let online = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c
            .head(PROBE_URL)
            .send()
            .await
            .map(|r| r.status().as_u16() < 500)
            .unwrap_or(false),
        Err(_) => false,
    };

    Ok(ServerStatus {
        online,
        players,
        uptime_seconds: uptime,
        server_name: SERVER_NAME.into(),
        host,
        last_check_unix: now,
    })
}

#[tauri::command]
fn get_mod_state() -> Result<ModState, String> {
    let path = state_file()?;
    if !path.exists() {
        return Ok(ModState::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| format!("read state: {e}"))?;
    let state: ModState = serde_json::from_str(&content).unwrap_or_else(|_| ModState::default());
    Ok(state)
}

#[tauri::command]
fn set_mod_state(state: ModState) -> Result<ModState, String> {
    let path = state_file()?;
    let serialized =
        serde_json::to_string_pretty(&state).map_err(|e| format!("serialize state: {e}"))?;
    fs::write(&path, serialized).map_err(|e| format!("write state: {e}"))?;
    apply_mod_state(&state)?;
    Ok(state)
}

fn apply_mod_state(state: &ModState) -> Result<(), String> {
    let eq = eq_dir()?;
    if !eq.exists() {
        return Ok(());
    }
    let mods_root = eq.join("_mods");

    apply_pack(&eq, &mods_root.join("old_models"), state.old_models)?;
    apply_pack(&eq, &mods_root.join("classic_spells"), state.classic_spells)?;
    apply_pack(&eq, &mods_root.join("takp_icons"), state.takp_icons)?;
    Ok(())
}

fn apply_pack(eq_dir: &Path, pack_dir: &Path, enable: bool) -> Result<(), String> {
    if !pack_dir.exists() {
        return Ok(());
    }

    let entries = fs::read_dir(pack_dir).map_err(|e| format!("read pack dir: {e}"))?;
    for entry in entries.flatten() {
        let src = entry.path();
        let Some(name) = src.file_name() else {
            continue;
        };
        let dst = eq_dir.join(name);

        if enable {
            if dst.exists() || dst.symlink_metadata().is_ok() {
                if is_symlink_to(&dst, pack_dir) {
                    let _ = fs::remove_file(&dst);
                } else {
                    continue;
                }
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, &dst)
                .map_err(|e| format!("symlink {}: {e}", dst.display()))?;
        } else if is_symlink_to(&dst, pack_dir) {
            let _ = fs::remove_file(&dst);
        }
    }
    Ok(())
}

fn is_symlink_to(path: &Path, pack_dir: &Path) -> bool {
    let Ok(meta) = path.symlink_metadata() else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(target) = fs::read_link(path) else {
        return false;
    };
    let resolved = if target.is_absolute() {
        target
    } else {
        path.parent().map(|p| p.join(&target)).unwrap_or(target)
    };
    resolved.starts_with(pack_dir)
}

/// Launch the EQ client. Windows runs eqgame.exe natively; macOS/Linux drive
/// it through Whisky's bundled Wine + GPTK runtime.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn launch_eq() -> Result<(), String> {
    let eq = eq_dir()?;
    let exe = eq.join("eqgame.exe");
    if !exe.exists() {
        return Err(format!(
            "EQ client not installed at {}. Run setup first.",
            eq.display()
        ));
    }
    // RoF2 requires "patchme" so it skips the retired Sony patcher and connects
    // straight to The Last Camp login server pinned in eqhost.txt.
    Command::new(&exe)
        .current_dir(&eq)
        .arg("patchme")
        .spawn()
        .map_err(|e| format!("spawn eqgame.exe: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn launch_eq() -> Result<(), String> {
    let wine = whisky_wine_bin()?;
    if !wine.exists() {
        return Err(format!(
            "Whisky's wine64 not found at {}. Install Whisky from {}.",
            wine.display(),
            WHISKY_DOWNLOAD_PAGE
        ));
    }
    let prefix = wine_prefix()?;
    if !prefix.exists() {
        return Err(format!(
            "Wine prefix missing at {}. Run setup first.",
            prefix.display()
        ));
    }
    let eq = eq_dir()?;
    if !eq.join("eqgame.exe").exists() {
        return Err(format!(
            "EQ client not installed at {}. Run setup first.",
            eq.display()
        ));
    }

    let log_path = data_dir()?.join("eqgame.log");
    let log = fs::File::create(&log_path).map_err(|e| format!("create log: {e}"))?;
    let log_err = log
        .try_clone()
        .map_err(|e| format!("clone log handle: {e}"))?;

    Command::new(&wine)
        .current_dir(&eq)
        .arg("eqgame.exe")
        .arg("patchme")
        .env("WINEPREFIX", &prefix)
        .env("WINEDEBUG", "-all")
        .env(
            "WINEDLLOVERRIDES",
            "d3d9,d3d10core,d3d11,d3d12,dxgi=n,b",
        )
        .env("WINEESYNC", "1")
        .env("WINEFSYNC", "1")
        .env("DXVK_ASYNC", "1")
        .stdout(log)
        .stderr(log_err)
        .spawn()
        .map_err(|e| format!("spawn wine64: {e}"))?;
    Ok(())
}

#[tauri::command]
fn get_patch_notes() -> Result<Vec<PatchEntry>, String> {
    let candidates = [
        eq_dir()?.join("CHANGELOG.md"),
        home_dir()?.join("crushbone-dist/CHANGELOG.md"),
        home_dir()?.join("Games/everquest_rof2/CHANGELOG.md"),
    ];

    let path = candidates
        .iter()
        .find(|p| p.exists())
        .ok_or_else(|| "CHANGELOG.md not found".to_string())?;

    let content = fs::read_to_string(path).map_err(|e| format!("read changelog: {e}"))?;
    Ok(parse_changelog(&content, 8))
}

fn parse_changelog(content: &str, limit: usize) -> Vec<PatchEntry> {
    let mut entries: Vec<PatchEntry> = Vec::new();
    let mut current: Option<PatchEntry> = None;
    let mut body_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(mut entry) = current.take() {
                entry.body = body_lines.join("\n").trim().to_string();
                entries.push(entry);
                body_lines.clear();
            }
            let (date, title) = match rest.split_once(" — ") {
                Some((d, t)) => (d.trim().to_string(), t.trim().to_string()),
                None => (rest.trim().to_string(), String::new()),
            };
            current = Some(PatchEntry {
                date,
                title,
                body: String::new(),
            });
        } else if current.is_some() {
            body_lines.push(line.to_string());
        }
    }

    if let Some(mut entry) = current.take() {
        entry.body = body_lines.join("\n").trim().to_string();
        entries.push(entry);
    }

    entries.truncate(limit);
    entries
}

#[derive(Debug, Serialize, Clone)]
pub struct PreflightCheck {
    pub key: String,
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct PreflightReport {
    pub ready: bool,
    pub checks: Vec<PreflightCheck>,
}

#[derive(Debug, Serialize, Clone)]
pub struct VerifyEntry {
    pub name: String,
    pub ok: bool,
    pub size: u64,
    pub detail: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct VerifyReport {
    pub ok: bool,
    pub entries: Vec<VerifyEntry>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LastCharacter {
    pub name: String,
    pub server: String,
    pub last_played_unix: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModPackInfo {
    pub key: String,
    pub label: String,
    pub installed: bool,
    pub downloadable: bool,
    pub url: Option<String>,
    pub bytes: u64,
}

fn check(key: &str, label: &str, ok: bool, detail: impl Into<String>) -> PreflightCheck {
    PreflightCheck {
        key: key.into(),
        label: label.into(),
        ok,
        detail: detail.into(),
    }
}

#[tauri::command]
fn get_preflight() -> Result<PreflightReport, String> {
    let mut checks = Vec::new();

    // Whisky + Wine prefix are macOS/Linux-only; Windows runs natively.
    #[cfg(not(target_os = "windows"))]
    {
        let whisky_app = Path::new(WHISKY_APP).exists();
        let wine_bin = whisky_wine_bin()?;
        let whisky = whisky_app && wine_bin.exists();
        checks.push(check(
            "whisky",
            "Whisky installed",
            whisky,
            if whisky {
                WHISKY_APP.to_string()
            } else {
                format!("Download from {}", WHISKY_DOWNLOAD_PAGE)
            },
        ));

        let prefix = wine_prefix()?;
        let prefix_ready = prefix.join("system.reg").exists();
        checks.push(check(
            "wine_prefix",
            "Wine prefix initialized",
            prefix_ready,
            if prefix_ready {
                prefix.display().to_string()
            } else {
                "Run setup to initialize prefix".into()
            },
        ));
    }

    let eq = eq_dir()?;
    let client_dir = eq.exists();
    checks.push(check(
        "client_dir",
        "EQ client folder",
        client_dir,
        eq.display().to_string(),
    ));

    let eqgame = eq.join("eqgame.exe").exists();
    checks.push(check(
        "client_files",
        "RoF2 client files",
        eqgame,
        if eqgame {
            "eqgame.exe present".to_string()
        } else {
            "Run setup to download client".into()
        },
    ));

    let host_path = eq.join("eqhost.txt");
    let expected = format!("Host={}", active_host());
    let host_ok = fs::read_to_string(&host_path)
        .map(|s| s.contains(&expected))
        .unwrap_or(false);
    checks.push(check(
        "eqhost",
        if is_tlc_server() {
            "Login server pinned to The Last Camp"
        } else {
            "Login server pinned"
        },
        host_ok,
        if host_ok {
            expected
        } else {
            "eqhost.txt missing or wrong host".into()
        },
    ));

    let ready = checks.iter().all(|c| c.ok);
    Ok(PreflightReport { ready, checks })
}

#[tauri::command]
fn verify_install() -> Result<VerifyReport, String> {
    let eq = eq_dir()?;
    let mut entries = Vec::new();
    let mut all_ok = true;

    for (name, min_size) in VERIFY_FILES {
        let path = eq.join(name);
        match fs::metadata(&path) {
            Ok(meta) => {
                let size = meta.len();
                let ok = size >= *min_size;
                if !ok {
                    all_ok = false;
                }
                entries.push(VerifyEntry {
                    name: (*name).into(),
                    ok,
                    size,
                    detail: if ok {
                        format!("{} bytes", size)
                    } else {
                        format!("undersized ({} < {})", size, min_size)
                    },
                });
            }
            Err(_) => {
                all_ok = false;
                entries.push(VerifyEntry {
                    name: (*name).into(),
                    ok: false,
                    size: 0,
                    detail: "missing".into(),
                });
            }
        }
    }

    Ok(VerifyReport {
        ok: all_ok,
        entries,
    })
}

#[tauri::command]
fn get_last_character() -> Result<Option<LastCharacter>, String> {
    let eq = eq_dir()?;
    if !eq.exists() {
        return Ok(None);
    }

    let suffix = format!("_{}.ini", SERVER_NAME);
    let mut best: Option<(PathBuf, SystemTime)> = None;

    let entries = fs::read_dir(&eq).map_err(|e| format!("read eq dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(&suffix) {
            continue;
        }
        if name.starts_with("UI_") || name.contains(".bak") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if best.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
            best = Some((path, modified));
        }
    }

    let Some((path, modified)) = best else {
        return Ok(None);
    };
    let stem = path
        .file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.strip_suffix(&suffix))
        .unwrap_or("Adventurer")
        .to_string();
    let last = modified
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Ok(Some(LastCharacter {
        name: stem,
        server: SERVER_NAME.into(),
        last_played_unix: last,
    }))
}

fn mod_pack_registry() -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    vec![
        ("old_models", "Classic Models", None),
        ("classic_spells", "Classic Spell Effects", None),
        ("takp_icons", "TAKP Buff Icons", None),
        (
            "cbz_core",
            "CBZ Quality-of-Life",
            Some("https://patch.thelastcamp.net/patch/mods/cbz_core.tar.gz"),
        ),
        (
            "shinsparxx_ui",
            "Shinsparxx UI",
            Some("https://patch.thelastcamp.net/patch/mods/shinsparxx_ui.tar.gz"),
        ),
    ]
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(read) = fs::read_dir(path) {
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_size(&entry.path());
            }
        }
    }
    total
}

fn is_pack_installed(pack_dir: &Path) -> bool {
    if pack_dir.join(".installed").exists() {
        return true;
    }
    // Legacy path: any non-blob, non-archive file counts as installed
    let Ok(read) = fs::read_dir(pack_dir) else {
        return false;
    };
    for entry in read.flatten() {
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        if name == "__pack.bin" || name.ends_with(".tar.gz") || name.ends_with(".zip") {
            continue;
        }
        return true;
    }
    false
}

#[tauri::command]
fn get_mod_packs() -> Result<Vec<ModPackInfo>, String> {
    let eq = eq_dir()?;
    let mods_root = eq.join("_mods");
    Ok(mod_pack_registry()
        .into_iter()
        .map(|(key, label, url)| {
            let pack_dir = mods_root.join(key);
            let installed = pack_dir.exists() && is_pack_installed(&pack_dir);
            let bytes = if installed { dir_size(&pack_dir) } else { 0 };
            ModPackInfo {
                key: key.into(),
                label: label.into(),
                installed,
                downloadable: url.is_some(),
                url: url.map(|u| u.into()),
                bytes,
            }
        })
        .collect())
}

#[derive(Debug, Serialize, Clone)]
pub struct DownloadProgress {
    pub key: String,
    pub stage: String,
    pub received: u64,
    pub total: u64,
    pub percent: u8,
}

fn emit_progress(app: &AppHandle, key: &str, stage: &str, received: u64, total: u64) {
    let percent = if total > 0 {
        ((received * 100) / total).min(100) as u8
    } else {
        0
    };
    let _ = app.emit(
        "mod-pack-progress",
        DownloadProgress {
            key: key.into(),
            stage: stage.into(),
            received,
            total,
            percent,
        },
    );
}

#[tauri::command]
async fn download_mod_pack(app: AppHandle, key: String) -> Result<ModPackInfo, String> {
    let registry = mod_pack_registry();
    let entry = registry
        .iter()
        .find(|(k, _, _)| *k == key.as_str())
        .ok_or_else(|| format!("unknown mod pack: {key}"))?;
    let url = entry
        .2
        .ok_or_else(|| "this pack has no download URL configured yet".to_string())?;

    let eq = eq_dir()?;
    let pack_dir = eq.join("_mods").join(entry.0);
    fs::create_dir_all(&pack_dir).map_err(|e| format!("create pack dir: {e}"))?;
    let _ = fs::remove_file(pack_dir.join(".installed"));

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    emit_progress(&app, entry.0, "connecting", 0, 0);
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("download: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("server returned {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let archive_path = pack_dir.join("__pack.tar.gz");
    let mut file =
        fs::File::create(&archive_path).map_err(|e| format!("create archive file: {e}"))?;
    let mut stream = resp.bytes_stream();
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("stream: {e}"))?;
        file.write_all(&bytes)
            .map_err(|e| format!("write chunk: {e}"))?;
        received += bytes.len() as u64;
        if received - last_emit > 256 * 1024 {
            emit_progress(&app, entry.0, "downloading", received, total);
            last_emit = received;
        }
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    emit_progress(&app, entry.0, "downloading", received, total.max(received));

    emit_progress(&app, entry.0, "extracting", 0, 0);
    let archive_file =
        fs::File::open(&archive_path).map_err(|e| format!("open archive: {e}"))?;
    let mut archive = Archive::new(GzDecoder::new(archive_file));
    archive
        .unpack(&pack_dir)
        .map_err(|e| format!("extract: {e}"))?;
    let _ = fs::remove_file(&archive_path);

    fs::write(pack_dir.join(".installed"), b"ok").map_err(|e| format!("mark installed: {e}"))?;
    let bytes = dir_size(&pack_dir);
    emit_progress(&app, entry.0, "done", bytes, bytes);

    Ok(ModPackInfo {
        key: entry.0.into(),
        label: entry.1.into(),
        installed: true,
        downloadable: true,
        url: Some(url.into()),
        bytes,
    })
}

// ============================================================
// Auto-patcher — fetches manifest.json, sha256-diffs against the
// local EQ install, downloads any changed/missing files, then
// returns control so the launcher can spawn eqgame.exe.
// ============================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManifestFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub version: String,
    pub patch_url_base: String,
    pub files: Vec<ManifestFile>,
    // Files the client must NOT have (e.g. post-PoP revamp .eqg that the
    // engine would pick over the era-correct .s3d). Removed before patching
    // so the client falls back to classic geometry. Optional for back-compat.
    #[serde(default)]
    pub delete: Vec<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateCheckResult {
    pub manifest_version: String,
    pub files_to_update: Vec<ManifestFile>,
    pub total_bytes: u64,
}

#[derive(Debug, Serialize, Clone)]
pub struct PatchProgress {
    pub stage: String,            // "checking" | "downloading" | "writing" | "done"
    pub current_file: String,
    pub files_done: u32,
    pub files_total: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

fn emit_patch(app: &AppHandle, p: PatchProgress) {
    let _ = app.emit("patch-progress", p);
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

async fn fetch_manifest() -> Result<Manifest, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let resp = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| format!("manifest fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("manifest server returned {}", resp.status()));
    }
    resp.json::<Manifest>()
        .await
        .map_err(|e| format!("manifest parse: {e}"))
}

#[tauri::command]
async fn check_for_updates() -> Result<UpdateCheckResult, String> {
    // Auto-patching is a The Last Camp service. Other servers run their own
    // client + patcher, so report nothing to update and let launch proceed.
    if !is_tlc_server() {
        return Ok(UpdateCheckResult {
            manifest_version: "n/a".into(),
            files_to_update: Vec::new(),
            total_bytes: 0,
        });
    }
    let manifest = fetch_manifest().await?;
    let eq = eq_dir()?;
    let mut needs_update = Vec::new();
    let mut total_bytes: u64 = 0;

    for entry in &manifest.files {
        let local = eq.join(&entry.path);
        let needs = if !local.exists() {
            true
        } else {
            // Cheap size check first; full hash only if size matches
            match fs::metadata(&local) {
                Ok(m) if m.len() != entry.size => true,
                Ok(_) => match sha256_of(&local) {
                    Ok(h) => !h.eq_ignore_ascii_case(&entry.sha256),
                    Err(_) => true,
                },
                Err(_) => true,
            }
        };
        if needs {
            total_bytes += entry.size;
            needs_update.push(entry.clone());
        }
    }

    Ok(UpdateCheckResult {
        manifest_version: manifest.version,
        files_to_update: needs_update,
        total_bytes,
    })
}

#[tauri::command]
async fn apply_updates(app: AppHandle) -> Result<u32, String> {
    // Only The Last Camp is auto-patched; for other servers this is a no-op.
    if !is_tlc_server() {
        let _ = &app;
        return Ok(0);
    }
    let manifest = fetch_manifest().await?;
    let eq = eq_dir()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    // Step 0: remove files the manifest says must not exist (revamp .eqg etc.)
    // so the client falls back to era-correct geometry. Parity with the
    // standalone updater scripts. Ignore paths that aren't present.
    for rel in &manifest.delete {
        let target = eq.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if target.exists() {
            let _ = fs::remove_file(&target);
        }
    }

    // Re-diff (file state may have changed since check)
    let pending: Vec<ManifestFile> = manifest
        .files
        .iter()
        .filter(|entry| {
            let local = eq.join(&entry.path);
            if !local.exists() {
                return true;
            }
            match fs::metadata(&local) {
                Ok(m) if m.len() != entry.size => true,
                Ok(_) => sha256_of(&local)
                    .map(|h| !h.eq_ignore_ascii_case(&entry.sha256))
                    .unwrap_or(true),
                Err(_) => true,
            }
        })
        .cloned()
        .collect();

    if pending.is_empty() {
        emit_patch(
            &app,
            PatchProgress {
                stage: "done".into(),
                current_file: String::new(),
                files_done: 0,
                files_total: 0,
                bytes_done: 0,
                bytes_total: 0,
            },
        );
        return Ok(0);
    }

    let total_bytes: u64 = pending.iter().map(|f| f.size).sum();
    let files_total = pending.len() as u32;
    let mut bytes_done: u64 = 0;

    for (idx, entry) in pending.iter().enumerate() {
        emit_patch(
            &app,
            PatchProgress {
                stage: "downloading".into(),
                current_file: entry.path.clone(),
                files_done: idx as u32,
                files_total,
                bytes_done,
                bytes_total: total_bytes,
            },
        );

        let url = format!(
            "{}{}",
            manifest.patch_url_base.trim_end_matches('/').to_string() + "/",
            entry.path
        );
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("download {}: {e}", entry.path))?;
        if !resp.status().is_success() {
            return Err(format!(
                "download {} returned {}",
                entry.path,
                resp.status()
            ));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("read {}: {e}", entry.path))?;

        // Verify sha256 before writing
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(&entry.sha256) {
            return Err(format!(
                "{}: sha256 mismatch (expected {}, got {})",
                entry.path, entry.sha256, got
            ));
        }

        // Atomic write: tmp file + rename
        let target = eq.join(&entry.path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        let tmp = target.with_extension("crushbone-tmp");
        fs::write(&tmp, &bytes).map_err(|e| format!("write tmp {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &target)
            .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), target.display()))?;

        bytes_done += entry.size;
        emit_patch(
            &app,
            PatchProgress {
                stage: "downloading".into(),
                current_file: entry.path.clone(),
                files_done: (idx + 1) as u32,
                files_total,
                bytes_done,
                bytes_total: total_bytes,
            },
        );
    }

    emit_patch(
        &app,
        PatchProgress {
            stage: "done".into(),
            current_file: String::new(),
            files_done: files_total,
            files_total,
            bytes_done,
            bytes_total: total_bytes,
        },
    );

    Ok(files_total)
}

// ============================================================
// First-run setup — Whisky check, prefix init, base client
// download + extract, eqhost.txt write. Together these turn
// the launcher into a one-click installer.
// ============================================================

#[derive(Debug, Serialize, Clone)]
pub struct SetupState {
    pub whisky_installed: bool,
    pub prefix_ready: bool,
    pub client_installed: bool,
    pub eqhost_set: bool,
    pub ready: bool,
    pub whisky_download_url: String,
    pub eq_dir: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct SetupProgress {
    pub stage: String,    // "downloading" | "extracting" | "initializing" | "done" | "error"
    pub step: String,     // "whisky" | "wine_prefix" | "client" | "eqhost"
    pub received: u64,
    pub total: u64,
    pub percent: u8,
    pub detail: String,
}

fn emit_setup(app: &AppHandle, step: &str, stage: &str, received: u64, total: u64, detail: &str) {
    let percent = if total > 0 {
        ((received * 100) / total).min(100) as u8
    } else if stage == "done" {
        100
    } else {
        0
    };
    let _ = app.emit(
        "setup-progress",
        SetupProgress {
            stage: stage.into(),
            step: step.into(),
            received,
            total,
            percent,
            detail: detail.into(),
        },
    );
}

#[tauri::command]
fn get_setup_state() -> Result<SetupState, String> {
    // Whisky/Wine are macOS/Linux-only. On Windows they're not applicable, so
    // we report them ready and gate overall readiness on client + eqhost only.
    #[cfg(target_os = "windows")]
    let (whisky_installed, prefix_ready, whisky_download_url) =
        (true, true, String::new());
    #[cfg(not(target_os = "windows"))]
    let (whisky_installed, prefix_ready, whisky_download_url) = (
        Path::new(WHISKY_APP).exists() && whisky_wine_bin()?.exists(),
        wine_prefix()?.join("system.reg").exists(),
        WHISKY_DOWNLOAD_PAGE.to_string(),
    );

    let eq = eq_dir()?;
    let client_installed = eq.join("eqgame.exe").exists();
    let eqhost_set = fs::read_to_string(eq.join("eqhost.txt"))
        .map(|s| s.contains(&format!("Host={}", active_host())))
        .unwrap_or(false);
    let ready = whisky_installed && prefix_ready && client_installed && eqhost_set;
    Ok(SetupState {
        whisky_installed,
        prefix_ready,
        client_installed,
        eqhost_set,
        ready,
        whisky_download_url,
        eq_dir: eq.display().to_string(),
    })
}

/// Windows has no Wine prefix to initialize — eqgame.exe runs natively.
#[cfg(target_os = "windows")]
#[tauri::command]
async fn setup_wine_prefix(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// Initialize a fresh Wine prefix at our managed location.
/// Runs `wine64 wineboot --init` once; idempotent on re-runs.
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn setup_wine_prefix(app: AppHandle) -> Result<(), String> {
    let wine = whisky_wine_bin()?;
    if !wine.exists() {
        return Err(format!(
            "Whisky's wine64 not found. Install Whisky from {} first.",
            WHISKY_DOWNLOAD_PAGE
        ));
    }
    let prefix = wine_prefix()?;
    fs::create_dir_all(&prefix).map_err(|e| format!("create prefix dir: {e}"))?;

    emit_setup(
        &app,
        "wine_prefix",
        "initializing",
        0,
        0,
        "Booting Wine prefix (~10s)…",
    );

    let status = Command::new(&wine)
        .env("WINEPREFIX", &prefix)
        .env("WINEDEBUG", "-all")
        .arg("wineboot")
        .arg("--init")
        .status()
        .await
        .map_err(|e| format!("spawn wineboot: {e}"))?;

    if !status.success() {
        emit_setup(
            &app,
            "wine_prefix",
            "error",
            0,
            0,
            &format!("wineboot exited {:?}", status.code()),
        );
        return Err(format!("wineboot failed (exit {:?})", status.code()));
    }

    emit_setup(&app, "wine_prefix", "done", 1, 1, "Wine prefix ready");
    Ok(())
}

/// Write eqhost.txt pointing at the public The Last Camp login server.
/// Matches the standard EQ ini format: `[LoginServer]` section header
/// followed by the `Host=` line. CRLF line endings so Wine's notepad-style
/// readers handle it cleanly.
#[tauri::command]
fn setup_eqhost() -> Result<(), String> {
    let eq = eq_dir()?;
    fs::create_dir_all(&eq).map_err(|e| format!("create eq dir: {e}"))?;
    let path = eq.join("eqhost.txt");
    let payload = format!("[LoginServer]\r\nHost={}\r\n", active_host());
    fs::write(&path, payload).map_err(|e| format!("write eqhost: {e}"))?;
    Ok(())
}

/// Create a standalone "play" shortcut that launches the game directly, skipping
/// the launcher UI. It connects to whatever server is currently selected (the
/// host pinned in eqhost.txt). macOS: a .app in ~/Applications (drag to the Dock)
/// that runs the same Whisky/Metal launch command as Enter World via `exec`, so
/// macOS does not reap the game when the wrapper would otherwise exit.
#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn create_play_shortcut() -> Result<String, String> {
    let wine = whisky_wine_bin()?;
    let prefix = wine_prefix()?;
    let eq = eq_dir()?;
    let apps = dirs::home_dir()
        .ok_or_else(|| "could not resolve home directory".to_string())?
        .join("Applications");
    fs::create_dir_all(&apps).map_err(|e| format!("create ~/Applications: {e}"))?;
    let app_path = apps.join("The Last Camp.app");
    let macos = app_path.join("Contents/MacOS");
    let resources = app_path.join("Contents/Resources");
    fs::create_dir_all(&macos).map_err(|e| format!("create app bundle: {e}"))?;
    fs::create_dir_all(&resources).map_err(|e| format!("create app bundle: {e}"))?;

    // {:?} on a Path emits a double-quoted, escaped string — safe for paths with
    // spaces. Same env as launch_eq; exec replaces the wrapper with the game.
    let script = format!(
        "#!/bin/bash\n\
         export WINEPREFIX={prefix:?}\n\
         export WINEDEBUG=-all\n\
         export WINEDLLOVERRIDES=\"d3d9,d3d10core,d3d11,d3d12,dxgi=n,b\"\n\
         export WINEESYNC=1\n\
         export WINEFSYNC=1\n\
         export DXVK_ASYNC=1\n\
         cd {eq:?} || exit 1\n\
         exec {wine:?} eqgame.exe patchme\n",
    );
    let exec_path = macos.join("run");
    fs::write(&exec_path, script).map_err(|e| format!("write launch script: {e}"))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&exec_path)
            .map_err(|e| format!("stat script: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&exec_path, perms).map_err(|e| format!("chmod script: {e}"))?;
    }

    let plist = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
        <plist version=\"1.0\"><dict>\
        <key>CFBundleName</key><string>The Last Camp</string>\
        <key>CFBundleDisplayName</key><string>The Last Camp</string>\
        <key>CFBundleExecutable</key><string>run</string>\
        <key>CFBundleIdentifier</key><string>live.thelastcamp.play</string>\
        <key>CFBundlePackageType</key><string>APPL</string>\
        <key>CFBundleIconFile</key><string>icon</string>\
        <key>CFBundleShortVersionString</key><string>1.0</string>\
        </dict></plist>";
    fs::write(app_path.join("Contents/Info.plist"), plist)
        .map_err(|e| format!("write Info.plist: {e}"))?;

    // Reuse the launcher's own campfire icon (Contents/Resources/icon.icns).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(icns) = exe
            .parent()
            .and_then(|p| p.parent())
            .map(|contents| contents.join("Resources/icon.icns"))
        {
            if icns.exists() {
                let _ = fs::copy(&icns, resources.join("icon.icns"));
            }
        }
    }

    // Nudge LaunchServices to register the new bundle + icon.
    let _ = std::process::Command::new("/usr/bin/touch")
        .arg(&app_path)
        .status();

    Ok(app_path.display().to_string())
}

/// Windows: a .bat on the Desktop that launches eqgame directly.
#[cfg(target_os = "windows")]
#[tauri::command]
fn create_play_shortcut() -> Result<String, String> {
    let eq = eq_dir()?;
    let desktop = dirs::desktop_dir().ok_or_else(|| "could not resolve Desktop".to_string())?;
    let bat = desktop.join("The Last Camp.bat");
    let script = format!(
        "@echo off\r\ncd /d \"{}\"\r\nstart \"\" eqgame.exe patchme\r\n",
        eq.display()
    );
    fs::write(&bat, &script).map_err(|e| format!("write shortcut: {e}"))?;
    Ok(bat.display().to_string())
}

/// No-op on Windows (no Whisky needed).
#[cfg(target_os = "windows")]
#[tauri::command]
fn open_whisky_download() -> Result<(), String> {
    Ok(())
}

/// Open the Whisky download page in the user's browser. The first-run
/// wizard calls this when Whisky.app is missing; user installs it via
/// drag-to-Applications, then returns to the wizard.
#[cfg(not(target_os = "windows"))]
#[tauri::command]
fn open_whisky_download() -> Result<(), String> {
    Command::new("open")
        .arg(WHISKY_DOWNLOAD_PAGE)
        .spawn()
        .map_err(|e| format!("open browser: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_server_status,
            get_mod_state,
            set_mod_state,
            launch_eq,
            get_patch_notes,
            get_preflight,
            verify_install,
            get_last_character,
            get_mod_packs,
            download_mod_pack,
            check_for_updates,
            apply_updates,
            get_setup_state,
            setup_wine_prefix,
            setup_eqhost,
            open_whisky_download,
            get_platform,
            get_eq_dir,
            set_eq_dir,
            get_server,
            set_server,
            create_play_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
