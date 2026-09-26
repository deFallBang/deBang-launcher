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
# Обёртка deBang Launcher.
#  1) окно запускается как debang-launcher (совпадает с StartupWMClass);
#  2) если окружение «очищено» (меню/панель не передали WAYLAND_DISPLAY и
#     XDG_RUNTIME_DIR), берём их из окружения живого композитора — иначе GTK
#     падает с "Failed to initialize gtk backend";
#  3) GDK_BACKEND намеренно не трогаем: пустое значение ломает GTK.
APP="$HOME/.local/share/debang-launcher-app/debang-launcher"

if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
  for proc in Hyprland sway weston niri; do
    pid=$(pgrep -x "$proc" 2>/dev/null | head -n1)
    [ -n "$pid" ] || continue
    # переменные из окружения композитора (NUL-разделённые)
    while IFS= read -r -d '' kv; do
      case "$kv" in
        WAYLAND_DISPLAY=*|XDG_RUNTIME_DIR=*|DISPLAY=*) export "$kv" ;;
      esac
    done < "/proc/$pid/environ" 2>/dev/null
    [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] && break
  done
fi

# рантайм-каталог по умолчанию, если его не передали
if [ -z "${XDG_RUNTIME_DIR:-}" ] && [ -d "/run/user/$(id -u)" ]; then
  export XDG_RUNTIME_DIR="/run/user/$(id -u)"
fi

# Hyprland не отдаёт WAYLAND_DISPLAY в своём окружении — ищем сокет
if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ] && [ -n "${XDG_RUNTIME_DIR:-}" ]; then
  for cand in "$XDG_RUNTIME_DIR"/wayland-*; do
    if [ -S "$cand" ]; then
      export WAYLAND_DISPLAY="$(basename "$cand")"
      break
    fi
  done
fi

if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
  echo "deBang Launcher: не найден WAYLAND_DISPLAY/DISPLAY — нет графической сессии." >&2
  exit 1
fi

export WEBKIT_DISABLE_DMABUF_RENDERER="${WEBKIT_DISABLE_DMABUF_RENDERER:-1}"
exec -a debang-launcher "$APP" "$@"
EOF
chmod 755 "$BIN_DIR/debang-launcher"

# The desktop entry must use an absolute path: many launchers (Hyprland app
# menus, some panels) do not put ~/.local/bin into PATH, so a bare command
# name fails silently.
sed "s|^Exec=.*|Exec=$BIN_DIR/debang-launcher %U|" packaging/debang-launcher.desktop \
  > "$APPS/app.debang.launcher.desktop"
chmod 644 "$APPS/app.debang.launcher.desktop"
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
