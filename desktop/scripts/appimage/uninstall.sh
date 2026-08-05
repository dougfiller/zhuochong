#!/bin/bash
# Uninstaller for Work Review 1.0.46.
# Removes everything install.sh set up. User data (~/.local/share/work-review/)
# is PRESERVED by default — pass --purge to wipe it. apt packages are never
# removed (they're generic tools — remove with apt manually if you want).
#
# Usage:
#   bash scripts/appimage/uninstall.sh            # keep user data (default)
#   bash scripts/appimage/uninstall.sh --purge    # also delete DB, screenshots, config
#   bash scripts/appimage/uninstall.sh --dry-run  # show what would be removed
#
set -euo pipefail

APPS_DIR="$HOME/Applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
DESKTOP_DIR="$HOME/.local/share/applications"
BIN_DIR="$HOME/bin"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/focused-window-dbus@flexagoon.com"
DATA_DIR="$HOME/.local/share/work-review"

PURGE=0
DRY=0
for arg in "$@"; do
    case "$arg" in
        --purge)   PURGE=1 ;;
        --dry-run) DRY=1 ;;
        -h|--help)
            sed -n '2,11p' "$0"; exit 0 ;;
        *) echo "unknown flag: $arg (see --help)" >&2; exit 2 ;;
    esac
done

log()  { printf '\033[1;34m[uninstall]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[uninstall]\033[0m %s\n' "$*" >&2; }

# Show action; actually do it unless --dry-run.
do_rm() {
    local target="$1"
    if [ ! -e "$target" ] && [ ! -L "$target" ]; then
        log "  (skip, not present) $target"
        return 0
    fi
    if [ "$DRY" -eq 1 ]; then
        log "  would remove: $target"
    else
        log "  rm $target"
        rm -rf -- "$target"
    fi
}

# ---- 0. stop the app if running ------------------------------------------
log "0/6 stop running Work_Review processes"
if pgrep -f "Work_Review --no-sandbox" >/dev/null; then
    if [ "$DRY" -eq 1 ]; then
        log "  would: pkill -f 'Work_Review --no-sandbox'"
    else
        pkill -f "Work_Review --no-sandbox" || true
        sleep 1
        pkill -9 -f "Work_Review --no-sandbox" 2>/dev/null || true
        log "  stopped"
    fi
else
    log "  not running"
fi

# ---- 1. desktop entry -----------------------------------------------------
log "1/6 .desktop entry"
do_rm "$DESKTOP_DIR/work-review.desktop"
[ "$DRY" -eq 1 ] || update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true

# ---- 2. icon --------------------------------------------------------------
log "2/6 icon"
do_rm "$ICON_DIR/Work_Review.png"
[ "$DRY" -eq 1 ] || gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

# ---- 3. shims + launcher --------------------------------------------------
log "3/6 shims + launcher in $BIN_DIR"
for f in work-review gdbus gnome-screenshot grim spectacle tesseract; do
    do_rm "$BIN_DIR/$f"
done
# If ~/bin is now empty, offer to remove it (non-destructive — we only rmdir,
# which fails silently if the dir still has other files).
if [ -d "$BIN_DIR" ] && [ "$DRY" -eq 0 ]; then
    rmdir "$BIN_DIR" 2>/dev/null && log "  removed empty $BIN_DIR" || true
fi

# ---- 4. AppImage ----------------------------------------------------------
log "4/6 AppImage(s) in $APPS_DIR"
shopt -s nullglob
for ai in "$APPS_DIR"/Work_Review_*.AppImage; do
    do_rm "$ai"
done
shopt -u nullglob
# Only rmdir if empty — don't nuke other AppImages the user put there.
if [ -d "$APPS_DIR" ] && [ "$DRY" -eq 0 ]; then
    rmdir "$APPS_DIR" 2>/dev/null && log "  removed empty $APPS_DIR" || true
fi

# ---- 5. GNOME extension ---------------------------------------------------
log "5/6 GNOME extension focused-window-dbus"
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

# ---- 6. user data ---------------------------------------------------------
log "6/6 user data ($DATA_DIR)"
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

echo
log "done."
cat <<NOTES

Not touched (on purpose):
  - apt packages (libfuse2, gnome-screenshot, grim, kde-spectacle,
    tesseract-ocr*, desktop-file-utils, git). Remove manually if desired:
      sudo apt-get remove <package>
NOTES
if [ "$PURGE" -eq 0 ] && [ -d "$DATA_DIR" ]; then
    echo "  - User data at $DATA_DIR (rerun with --purge to delete)."
fi
