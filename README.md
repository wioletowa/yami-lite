# yami-lite

Пиксельный шелл для niri в одном бинарнике, основан на [yami](https://github.com/Trendorin/yami).

## Установка

```sh
git clone https://github.com/wioletowa/yami-lite
cd yami-lite
cargo build --release
cp target/release/yami-lite ~/.local/bin/
```

В `~/.config/niri/config.kdl`:

```kdl
spawn-at-startup "yami-lite"

binds {
    Mod+S      { spawn "yami-lite" "msg" "settings"; }
    Mod+Escape { spawn "yami-lite" "msg" "power"; }
    Mod+L      { spawn "yami-lite" "msg" "lock"; }
}
```

Все команды: `yami-lite help`.

MIT, шрифты — OFL (`fonts/`).
