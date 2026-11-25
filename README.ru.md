**[English](README.md) · [Русский](README.ru.md)**

Мод добавляет возвращение к точкам сохранения при гибели и интерактивную перемотку времени для движка [Horizon](https://github.com/horizon-rust/horizon).
Восстанавливает чанки, сущностей, блок-сущности, плановые тики, состояние ГСЧ, измерения и параметры игрока к точкам сохранения либо интерактивно через скраббинг колёсиком мыши.

Скрипт `build.rs` собирает подмодули `src/runtime/*.rs`, проверяет миксины из `horizon-mod.toml` и упаковывает плоский детерминированный пакет `target/rreturn-<version>.hmod`.

```sh
cargo build
```

Установка `target/rreturn-<version>.hmod` и сборка в инстанс Horizon:

```sh
cp target/rreturn-*.hmod ~/.local/share/horizon/horizon-native-mods/
horizon-launcher native-build --trust-native-code
```

Либо автоматический деплой пакета в активный инстанс лаунчера:

```sh
python3 tools/package.py --deploy
horizon-launcher native-build --trust-native-code
```

| Клавиша / Команда                                         | Описание                                                          |
| --------------------------------------------------------- | ----------------------------------------------------------------- |
| `U` / Колёсико мыши                                       | Интерактивная перемотка времени (удержание/переключение, подтверждение `Enter`) |
| `/rreturn scrub <start\|commit\|cancel\|step>`            | Консольное управление субтиковым скраббингом времени              |
| `F6` / `/rreturn set [slot]`                              | Сохранить чекпоинт в слот (по умолчанию `default`)                |
| `F7` / `/rreturn load [slot]`                             | Откатить мир и игрока к слоту                                     |
| `/rreturn anchor mode <mode>`                             | Режим ключевого игрока (`fixed`, `multi`, `everyone`, `rotating`) |
| `K` / `/rreturn menu`                                     | Открыть меню настроек RReturn и правил игроков                    |
| `/rreturn anchor cooldown <sec>`                          | Кулдаун между откатами в секундах (`0` — выключен)                |
| `/rreturn slot <list\|save\|load\|select\|delete> [name]` | Управление именованными слотами                                   |
| `/rreturn policy <get\|set\|reset> [field] [bool]`        | Просмотр и изменение флагов политики отката                       |
| `/rreturn autosave <on\|off\|interval\|adaptive> [sec]`   | Настройка автосохранений                                          |
| `/rreturn f3 [on\|off]`                                   | Подсветка хитбоксов мобов (F3+B) относительно прошлой петли       |
| `/rreturn status` / `/rreturn json` / `/rreturn metrics`  | Статус и метрики угрозы (текст или JSON)                          |
| `/rreturn bench`                                          | Пофазная сводка времени последнего отката                         |

```sh
python3 tools/pr_ready.py --quick
python3 tools/rreturn_e2e.py --fast
python3 tools/rreturn_e2e.py
python3 tools/rreturn_bench.py
python3 tools/rreturn_spark.py --record 5
```

[GPL-3.0-only](LICENSE).