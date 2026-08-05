#!/bin/bash
# Uninstaller for the .deb install of Work Review.
# Removes the apt package and the GNOME extension. User data is PRESERVED
# by default — pass --purge to wipe it. apt Depends (tesseract, screenshot
# tools, webkit, ...) are left alone (use `sudo apt autoremove` yourself).
#
# Usage:
#   bash scripts/deb/uninstall.sh               # keep user data (default)
#   bash scripts/deb/uninstall.sh --purge       # also wipe DB + screenshots + config
#   bash scripts/deb/uninstall.sh --dry-run     # show what would happen
#
set -euo pipefail

EXT_DIR="$HOME/.local/share/gnome-shell/extensions/focused-window-dbus@flexagoon.com"
DATA_DIR="$HOME/.local/share/work-review"

PURGE=0
DRY=0
for arg in "$@"; do
    case "$arg" in
        --purge)   PURGE=1 ;;
        --dry-run) DRY=1 ;;
        -h|--help) sed -n '2,11p' "$0"; exit 0 ;;
        *) echo "unknown flag: $arg (see --help)" >&2; exit 2 ;;
    esac
done

log()  { printf '\033[1;34m[uninstall]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[uninstall]\033[0m %s\n' "$*" >&2; }

do_rm() {
    local target="$1"
    if [ ! -e "$target" ] && [ ! -L "$target" ]; then
        log "  (skip, not present) $target"; return 0
    fi
    if [ "$DRY" -eq 1 ]; then log "  would remove: $target"
    else log "  rm $target"; rm -rf -- "$target"
    fi
}

# ---- 0. stop running processes ------------------------------------------
log "0/4 stop running Work_Review processes"
if pgrep -x "Work_Review" >/dev/null; then
    if [ "$DRY" -eq 1 ]; then log "  would: pkill -x Work_Review"
    else
        pkill -x "Work_Review" || true
        sleep 1
        pkill -9 -x "Work_Review" 2>/dev/null || true
        log "  stopped"
    fi
else
    log "  not running"
fi

# ---- 1. apt package ------------------------------------------------------
log "1/4 apt package work-review"
if dpkg -s work-review >/dev/null 2>&1; then
    if [ "$DRY" -eq 1 ]; then
        log "  would: sudo apt-get remove -y work-review"
    else
        log "  sudo apt remove (may prompt for password)"
        sudo apt-get remove -y work-review
    fi
else
    log "  not installed"
fi

# ---- 1b. no-flash gnome-screenshot shim ---------------------------------
SHIM="$HOME/.local/bin/gnome-screenshot"
if [ -f "$SHIM" ] && grep -q "work-review:gnome-screenshot-noflash-shim" "$SHIM" 2>/dev/null; then
    log "1b/4 no-flash gnome-screenshot shim"
    do_rm "$SHIM"
fi

# ---- 1c. silent screen-capture.oga overrides ----------------------------
# Only remove files whose sha256 matches what reinstall.sh wrote — never
# touch files the user installed themselves.
SILENT_OGA_SHA256="a2765ad17bccf6dfd4226cbf84820e9d3df777b05cd3c2944fc69839c70f9bc1"
for theme in Yaru freedesktop; do
    OGA="$HOME/.local/share/sounds/$theme/stereo/screen-capture.oga"
    [ -f "$OGA" ] || continue
    if [ "$(sha256sum "$OGA" 2>/dev/null | cut -d' ' -f1)" = "$SILENT_OGA_SHA256" ]; then
        log "1c/4 silent screen-capture.oga ($theme)"
        do_rm "$OGA"
    else
        log "1c/4 keep $OGA (sha256 ≠ ours, looks user-installed)"
    fi
done

# ---- 2. GNOME extension --------------------------------------------------
log "2/4 GNOME extension focused-window-dbus"
if [ -d "$EXT_DIR" ]; then
    if [ "$DRY" -eq 1 ]; then
        log "  would disable + remove $EXT_DIR"
    else
        gnome-extensions disable focused-window-dbus@flexagoon.com 2>/dev/null || true
        rm -rf "$EXT_DIR"
        log "  removed $EXT_DIR (effective after next Shell reload / relogin)"
    fi
else
    log "  not installed"
fi

# ---- 3. user data --------------------------------------------------------
log "3/4 user data ($DATA_DIR)"
if [ "$PURGE" -eq 1 ]; then
    do_rm "$DATA_DIR"
else
    if [ -d "$DATA_DIR" ]; then
        SIZE=$(du -sh "$DATA_DIR" 2>/dev/null | cut -f1)
        log "  KEPT: $DATA_DIR ($SIZE). Pass --purge to remove."
    else
        log "  not present"
    fi
fi

log "4/4 done"
cat <<NOTES

Not touched (on purpose):
  - apt dependencies pulled in by the .deb (tesseract-ocr*, gnome-screenshot,
    grim, libwebkit2gtk-4.1-0, xdotool, xprintidle, ...). They're generic
    tools other apps may use. To see them:
        apt-mark showauto | grep -E '^(tesseract|xdotool|xprintidle|grim|libwebkit)'
    And to prune only the ones no other package needs:
        sudo apt-get autoremove
NOTES
if [ "$PURGE" -eq 0 ] && [ -d "$DATA_DIR" ]; then
    echo "  - User data at $DATA_DIR (rerun with --purge to delete)."
fi
