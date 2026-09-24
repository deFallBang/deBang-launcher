#!/usr/bin/env bash
# Локальная установка deBang Launcher для пользователя (Linux, X11/Wayland).
# Использование: ./packaging/install-linux.sh [путь-к-бинарнику]
set -euo pipefail

BIN="${1:-src-tauri/target/release/debang-launcher}"
APP_DIR="$HOME/.local/share/debang-launcher-app"
BIN_DIR="$HOME/.local/bin"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor"

[ -f "$BIN" ] || { echo "Бинарник не найден: $BIN (собери: npm run tauri build)"; exit 1; }

mkdir -p "$APP_DIR" "$BIN_DIR" "$APPS" \
  "$ICONS/32x32/apps" "$ICONS/64x64/apps" "$ICONS/128x128/apps" "$ICONS/256x256/apps"

install -m755 "$BIN" "$APP_DIR/debang-launcher"
install -m644 src-tauri/icons/icon.png "$APP_DIR/icon.png"

cat > "$BIN_DIR/debang-launcher" <<'EOF'
#!/usr/bin/env bash
# Обёртка: окно запускается как debang-launcher (совпадает с StartupWMClass).
# ВАЖНО: не трогаем GDK_BACKEND — пустое значение ломает инициализацию GTK.
export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
exec -a debang-launcher "$HOME/.local/share/debang-launcher-app/debang-launcher" "$@"
EOF
chmod 755 "$BIN_DIR/debang-launcher"

install -m644 packaging/debang-launcher.desktop "$APPS/app.debang.launcher.desktop"
install -m644 src-tauri/icons/32x32.png "$ICONS/32x32/apps/debang-launcher.png"
install -m644 src-tauri/icons/64x64.png "$ICONS/64x64/apps/debang-launcher.png"
install -m644 src-tauri/icons/128x128.png "$ICONS/128x128/apps/debang-launcher.png"
install -m644 src-tauri/icons/128x128@2x.png "$ICONS/256x256/apps/debang-launcher.png"

command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -f -t "$ICONS" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" || true

echo "Установлено:"
echo "  бинарник: $APP_DIR/debang-launcher"
echo "  обёртка : $BIN_DIR/debang-launcher"
echo "  ярлык   : $APPS/app.debang.launcher.desktop"
