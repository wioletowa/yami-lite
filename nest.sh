#!/bin/sh
# Шелл во вложенном niri (окно на текущем столе) — проверить правку без установки.
#   ./nest.sh start          — собрать (без оптимизаций), поднять niri в окне и шелл в нём
#   ./nest.sh reload         — пересобрать и перезапустить только шелл
#   ./nest.sh shot [файл]    — снимок вложенного экрана (по умолчанию target/nest/shot.png)
#   ./nest.sh msg …          — yami-lite msg … во вложенный шелл
#   ./nest.sh niri …         — niri msg … во вложенный niri (action spawn, outputs…)
#   ./nest.sh log            — хвост лога шелла (паники, eprintln)
#   ./nest.sh stop           — закрыть
# Всё своё: XDG-папки в target/nest (настройки при первом запуске копируются из ~/.config),
# отдельная сессия D-Bus — уведомления и трей настоящего шелла не трогаются.
# FEATURES=desktop,privacy ./nest.sh start — другие модули (по умолчанию full).
set -eu

cd "$(dirname "$0")"
root="$PWD"
nest="$root/target/nest"
run="$nest/run"
bin="$root/target/debug/yami-lite"
features="${FEATURES:-full}"

host_display="${WAYLAND_DISPLAY:-wayland-1}"
case "$host_display" in
    /*) ;;
    *) host_display="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/$host_display" ;;
esac

sandbox_env() {
    export XDG_RUNTIME_DIR="$run"
    export XDG_CONFIG_HOME="$nest/config"
    export XDG_STATE_HOME="$nest/state"
    export XDG_CACHE_HOME="$nest/cache"
    export XDG_DATA_HOME="$nest/data"
}

niri_sock() {
    ls "$run"/niri.*.sock 2>/dev/null | head -n1
}

build() {
    cargo build --features "$features" 2>&1 | grep -E "^(error|warning: unused)" -A5 || true
}

# шелл внутри: через niri, чтобы получил его окружение (дисплей, сокет niri, D-Bus)
spawn_shell() {
    NIRI_SOCKET="$(niri_sock)" niri msg action spawn -- sh -c "echo \$\$ >'$nest/shell.pid'; RUST_BACKTRACE=1 exec '$bin' >>'$nest/shell.log' 2>&1" >/dev/null
}

case "${1:-}" in
    start)
        build
        if [ -n "$(niri_sock)" ] && kill -0 "$(cat "$nest/niri.pid" 2>/dev/null)" 2>/dev/null; then
            echo "уже запущен: ./nest.sh reload"
            exit 0
        fi
        rm -rf "$run"
        mkdir -p -m 700 "$run"
        mkdir -p "$nest/config" "$nest/state" "$nest/cache" "$nest/data"
        if [ ! -e "$nest/config/yami-lite" ] && [ -d "$HOME/.config/yami-lite" ]; then
            cp -r "$HOME/.config/yami-lite" "$nest/config/yami-lite"
        fi
        cat >"$nest/niri.kdl" <<EOF
hotkey-overlay { skip-at-startup; }
prefer-no-csd
screenshot-path null
spawn-at-startup "sh" "-c" "echo \$\$ >'$nest/shell.pid'; RUST_BACKTRACE=1 exec '$bin' >>'$nest/shell.log' 2>&1"
binds {
    Mod+S { spawn "$bin" "msg" "settings"; }
    Mod+Escape { spawn "$bin" "msg" "power"; }
    Mod+Q { close-window; }
    Mod+T { spawn "foot"; }
}
EOF
        (
            sandbox_env
            export WAYLAND_DISPLAY="$host_display"
            unset NIRI_SOCKET DISPLAY
            exec setsid dbus-run-session -- niri -c "$nest/niri.kdl" >"$nest/niri.log" 2>&1
        ) &
        echo $! >"$nest/niri.pid"
        # сокет niri и первый кадр шелла
        i=0
        while [ -z "$(niri_sock)" ] && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
        [ -n "$(niri_sock)" ] || { echo "niri не поднялся, см. $nest/niri.log"; exit 1; }
        echo "вложенный niri: $(niri_sock)"
        ;;
    reload)
        build
        sandbox_env
        "$bin" msg quit >/dev/null 2>&1 || true
        # прошлый должен уйти совсем: живой сокет — и новый решит, что шелл уже запущен. Только
        # свой, по номеру из shell.pid: тот же бинарник может работать и в настоящем сеансе
        pid="$(cat "$nest/shell.pid" 2>/dev/null || true)"
        if [ -n "$pid" ]; then
            i=0
            while kill -0 "$pid" 2>/dev/null && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
            kill -9 "$pid" 2>/dev/null || true
        fi
        rm -f "$run/yami-lite.sock"
        spawn_shell
        # отвечает — значит, готов принимать msg
        i=0
        while ! "$bin" msg ping >/dev/null 2>&1 && [ $i -lt 100 ]; do sleep 0.1; i=$((i + 1)); done
        ;;
    shot)
        out="${2:-$nest/shot.png}"
        sandbox_env
        WAYLAND_DISPLAY=wayland-1 grim "$out"
        echo "$out"
        ;;
    msg)
        shift
        sandbox_env
        exec "$bin" msg "$@"
        ;;
    niri)
        shift
        NIRI_SOCKET="$(niri_sock)" exec niri msg "$@"
        ;;
    log)
        tail -n "${2:-40}" "$nest/shell.log"
        ;;
    stop)
        pid="$(cat "$nest/niri.pid" 2>/dev/null || true)"
        [ -n "$pid" ] && pkill -P "$pid" 2>/dev/null; [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
        NIRI_SOCKET="$(niri_sock)" niri msg action quit --skip-confirmation 2>/dev/null || true
        rm -f "$nest/niri.pid"
        ;;
    *)
        sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
        ;;
esac
