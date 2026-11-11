# yami-lite

Пиксельный шелл для niri в одном бинарнике, основан на [yami](https://github.com/Trendorin/yami).

## Установка

```sh
git clone https://github.com/wioletowa/yami-lite
cd yami-lite
./install.sh --desktop --privacy   # ядро и выбранные модули, в ~/.local/bin
```

Без флагов собирается только ядро. `./install.sh --all` — все модули, `--prefix DIR` — другая
папка, `--no-install` — только собрать. Без скрипта то же самое:
`cargo build --release --features desktop,privacy` (все — `--features full`).

## Модули

| флаг | фича | что даёт |
|---|---|---|
| `--fan` | `fan` | вентилятор ThinkPad в баре: уровни, кривая демона thfn-gtk, график |
| `--wallhaven` | `wallhaven` | поиск и загрузка обоев с wallhaven.cc |
| `--desktop` | `desktop` | рабочий стол: файлы из `~/Desktop`, перетаскивание в окна и обратно, буфер файлов |
| `--recolor` | `recolor` | значки папок цвета акцента (Papirus, Neuwaita) |
| `--privacy` | `privacy` | замок в баре: приложение — чёрным в трансляции экрана |

Что собрано — `yami-lite version` и настройки → «о шелле».

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
