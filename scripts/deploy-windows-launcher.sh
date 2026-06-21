#!/usr/bin/env bash
# Deploy launcher builds to patch.thelastcamp.net — Windows and/or macOS.
#
# tauri-plugin-updater serves ONE version across ALL platforms in a single
# manifest.json. So a release at version X must ship every platform's artifact
# at X: if you bump the manifest to 0.4.0 but leave the darwin entry pointing at
# the old 0.3.0 .app.tar.gz, Mac users on 0.3.0 are told "update to 0.4.0",
# download the still-0.3.0 artifact, and get re-prompted forever (update loop).
# This script ships both platforms at parity and REFUSES to leave a platform
# stale across a version bump (override with --allow-stale-platforms).
#
# It takes the artifacts and:
#   1. uploads each provided platform's installer + self-update payload to the
#      patcher host (Windows: versioned -setup.exe + stable .exe alias; macOS:
#      stable Crushbone-Launcher.app.tar.gz + Crushbone-Launcher.dmg)
#   2. backs up every remote file it overwrites (timestamped .bak)
#   3. merges the provided platform entries into the served launcher
#      manifest.json, sets .version/.pub_date, and preserves untouched platforms
#
# Usage:
#   scripts/deploy-windows-launcher.sh --version 0.4.0 \
#       --win ~/Downloads/crushbone-launcher-windows \
#       --mac src-tauri/target/release/bundle
#
#   # Windows-only (will ERROR if it would strand the existing macOS entry):
#   scripts/deploy-windows-launcher.sh --version 0.4.0 --win ~/Downloads/win
#
#   # Legacy positional form still works: <win_artifacts_dir> <version>
#   scripts/deploy-windows-launcher.sh ~/Downloads/win 0.4.0
#
# Flags:
#   --version V               release version (required)
#   --win DIR                 dir containing the *-setup.exe + *-setup.exe.sig
#   --mac DIR                 dir containing *.app.tar.gz(.sig) + *.dmg (the
#                             `pnpm tauri build` bundle/ dir)
#   --allow-stale-platforms   permit a version bump that leaves a platform's
#                             artifact untouched (intentional update loop — rare)
#   --no-backup               skip timestamped backups of overwritten files
#   --dry-run                 print the plan + merged manifest, touch nothing
#
# Requires: ssh access to the patcher host, jq, curl.
set -euo pipefail

# Deploy target — set in your environment, never hardcoded (keeps infra out of the public repo).
#   export TLC_DEPLOY_HOST="root@your-server"
HOST="${TLC_DEPLOY_HOST:?Set TLC_DEPLOY_HOST=root@your-server before deploying}"
REMOTE_DIR="/srv/crushbone/patcher/launcher"
BASE_URL="https://patch.thelastcamp.net/patch/launcher"
STABLE_EXE="Crushbone-Launcher-Setup.exe"
MAC_TAR_NAME="Crushbone-Launcher.app.tar.gz"
MAC_DMG_NAME="Crushbone-Launcher.dmg"

VERSION=""
WIN_DIR=""
MAC_DIR=""
ALLOW_STALE=0
NO_BACKUP=0
DRY_RUN=0

die() { echo "error: $*" >&2; exit 1; }

# Legacy positional form: `<win_dir> <version>` (no flags).
if [ "$#" -ge 1 ] && [ "${1#-}" = "$1" ]; then
  WIN_DIR="${1:?}"
  VERSION="${2:?usage: deploy-windows-launcher.sh <win_artifacts_dir> <version>}"
else
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --version) VERSION="${2:?}"; shift 2 ;;
      --win)     WIN_DIR="${2:?}"; shift 2 ;;
      --mac)     MAC_DIR="${2:?}"; shift 2 ;;
      --allow-stale-platforms) ALLOW_STALE=1; shift ;;
      --no-backup) NO_BACKUP=1; shift ;;
      --dry-run) DRY_RUN=1; shift ;;
      -h|--help) sed -n '2,41p' "$0"; exit 0 ;;
      *) die "unknown arg: $1 (try --help)" ;;
    esac
  done
fi

command -v jq  >/dev/null || die "jq not found"
command -v curl >/dev/null || die "curl not found"
[ -n "$VERSION" ] || die "--version is required"
[ -n "$WIN_DIR$MAC_DIR" ] || die "provide --win and/or --mac"

# ---- locate artifacts -------------------------------------------------------
# Tauri v2 NSIS: the -setup.exe is BOTH the installer and the updater payload
# (the updater re-runs the installer). It is signed as -setup.exe.sig. There is
# no .nsis.zip (that was Tauri v1).
win_exe="" win_sig=""
if [ -n "$WIN_DIR" ]; then
  [ -d "$WIN_DIR" ] || die "--win dir not found: $WIN_DIR"
  win_exe="$(find "$WIN_DIR" -name '*-setup.exe'     | head -1)"
  win_sig="$(find "$WIN_DIR" -name '*-setup.exe.sig' | head -1)"
  [ -f "$win_exe" ] || die "no *-setup.exe in $WIN_DIR"
  [ -f "$win_sig" ] || die "no *-setup.exe.sig in $WIN_DIR"
fi

mac_tar="" mac_sig="" mac_dmg=""
if [ -n "$MAC_DIR" ]; then
  [ -d "$MAC_DIR" ] || die "--mac dir not found: $MAC_DIR"
  mac_tar="$(find "$MAC_DIR" -name '*.app.tar.gz'     | head -1)"
  mac_sig="$(find "$MAC_DIR" -name '*.app.tar.gz.sig' | head -1)"
  mac_dmg="$(find "$MAC_DIR" -name '*.dmg'            | head -1)"
  [ -f "$mac_tar" ] || die "no *.app.tar.gz in $MAC_DIR (did the updater target build?)"
  [ -f "$mac_sig" ] || die "no *.app.tar.gz.sig in $MAC_DIR"
  [ -f "$mac_dmg" ] || die "no *.dmg in $MAC_DIR"
fi

# ---- stale-platform guard ---------------------------------------------------
cur="$(curl -fsSL "$BASE_URL/manifest.json")"
cur_ver="$(printf '%s' "$cur" | jq -r '.version // empty')"

provided=$'\n'
[ -n "$WIN_DIR" ] && provided+=$'windows-x86_64\n'
[ -n "$MAC_DIR" ] && provided+=$'darwin-aarch64\n'

stale=""
while IFS= read -r p; do
  [ -z "$p" ] && continue
  printf '%s' "$provided" | grep -qxF "$p" || stale+="$p "
done < <(printf '%s' "$cur" | jq -r '.platforms // {} | keys[]')

if [ -n "$stale" ] && [ "$VERSION" != "$cur_ver" ] && [ "$ALLOW_STALE" -ne 1 ]; then
  die "refusing to bump $cur_ver -> $VERSION while leaving platform(s) stale: ${stale}
      tauri serves one version for all platforms, so these would point at old
      artifacts and loop on update. Rebuild + pass --mac/--win for them, or
      re-run with --allow-stale-platforms if you really mean it."
fi

# ---- upload -----------------------------------------------------------------
pub_date="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
ts="$(date -u +%Y%m%dT%H%M%SZ)"
sanitize() { basename "$1" | tr ' ' '-'; }

echo "Deploying launcher v$VERSION (was $cur_ver)"
[ -n "$WIN_DIR" ] && echo "  windows: $(basename "$win_exe")"
[ -n "$MAC_DIR" ] && echo "  macos:   $(basename "$mac_dmg")"

# Names we will write remotely (no spaces) — used for backup + manifest URLs.
win_exe_name="" ; [ -n "$WIN_DIR" ] && win_exe_name="$(sanitize "$win_exe")"

# Signature values + manifest URLs (read locally; needed for dry-run too).
win_sig_val="" win_url=""
if [ -n "$WIN_DIR" ]; then
  win_sig_val="$(cat "$win_sig")"
  win_url="$BASE_URL/$win_exe_name"
fi
mac_sig_val="" mac_url=""
if [ -n "$MAC_DIR" ]; then
  mac_sig_val="$(cat "$mac_sig")"
  mac_url="$BASE_URL/$MAC_TAR_NAME"
fi

if [ "$DRY_RUN" -ne 1 ]; then
  # Back up everything we are about to overwrite.
  if [ "$NO_BACKUP" -ne 1 ]; then
    targets="manifest.json"
    [ -n "$WIN_DIR" ] && targets+=" $win_exe_name $STABLE_EXE"
    [ -n "$MAC_DIR" ] && targets+=" $MAC_TAR_NAME $MAC_DMG_NAME"
    ssh "$HOST" "cd '$REMOTE_DIR' && for f in $targets; do [ -f \"\$f\" ] && cp -p \"\$f\" \"\$f.bak-$ts\" && echo \"  backed up \$f\"; done || true"
  fi

  if [ -n "$WIN_DIR" ]; then
    scp -q "$win_exe" "$HOST:$REMOTE_DIR/$win_exe_name"
    ssh "$HOST" "cp '$REMOTE_DIR/$win_exe_name' '$REMOTE_DIR/$STABLE_EXE'"
  fi
  if [ -n "$MAC_DIR" ]; then
    scp -q "$mac_tar" "$HOST:$REMOTE_DIR/$MAC_TAR_NAME"
    scp -q "$mac_dmg" "$HOST:$REMOTE_DIR/$MAC_DMG_NAME"
  fi
fi

# ---- merge manifest ---------------------------------------------------------
new="$(printf '%s' "$cur" | jq --arg ver "$VERSION" --arg pub "$pub_date" \
  '.version=$ver | .pub_date=$pub')"
if [ -n "$WIN_DIR" ]; then
  new="$(printf '%s' "$new" | jq --arg sig "$win_sig_val" --arg url "$win_url" \
    '.platforms["windows-x86_64"]={signature:$sig, url:$url}')"
fi
if [ -n "$MAC_DIR" ]; then
  new="$(printf '%s' "$new" | jq --arg sig "$mac_sig_val" --arg url "$mac_url" \
    '.platforms["darwin-aarch64"]={signature:$sig, url:$url}')"
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo
  echo "[dry-run] no files written. Merged manifest would be:"
  printf '%s\n' "$new" | jq .
  exit 0
fi

tmp="$(mktemp)"; printf '%s\n' "$new" > "$tmp"
scp -q "$tmp" "$HOST:$REMOTE_DIR/manifest.json"
rm -f "$tmp"

echo
echo "Live:"
[ -n "$WIN_DIR" ] && echo "  Windows installer (share this): $BASE_URL/$STABLE_EXE"
[ -n "$MAC_DIR" ] && echo "  macOS installer (share this):   $BASE_URL/$MAC_DMG_NAME"
echo "  Manifest now v$VERSION with platforms: $(printf '%s' "$new" | jq -rc '.platforms | keys')"
echo
echo "Verify:"
echo "  curl -s $BASE_URL/manifest.json | jq '.version, (.platforms|keys)'"
[ -n "$WIN_DIR" ] && echo "  curl -sI $BASE_URL/$STABLE_EXE | head -1"
