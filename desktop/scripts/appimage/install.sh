#!/bin/bash
# One-shot installer for Work Review 1.0.46 on Ubuntu 24.04 / GNOME 46 / Wayland.
# Idempotent: safe to re-run — all steps either verify existing state or overwrite.
# See ../../work-review-debugging.md for why each piece is needed.
#
# Usage:
#   bash scripts/appimage/install.sh               # auto-detect AppImage
#   WR_APPIMAGE=/path/to/X.AppImage bash .../install.sh
#
set -euo pipefail

APPS_DIR="$HOME/Applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
DESKTOP_DIR="$HOME/.local/share/applications"
BIN_DIR="$HOME/bin"
EXT_DIR="$HOME/.local/share/gnome-shell/extensions/focused-window-dbus@flexagoon.com"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

log()   { printf '\033[1;34m[install]\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33m[install]\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31m[install]\033[0m %s\n' "$*" >&2; exit 1; }

# ---- 1. apt packages ------------------------------------------------------
log "1/6 apt packages"
PKGS=(libfuse2 gnome-screenshot grim kde-spectacle
      tesseract-ocr tesseract-ocr-chi-sim tesseract-ocr-chi-tra
      desktop-file-utils git)
MISSING=()
for p in "${PKGS[@]}"; do
    # Ubuntu 24.04 renamed libfuse2 → libfuse2t64 (t64 ABI transition); accept either
    if ! dpkg -s "$p" >/dev/null 2>&1 && ! dpkg -s "${p}t64" >/dev/null 2>&1; then
        MISSING+=("$p")
    fi
done
if [ ${#MISSING[@]} -gt 0 ]; then
    log "  installing: ${MISSING[*]}"
    sudo apt-get update -qq
    sudo apt-get install -y "${MISSING[@]}"
else
    log "  all already installed"
fi

# ---- 2. GNOME extension ---------------------------------------------------
log "2/6 GNOME extension focused-window-dbus"
if [ -f "$EXT_DIR/extension.js" ]; then
    log "  already present at $EXT_DIR"
else
    TMP_FWD=$(mktemp -d); trap "rm -rf $TMP_FWD" EXIT
    git clone --quiet https://github.com/flexagoon/focused-window-dbus.git "$TMP_FWD/fwd"
    (cd "$TMP_FWD/fwd" && git checkout --quiet 0368030 -- .)
    mkdir -p "$EXT_DIR"
    cp "$TMP_FWD/fwd/extension.js" "$TMP_FWD/fwd/metadata.json" "$TMP_FWD/fwd/LICENSE" "$EXT_DIR/"
    log "  installed to $EXT_DIR (log out + back in to activate)"
fi

# ---- 3. locate and place AppImage ----------------------------------------
log "3/6 place AppImage in $APPS_DIR"
mkdir -p "$APPS_DIR"
SRC=""
if [ -n "${WR_APPIMAGE:-}" ] && [ -f "$WR_APPIMAGE" ]; then
    SRC="$WR_APPIMAGE"
else
    for candidate in \
        "$APPS_DIR"/Work_Review_*.AppImage \
        "$SCRIPT_DIR/../.."/Work_Review_*.AppImage; do
        [ -f "$candidate" ] && { SRC="$candidate"; break; }
    done
fi
[ -n "$SRC" ] || die "no Work_Review_*.AppImage found. Place one in $APPS_DIR or set WR_APPIMAGE=/path/to/it."

TARGET="$APPS_DIR/$(basename "$SRC")"
if [ "$(readlink -f "$SRC")" != "$(readlink -f "$TARGET")" ]; then
    log "  moving $SRC -> $TARGET"
    mv "$SRC" "$TARGET"
else
    log "  already at $TARGET"
fi
chmod +x "$TARGET"
APPIMAGE="$TARGET"

# ---- 4. extract icon ------------------------------------------------------
log "4/6 extract icon"
if [ -f "$ICON_DIR/Work_Review.png" ]; then
    log "  already present at $ICON_DIR/Work_Review.png"
else
    TMP_EXTRACT=$(mktemp -d)
    (cd "$TMP_EXTRACT" && "$APPIMAGE" --appimage-extract >/dev/null)
    mkdir -p "$ICON_DIR"
    cp "$TMP_EXTRACT/squashfs-root/Work_Review.png" "$ICON_DIR/Work_Review.png"
    rm -rf "$TMP_EXTRACT"
    gtk-update-icon-cache "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
    log "  installed to $ICON_DIR/Work_Review.png"
fi

# ---- 5. shims, launcher, .desktop ----------------------------------------
log "5/6 deploy shims, launcher, .desktop"
mkdir -p "$BIN_DIR" "$DESKTOP_DIR"
for cmd in gdbus gnome-screenshot grim spectacle tesseract; do
cat > "$BIN_DIR/$cmd" <<EOF
#!/bin/bash
# Shim: strip the AppImage's bundled LD_LIBRARY_PATH before exec-ing the real
# system binary, otherwise it links against stale glib and exits 127.
unset LD_LIBRARY_PATH GDK_PIXBUF_MODULE_FILE GIO_EXTRA_MODULES GSETTINGS_SCHEMA_DIR GIO_MODULE_DIR
exec /usr/bin/$cmd "\$@"
EOF
chmod +x "$BIN_DIR/$cmd"
done

cat > "$BIN_DIR/work-review" <<EOF
#!/bin/bash
# Launcher for Work Review AppImage on Ubuntu 24.04 / GNOME 46 / Wayland.
set -u
APPIMAGE="\${WR_APPIMAGE:-$APPIMAGE}"
LOGDIR="\$HOME/.local/share/work-review"
mkdir -p "\$LOGDIR"
export GIO_MODULE_DIR=/nonexistent
export PATH="\$HOME/bin:/usr/bin:/usr/local/bin:\$PATH"
export RUST_LOG="\${RUST_LOG:-work_review=info,work_review_core=info}"
exec "\$APPIMAGE" --no-sandbox "\$@" >>"\$LOGDIR/run.log" 2>&1
EOF
chmod +x "$BIN_DIR/work-review"

cat > "$DESKTOP_DIR/work-review.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Work Review
Comment=工作回顾与AI日报助手
Exec=$BIN_DIR/work-review %U
Icon=Work_Review
Terminal=false
Categories=Office;
StartupWMClass=Work_Review
EOF
update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true

# ---- 6. summary -----------------------------------------------------------
log "6/6 done"
cat <<NOTES

Installed:
  AppImage   : $APPIMAGE
  Launcher   : $BIN_DIR/work-review
  Desktop    : $DESKTOP_DIR/work-review.desktop
  Icon       : $ICON_DIR/Work_Review.png
  Shims      : $BIN_DIR/{gdbus,gnome-screenshot,grim,spectacle,tesseract}
  Extension  : $EXT_DIR

Next (one-time manual):
  1. Log out of GNOME and log back in (Wayland can't reload Shell in-place).
  2. gnome-extensions enable focused-window-dbus@flexagoon.com
  3. Press Super, search "Work Review", click to launch.

To remove, run: $SCRIPT_DIR/uninstall.sh
NOTES
