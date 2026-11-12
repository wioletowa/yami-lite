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

Что делает скрипт после сборки:

- кладёт бинарник в `~/.local/bin` — заменой файла, так что работающий шелл не падает;
- ставит значок `yami-lite` и ярлык `yami-lite.desktop` в `~/.local/share`: из лаунчера
  `yami-lite` запускает шелл, а если он уже работает — открывает настройки, второй не стартует;
- запускает шелл в фоне: работающий перезапускается на месте (в игре или полноэкранном окне —
  нет, тогда `yami-lite restart --force`), не запущенный стартует через `setsid`, без терминала.

Автозапуск при входе — за niri (`spawn-at-startup` ниже), в его конфиг скрипт не пишет.

## Модули

| флаг | фича | что даёт |
|---|---|---|
| `--fan` | `fan` | вентилятор thfn-gtk |
| `--wallhaven` | `wallhaven` | поиск и загрузка обоев с wallhaven.cc |
| `--desktop` | `desktop` | рабочий стол |
| `--recolor` | `recolor` | значки папок цвета акцента (Papirus, Neuwaita) |
| `--privacy` | `privacy` | Приватность окон при демонстрации экрана |
| `--visualizer` | `visualizer` | пиксельный визуализатор звука по краям экрана |

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

С `--visualizer` кайма по краям лежит над обоями, и в обзоре обе остаются на месте во весь экран —
если пустить их в фон обзора (столы при этом прозрачные):

```kdl
layout {
    background-color "transparent"
}
layer-rule {
    match namespace="^yami-(wall|edge)$"
    place-within-backdrop true
}
```

Все команды: `yami-lite help`.

MIT, шрифты — OFL (`fonts/`).
