#!/bin/sh
# Сборка yami-lite с нужными модулями и установка в ~/.local/bin.
#   ./install.sh                         — только ядро
#   ./install.sh --fan --desktop         — ядро и выбранные модули
#   ./install.sh --all                   — всё
#   ./install.sh --all --no-install      — только собрать (target/release/yami-lite)
set -eu

usage() {
    cat <<'EOF'
yami-lite installer

  --fan         ThinkPad fan control in the bar (thfn-gtk daemon)
  --wallhaven   Wallhaven wallpaper search and download
  --desktop     desktop icons from ~/Desktop, drag and drop, file clipboard
  --recolor     recolor folder icons to the accent (Papirus, Neuwaita)
  --privacy     hide app windows from screencasts (bar lock)
  --all         every module above

  --prefix DIR  install into DIR (default ~/.local/bin)
  --no-install  only build
  -h, --help    this help
EOF
}

features=""
prefix="${HOME}/.local/bin"
install=1
add() { features="${features:+$features,}$1"; }

while [ $# -gt 0 ]; do
    case "$1" in
        --fan) add fan ;;
        --wallhaven) add wallhaven ;;
        --desktop) add desktop ;;
        --recolor) add recolor ;;
        --privacy) add privacy ;;
        --all) add full ;;
        --prefix) shift; prefix="$1" ;;
        --no-install) install=0 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

cd "$(dirname "$0")"
echo "building yami-lite with: ${features:-core only}"
if [ -n "$features" ]; then
    cargo build --release --features "$features"
else
    cargo build --release
fi

if [ "$install" = 1 ]; then
    mkdir -p "$prefix"
    # заменить файл, а не писать в него: запущенный шелл держит старый бинарник
    cp target/release/yami-lite "$prefix/yami-lite.new"
    mv -f "$prefix/yami-lite.new" "$prefix/yami-lite"
    echo "installed: $prefix/yami-lite"
    # значок и ярлык: `yami-lite` запускает шелл, а работающему открывает настройки
    # (окно настроек, app-id yami-lite-settings, — со своим значком)
    data="${XDG_DATA_HOME:-$HOME/.local/share}"
    mkdir -p "$data/icons/hicolor/scalable/apps" "$data/applications"
    cp assets/yami-lite.svg "$data/icons/hicolor/scalable/apps/yami-lite.svg"
    cp assets/yami-lite.desktop "$data/applications/yami-lite.desktop"
    rm -f "$data/applications/yami-lite-settings.desktop"
    # лаунчеры и тема значков подхватят сразу, без перезахода
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q "$data/applications" || true
    command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$data/icons/hicolor" || true
    echo "desktop entry: $data/applications/yami-lite.desktop (icon yami-lite)"
    "$prefix/yami-lite" version
    # запустить новый бинарник в фоне: работающий шелл перезапускается сам (exec на месте —
    # остаётся ребёнком niri), иначе — новый сеанс без терминала, вывод в никуда
    if [ -n "${WAYLAND_DISPLAY:-}" ]; then
        if "$prefix/yami-lite" msg ping >/dev/null 2>&1; then
            "$prefix/yami-lite" restart && echo "shell restarted" || echo "shell not restarted: yami-lite restart --force"
        else
            setsid -f "$prefix/yami-lite" </dev/null >/dev/null 2>&1
            echo "shell started in background"
        fi
    fi
fi
