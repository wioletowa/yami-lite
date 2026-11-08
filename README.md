# yami-lite

Личная облегчённая версия пиксельного шелла [yami](https://github.com/Trendorin/yami) для niri:
один бинарник, без установщика, без своих тем в системе.

Что внутри: бар, док, уведомления (с историей и ответом), трей, всплывашки (календарь, звук,
музыка с текстами песен, сеть, bluetooth, система, батарея), OSD, блокировка (PAM), меню
питания, виджеты рабочего стола, обои (своя картинка или пиксельные из палитры), ночной свет
(wlsunset), свои темы, игровой режим (уведомления ждут, пока игра в фокусе), настройки.

Чего нет: лаунчера, буфера и скриншотов (это отдельные программы), установщика, обновлений,
yami-files / view / play / greet, управления конфигом niri.

## Сборка

```sh
cargo build --release
cp target/release/yami-lite ~/.local/bin/
```

Шрифты Tiny5 и DotGothic16 вшиты в бинарник. Настройки создаются сами при первом изменении:
`~/.config/yami-lite/settings.json` (их можно править руками или `yami-lite config set`).

## niri

Шелл не пишет в конфиг niri — бинды твои:

```kdl
spawn-at-startup "yami-lite"

binds {
    Mod+S      { spawn "yami-lite" "msg" "settings"; }
    Mod+Escape { spawn "yami-lite" "msg" "power"; }
    Mod+L      { spawn "yami-lite" "msg" "lock"; }
    Mod+B      { spawn "yami-lite" "msg" "bar"; }
    XF86AudioPlay { spawn "yami-lite" "msg" "media" "play-pause"; }
}
```

Все команды: `yami-lite help msg`.

Настройки — обычное окно niri (app-id `yami-lite-settings`), в ленте, как любая программа.
Хочешь плавающим — правило в своём config.kdl:

```kdl
window-rule {
    match app-id="^yami-lite-settings$"
    open-floating true
    default-column-width { fixed 900; }
    default-window-height { fixed 640; }
}
```

## Перекраска программ

Настройки → цвет → программы. Каждая выключена, пока не включишь: GTK 3/4, Qt (qt6ct/qt5ct),
kitty, foot, alacritty, fuzzel, niri. Включённая получает файл `yami-lite-colors.*` рядом со своим
конфигом и одну строку подключения в нём (для niri — `include "yami-lite-colors.kdl"` в конце
`config.kdl`). Выключил — строка уходит, остальной конфиг не трогается. Только цвета: тем,
значков и курсоров шелл не ставит.

## Лицензия

MIT, как у yami. Шрифты — OFL (`fonts/`).
