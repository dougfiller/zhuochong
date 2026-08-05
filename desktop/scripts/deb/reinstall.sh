#!/bin/bash
# Full-reset installer: remove every trace of a previous Work Review install
# (AppImage-based, deb-based, or both), then cleanly reinstall via the official
# .deb. User data (~/.local/share/work-review/) is preserved by default.
#
# Usage:
#   bash scripts/deb/reinstall.sh                 # full wipe + reinstall, user data kept
#   bash scripts/deb/reinstall.sh --dry-run       # show every action, change nothing
#   bash scripts/deb/reinstall.sh --purge-data    # also wipe DB/screenshots/config
#   WR_DEB=/path/to/foo.deb bash .../reinstall.sh     # use a specific local .deb
#   WR_DEB_URL=https://.../foo.deb bash .../reinstall.sh   # use a specific URL
#
# Local-deb lookup: only scans $PWD for Work_Review_*.deb (any version).
# If nothing is found and no env override is given, the latest release is
# resolved from the GitHub API.
#
set -euo pipefail

EXPLICIT_URL="${WR_DEB_URL:-}"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/focused-window-dbus@flexagoon.com"
DATA_DIR="$HOME/.local/share/work-review"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DRY=0
PURGE_DATA=0
for arg in "$@"; do
    case "$arg" in
        --dry-run)     DRY=1 ;;
        --purge-data)  PURGE_DATA=1 ;;
        -h|--help)     sed -n '2,13p' "$0"; exit 0 ;;
        *) echo "unknown flag: $arg (see --help)" >&2; exit 2 ;;
    esac
done

log()   { printf '\033[1;34m[reinstall]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[reinstall]\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31m[reinstall]\033[0m %s\n' "$*" >&2; exit 1; }

run() {
    if [ "$DRY" -eq 1 ]; then
        printf '\033[2m[dry] %s\033[0m\n' "$*"
    else
        eval "$@"
    fi
}

do_rm() {
    local t="$1"
    if [ ! -e "$t" ] && [ ! -L "$t" ]; then return 0; fi
    if [ "$DRY" -eq 1 ]; then printf '\033[2m[dry] rm -rf %s\033[0m\n' "$t"
    else rm -rf -- "$t"; fi
}

# =========================================================================
# PHASE 1 — detect what's installed
# =========================================================================
log "phase 1/5: detect existing install state"

HAS_DEB=0
dpkg -s work-review >/dev/null 2>&1 && HAS_DEB=1

APPIMAGE_HITS=()
shopt -s nullglob
for ai in "$HOME/Applications"/Work_Review_*.AppImage; do
    APPIMAGE_HITS+=("$ai")
done
shopt -u nullglob

HAS_USER_DESKTOP=0
[ -f "$HOME/.local/share/applications/work-review.desktop" ] && HAS_USER_DESKTOP=1

HAS_SHIMS=0
for s in gdbus gnome-screenshot grim spectacle tesseract work-review; do
    if [ -f "$HOME/bin/$s" ] && grep -q "AppImage.*LD_LIBRARY_PATH\|Launcher for Work Review" "$HOME/bin/$s" 2>/dev/null; then
        HAS_SHIMS=1; break
    fi
done

HAS_EXT=0
[ -d "$EXT_DIR" ] && HAS_EXT=1

HAS_RUNNING=0
pgrep -f "Work_Review" >/dev/null 2>&1 && HAS_RUNNING=1

log "  .deb installed        : $([ $HAS_DEB -eq 1 ] && echo yes || echo no)"
log "  AppImage files        : ${#APPIMAGE_HITS[@]} found"
for ai in "${APPIMAGE_HITS[@]}"; do log "      - $ai"; done
log "  user .desktop entry   : $([ $HAS_USER_DESKTOP -eq 1 ] && echo yes || echo no)"
log "  AppImage shims in ~/bin: $([ $HAS_SHIMS -eq 1 ] && echo yes || echo no)"
log "  GNOME extension       : $([ $HAS_EXT -eq 1 ] && echo yes || echo no)"
log "  currently running     : $([ $HAS_RUNNING -eq 1 ] && echo yes || echo no)"
log "  user data (kept)      : $([ -d "$DATA_DIR" ] && du -sh "$DATA_DIR" 2>/dev/null | cut -f1 || echo none)"

# =========================================================================
# PHASE 2 — stop and fully remove old install(s)
# =========================================================================
log "phase 2/5: remove old install(s) [user data preserved]"

if [ "$HAS_RUNNING" -eq 1 ]; then
    log "  stopping running Work_Review processes"
    run "pkill -f Work_Review || true"
    run "sleep 1"
    run "pkill -9 -f Work_Review 2>/dev/null || true"
fi

# 2a. apt .deb
if [ "$HAS_DEB" -eq 1 ]; then
    log "  sudo apt remove work-review"
    run "sudo apt-get remove -y work-review"
fi

# 2b. AppImage files
for ai in "${APPIMAGE_HITS[@]}"; do
    log "  removing AppImage: $ai"
    do_rm "$ai"
done

# 2c. user-level desktop entry, icon, shims, launcher (the AppImage scheme)
log "  removing user-scope AppImage-scheme artefacts"
do_rm "$HOME/.local/share/applications/work-review.desktop"
do_rm "$HOME/.local/share/icons/hicolor/256x256/apps/Work_Review.png"
for f in work-review gdbus gnome-screenshot grim spectacle tesseract; do
    # Only nuke our shims, not random user scripts that happen to share names
    if [ -f "$HOME/bin/$f" ] && grep -qE "AppImage|Launcher for Work Review|bundled LD_LIBRARY_PATH" "$HOME/bin/$f" 2>/dev/null; then
        do_rm "$HOME/bin/$f"
    fi
done
# Try to remove ~/Applications and ~/bin if they're now empty; ignore failures.
[ "$DRY" -eq 0 ] && rmdir "$HOME/Applications" 2>/dev/null || true
[ "$DRY" -eq 0 ] && rmdir "$HOME/bin" 2>/dev/null || true

# 2d. Optional user-data purge
if [ "$PURGE_DATA" -eq 1 ]; then
    log "  --purge-data: removing $DATA_DIR"
    do_rm "$DATA_DIR"
else
    log "  preserving user data at $DATA_DIR"
fi

# 2e. Refresh caches so stale .desktop / icons disappear from Activities
if [ "$DRY" -eq 0 ]; then
    update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
    gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
fi

# =========================================================================
# PHASE 3 — install the .deb (apt pulls all deps)
# =========================================================================
log "phase 3/5: install work-review .deb"

DEB=""
if [ -n "${WR_DEB:-}" ] && [ -f "$WR_DEB" ]; then
    DEB="$WR_DEB"
else
    # Only scan the current working directory; pick any version match.
    shopt -s nullglob
    for candidate in "$PWD"/Work_Review_*.deb; do
        DEB="$candidate"; break
    done
    shopt -u nullglob
fi

if [ -z "$DEB" ]; then
    # Resolve URL: explicit override > GitHub latest release.
    if [ -n "$EXPLICIT_URL" ]; then
        DEB_URL="$EXPLICIT_URL"
    else
        log "  resolving latest release from GitHub API"
        if [ "$DRY" -eq 0 ]; then
            DEB_URL=$(curl -fsSL https://api.github.com/repos/wm94i/Work-Review/releases/latest \
                | grep -oE '"browser_download_url":[[:space:]]*"[^"]*_amd64\.deb"' \
                | head -1 \
                | sed -E 's/.*"([^"]+)"$/\1/')
            [ -n "$DEB_URL" ] || die "could not find an *_amd64.deb asset on the latest GitHub release"
        else
            DEB_URL="<resolved at run time from github releases/latest>"
        fi
    fi
    log "  downloading $DEB_URL"
    if [ "$DRY" -eq 0 ]; then
        DEB=$(mktemp /tmp/work-review.XXXXXX.deb)
        trap 'rm -f "$DEB"' EXIT
        curl -fsSL --progress-bar -o "$DEB" "$DEB_URL" || die "download failed"
    else
        printf '\033[2m[dry] curl -o /tmp/work-review.deb %s\033[0m\n' "$DEB_URL"
        DEB="/tmp/work-review.deb"
    fi
else
    log "  using local: $DEB"
fi

log "  sudo apt install $DEB (auto-resolves dependencies)"
run "sudo apt-get update -qq"
run "sudo apt-get install -y \"$DEB\""

log "  ensuring Chinese OCR language packs"
OCR_MISSING=()
for p in tesseract-ocr-chi-sim tesseract-ocr-chi-tra; do
    dpkg -s "$p" >/dev/null 2>&1 || OCR_MISSING+=("$p")
done
if [ ${#OCR_MISSING[@]} -gt 0 ]; then
    run "sudo apt-get install -y ${OCR_MISSING[*]}"
fi

# =========================================================================
# PHASE 4 — neutralize gnome-screenshot 41 on GNOME 46 (no-flash + no-sound)
# =========================================================================
# Ubuntu 24.04 ships gnome-screenshot 41, which calls
#   org.gnome.Shell.Screenshot.Screenshot(... flash=TRUE ...)
# on every invocation. On GNOME 46, gnome-shell responds with two visible
# side effects:
#   (a) a fullscreen flash animation
#   (b) a "camera shutter" event sound (libcanberra → screen-capture event)
# Work Review screenshots every 10s, so the desktop blinks AND clicks every
# 10s. We fix both:
#   (a) drop a Python shim at ~/.local/bin/gnome-screenshot that acquires
#       the org.gnome.Screenshot bus name and calls the same DBus method
#       with flash=FALSE (Shell only trusts callers holding that name).
#   (b) drop a 50ms silent .oga at ~/.local/share/sounds/{Yaru,freedesktop}
#       /stereo/screen-capture.oga — XDG Sound Theme spec gives the
#       user-level file priority over the system one.
log "phase 4/5: neutralize gnome-screenshot (no-flash + no-sound)"
SHIM_DST="$HOME/.local/bin/gnome-screenshot"
if ! dpkg -s python3-dbus >/dev/null 2>&1; then
    run "sudo apt-get install -y python3-dbus"
fi
mkdir -p "$HOME/.local/bin"
if [ "$DRY" -eq 1 ]; then
    printf '\033[2m[dry] write %s (no-flash shim)\033[0m\n' "$SHIM_DST"
else
    cat >"$SHIM_DST" <<'SHIM'
#!/usr/bin/env python3
# work-review:gnome-screenshot-noflash-shim
# No-flash gnome-screenshot replacement for GNOME 46 Wayland.
# Acquires the `org.gnome.Screenshot` bus name (which gnome-shell trusts) so
# we can call org.gnome.Shell.Screenshot.Screenshot with flash=FALSE.
# Falls back to /usr/bin/gnome-screenshot for argument sets we don't handle.
import os, sys
import dbus, dbus.service
from dbus.mainloop.glib import DBusGMainLoop

REAL = "/usr/bin/gnome-screenshot"

def fallback():
    os.execv(REAL, [REAL] + sys.argv[1:])

def parse(argv):
    out = {"file": None, "window": False, "cursor": False, "area": False,
           "interactive": False, "clipboard": False, "delay": 0,
           "version": False, "help": False, "extra": False}
    it = iter(argv[1:])
    for a in it:
        if a in ("-f", "--file"):
            try: out["file"] = next(it)
            except StopIteration: return None
        elif a.startswith("--file="):
            out["file"] = a.split("=", 1)[1]
        elif a in ("-w", "--window"): out["window"] = True
        elif a in ("-p", "--include-pointer"): out["cursor"] = True
        elif a in ("-a", "--area"): out["area"] = True
        elif a in ("-i", "--interactive"): out["interactive"] = True
        elif a in ("-c", "--clipboard"): out["clipboard"] = True
        elif a == "--version": out["version"] = True
        elif a in ("-h", "--help", "--help-all"): out["help"] = True
        elif a in ("-d", "--delay"):
            try: out["delay"] = int(next(it))
            except (StopIteration, ValueError): return None
        elif a.startswith("--delay="):
            try: out["delay"] = int(a.split("=", 1)[1])
            except ValueError: return None
        elif a in ("-b", "--include-border", "-B", "--remove-border", "--include-icc-profile"): pass
        else: out["extra"] = True
    return out

def main():
    args = parse(sys.argv)
    if args is None: fallback()
    if args["version"]:
        print("gnome-screenshot 41.0 (work-review no-flash shim)"); return
    if args["help"]: fallback()
    if args["extra"] or args["area"] or args["interactive"] or args["clipboard"]: fallback()
    if not args["file"]: fallback()

    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    # Hold the BusName in a local; if it goes out of scope, GC releases the
    # well-known name and the Shell.Screenshot call gets AccessDenied.
    try:
        _name = dbus.service.BusName("org.gnome.Screenshot", bus, do_not_queue=True)
    except Exception:
        _name = None

    proxy = bus.get_object("org.gnome.Shell.Screenshot", "/org/gnome/Shell/Screenshot")
    iface = dbus.Interface(proxy, "org.gnome.Shell.Screenshot")
    try:
        if args["window"]:
            ok, _ = iface.ScreenshotWindow(False, args["cursor"], False, args["file"])
        else:
            ok, _ = iface.Screenshot(args["cursor"], False, args["file"])
    except Exception as e:
        sys.stderr.write(f"no-flash shim: {e}; falling back to {REAL}\n")
        fallback(); return
    if not ok:
        fallback(); return
    if not (os.path.exists(args["file"]) and os.path.getsize(args["file"]) > 0):
        fallback(); return

if __name__ == "__main__":
    main()
SHIM
    chmod +x "$SHIM_DST"
    log "  installed $SHIM_DST"
fi

# 4b. silent screen-capture.oga overrides
# 50ms mono Vorbis silence. sha256 below is the install-time fingerprint —
# uninstall.sh checks it so we never delete a sound the user installed.
SILENT_OGA_SHA256="a2765ad17bccf6dfd4226cbf84820e9d3df777b05cd3c2944fc69839c70f9bc1"
SILENT_OGA_B64='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ABhwAAAIMKEMFBqyIgCIAgAAnHPOOeecc84555xzzjnnnHPOOecYY4wxxhhjjDHGGGOMMcYYY4wxxhhjjDHGGGOMMcYEAOxEOADsRFgIhYasBADCAQAAhBSCklIppZQSOeeklFJKKaWUyEEIpZRSSimlRNJJKaWUUkoppXFQSimllFJKKaGUUkoppZRSSgmllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoppZRSSimllFJKKaWUUkoBACYPDgBQCTbOsJJ0VjgaXGjISgAgNwAAUIo5xiSUkEpIJYQQSuUYhM5JCSm1VkIKrYQKOmido5BSS62VlEpJmYQQQiihhFJaKSW1UjIIoYRQSgghpVJKCaFlUEIKJZSUUkkttFRKySCEUFoJqZXUWgollZRBKamEklIqrbWUSkqtg9JSKa211kpKIZWWUgelpJZSKaW1FkprrbVOUiktpNZSa62VVkopnaWUSkmttZZaaymlVkIprbTSWikltdZSay2V1FpLraXWUmutpdZKKSWlllprrbWWWioptZRCKaWVkkJqqaXWSiothNBSSaWVVlprKaWUSigllZRaKqm1llJopYXSSkklpZZKKiml1FIqoZQSUiqhldRSa6mllkoqLbXUUiuplJZKSqkUAAB04AAAEGBEpYXYacaVR+CIQoYJKAAAEAQAGIiQmUCgAAoMZADAAUKCFABQWGAoXeiCECJIF0EWD1w4ceOJG07o0AYAGIiQmQChGCIkZAPABEWFdACwuMAoXeiCECJIF0EWD1w4ceOJG07o0AIBAAAAAAACAB8AAAcGEBHRXIbGBkeHxwdIiAgAAAAAAAAAAAAAAIBPZ2dTAARPBAAAAAAAAOjzrPYCAAAAUyLQvAQBAQEBAAAAAA=='
log "  silent screen-capture.oga override"
if [ "$DRY" -eq 0 ]; then
    OGA_TMP=$(mktemp /tmp/wr-silent.XXXXXX.oga)
    printf '%s' "$SILENT_OGA_B64" | base64 -d > "$OGA_TMP"
    if [ "$(sha256sum "$OGA_TMP" | cut -d' ' -f1)" != "$SILENT_OGA_SHA256" ]; then
        rm -f "$OGA_TMP"
        die "embedded silent oga decoded with unexpected sha256 — refusing to install"
    fi
    for theme in Yaru freedesktop; do
        DST="$HOME/.local/share/sounds/$theme/stereo/screen-capture.oga"
        mkdir -p "$(dirname "$DST")"
        cp "$OGA_TMP" "$DST"
        log "    -> $DST"
    done
    rm -f "$OGA_TMP"
else
    printf '\033[2m[dry] write %s/.local/share/sounds/{Yaru,freedesktop}/stereo/screen-capture.oga (50ms silent vorbis)\033[0m\n' "$HOME"
fi

# =========================================================================
# PHASE 5 — install GNOME extension (required for Wayland window tracking)
# =========================================================================
log "phase 5/5: GNOME extension focused-window-dbus"
if [ -f "$EXT_DIR/extension.js" ]; then
    log "  already present at $EXT_DIR"
else
    if ! command -v git >/dev/null; then
        run "sudo apt-get install -y git"
    fi
    TMP_FWD=$(mktemp -d)
    trap 'rm -rf "$TMP_FWD"' EXIT
    if [ "$DRY" -eq 0 ]; then
        git clone --quiet https://github.com/flexagoon/focused-window-dbus.git "$TMP_FWD/fwd"
        (cd "$TMP_FWD/fwd" && git checkout --quiet 0368030 -- .)
        mkdir -p "$EXT_DIR"
        cp "$TMP_FWD/fwd/extension.js" "$TMP_FWD/fwd/metadata.json" "$TMP_FWD/fwd/LICENSE" "$EXT_DIR/"
    else
        printf '\033[2m[dry] git clone focused-window-dbus (commit 0368030) -> %s\033[0m\n' "$EXT_DIR"
    fi
    log "  installed to $EXT_DIR"
fi

# Detect whether Shell has already scanned the dir.
if [ "$DRY" -eq 0 ]; then
    SHELL_SEES=$(gnome-extensions list 2>/dev/null | grep -c "^focused-window-dbus@flexagoon.com$" || true)
    STATE=$(gnome-extensions info focused-window-dbus@flexagoon.com 2>/dev/null | grep -E "^\s*State:" | awk '{print $2}' || true)
else
    SHELL_SEES=0; STATE=""
fi

cat <<NOTES

=========================================================================
Done.

  .deb        : /usr/bin/Work_Review  (apt package 'work-review')
  .desktop    : /usr/share/applications/Work Review.desktop
  Extension   : $EXT_DIR
  User data   : $([ -d "$DATA_DIR" ] && echo "$DATA_DIR (kept)" || echo "(none)")

NOTES

if [ "$DRY" -eq 1 ]; then
    cat <<NOTES
(dry-run — no changes made.)
NOTES
    exit 0
fi

if [ "$SHELL_SEES" -eq 0 ]; then
    cat <<NOTES
Next steps (REQUIRED):
  1. Log out of GNOME and log back in (Wayland has no live Shell reload).
  2. gnome-extensions enable focused-window-dbus@flexagoon.com
  3. Press Super, search "Work Review", click to launch.
NOTES
elif [ "$STATE" != "ACTIVE" ]; then
    log "enabling extension now"
    gnome-extensions enable focused-window-dbus@flexagoon.com 2>&1 || true
    cat <<NOTES
Next step: press Super, search "Work Review", click to launch.
NOTES
else
    cat <<NOTES
Everything already active. Press Super, search "Work Review", click to launch.
NOTES
fi

cat <<NOTES

Verify after a minute of real use:
  sqlite3 \$HOME/.local/share/work-review/workreview.db 'SELECT COUNT(*) FROM activities;'

NOTES
