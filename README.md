**[English](README.md) · [Русский](README.ru.md)**

Timeline rewind mod (`Return by Death`) for [Horizon](https://github.com/horizon-rust/horizon).
Rewinds world chunks, entities, block entities, scheduled ticks, RNG state, dimensions, and player state to the last checkpoint on death or manual trigger.

`build.rs` assembles `src/runtime/*.rs`, synchronizes `src/ext/*.rs`, `src/hooks/*.rs`, `src/horizon-mixin.toml`, and `LICENSE` into `mixins/rreturn/`, and packages `target/rreturn-<version>.hnmod`.

```sh
cargo build
```

Install `target/rreturn-<version>.hnmod` and build into a Horizon instance:

```sh
cp target/rreturn-*.hnmod ~/.local/share/horizon/horizon-native-mods/
horizon-launcher native-build --trust-native-code
```

Or build directly from the source directory after `cargo build`:

```sh
horizon-launcher native-build . --trust-native-code
```

| Input / Command | Description |
|---|---|
| `F6` / `/rreturn set [slot]` | Save checkpoint to `slot` (default: `default`) |
| `F7` / `/rreturn load [slot]` | Roll back world and player state to `slot` |
| `F8` / `/rreturn anchor mode <mode>` | Set key player mode (`fixed`, `multi`, `everyone`, `rotating`) |
| `/rreturn anchor cooldown <sec>` | Set rollback cooldown in seconds (`0` disables cooldown) |
| `/rreturn slot <list\|save\|load\|select\|delete> [name]` | Manage named checkpoint slots |
| `/rreturn policy <get\|set\|reset> [field] [bool]` | Inspect or update rollback policy flags |
| `/rreturn autosave <on\|off\|interval\|adaptive> [sec]` | Configure automatic checkpoints |
| `/rreturn f3 [on\|off]` | Toggle F3+B mob hitbox color comparison against the previous loop |
| `/rreturn status` / `/rreturn json` / `/rreturn metrics` | Status and threat metrics (text or JSON) |
| `/rreturn bench` | Show timing breakdown of the last rollback |

```sh
python3 tools/pr_ready.py --quick
python3 tools/rreturn_e2e.py --fast
python3 tools/rreturn_e2e.py
python3 tools/rreturn_bench.py
python3 tools/rreturn_spark.py --record 5
```

[GPL-3.0-only](LICENSE).