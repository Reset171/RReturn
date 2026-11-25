# RReturn — Agent & Contributor Guide

Native timeline-rewind mod for **Horizon** (Minecraft: Java Edition **26.3**).

## 1. Package & Build Pipeline (Critical)

- **Flat `.hmod` format:** `horizon-mod.toml` is the **Single Source of Truth** for `game`, `horizon`, and `mixins` pins. Never duplicate version pins in scripts or documentation.
- **Generated runtime:** `src/runtime.rs` is assembled by `build.rs` from `src/runtime/*.rs` and is **gitignored** — never commit `src/runtime.rs` and never edit it directly.
- **`cargo check` / `cargo build` does NOT compile the mod against Horizon!** It only checks the standalone unit-test crate (`src/lib.rs`), regenerates `src/runtime.rs`, and packages `target/rreturn-<version>.hmod`.
- **Do NOT build directly from `.` (`horizon-launcher native-build .`):** `tools/package.py` injects `horizon-mod-api` into packaged `Cargo.toml` and registers settings/keybindings (`impl mod_api::Hooks`) into packaged `src/lib.rs` inside `.hmod`.
- **Mandatory build & verify pipeline after any Rust/manifest edit:**
  ```sh
  python3 tools/pr_ready.py --quick
  python3 tools/package.py --deploy
  horizon-launcher native-build --trust-native-code
  ```
- **No auto-deploy on `cargo build`:** `build.rs` only builds into `target/`. Deployment to launcher instances happens strictly via explicit `tools/package.py --deploy`.
- **Single `.hmod` invariant:** Never keep duplicate `.hmod` files (e.g. `rreturn.hmod` + `rreturn-0.2.0.hmod`) in `horizon-native-mods/` — Horizon Launcher treats each file as a separate mod.

## 2. Build Assembly & Horizon Mixin Invariants

Violating any of these breaks `horizon-launcher native-build`:

1. **Register every new runtime module in 3 places:**
   - `RUNTIME_FILES` in `build.rs` (in dependency order),
   - `src/runtime/mod.rs`,
   - this `AGENTS.md` layout map.
2. **NO inline `super::<sibling>::fn()` calls inside function bodies in `src/runtime/*.rs`:**
   - `build.rs` strips top-level `use super::...;`, `use horizon_...;`, `use std::...;` lines when flattening into `src/runtime.rs`, which is injected into Horizon's `autosave.rs` (where `super` is `horizon_client::game`).
   - Always import at top of file (`use super::scrub_audio::play_scrub_sounds;`) and call unqualified (`play_scrub_sounds(...)`). Add new external Horizon/std imports to the generated header in `build.rs`.
3. **Engine extension constraints (`src/ext/*_ext.rs`):**
   - `[[native.module]]` targets are ONLY allowed inside `crates/horizon-client/src/game/...` (files in `horizon-client-lib`, `horizon-gameplay`, `horizon-client-render`, `src/lan/` fail with `E_MIXIN_MODULE_DESTINATION` / `E_MIXIN_SOURCE`).
   - Methods on Horizon types MUST use **Extension Traits** (`pub trait FooExt` + `impl FooExt for Foo`), never inherent `impl Foo` (orphan rule `E0116` inside `horizon_patched`).
   - Game area directories containing `.game-area` (`world/`, `mobs/`, `network/`, `items/`, `interface/`, `modding/`) are flattened in Rust module paths (`world/autosave.rs` → `horizon_client::game::autosave`).
4. **Mixin hook constraints (`src/hooks/hook_*.rs` & `horizon-mod.toml`):**
   - Never target functions containing `#[cfg(...)]` in their body (`E_UNSUPPORTED_CFG`).
   - Parameter names and types in `bindings` must match helper signatures 1-to-1 (including leading `_`).
   - Input blocking hooks: target `mod_movement_keys` (`cancellable = true`), not `native_movement_input`; in `hook_key_mapping_pre`, use a **blacklist** of gameplay actions only (never a whitelist that blocks `U`, `Esc`, `Enter`, or arrow keys).
5. **Threading & Deadlock Safety:**
   - Horizon structs (`Mob`, `ArmorStand`, `PlayerMotion`, `LevelSnapshot`, `GuestSnapshot`) contain `Rc<RefCell<...>>` (`!Send + !Sync`) → keep snapshots, `SCRUB_RING`, and `SERVER_JOURNAL` in `thread_local!`, never in `static OnceLock<Mutex<...>>`.
   - **NEVER hold a `Mutex` (e.g. `session_lock()`) across `game.integrated.call(...)`** — causes a 10-second deadlock with the server thread. Use `AtomicBool` (e.g. `SCRUB_ACTIVE`) for cross-thread flags and `drop(guard)` before any `server.call`.

## 3. Core Product & Architecture Invariants

- **100% Byte-Exact Determinism & Zero-Loss:** Never sacrifice rollback accuracy, drop particles/state, or introduce hacky approximations to save RAM/CPU. Optimize via true Copy-on-Write (`Arc`, `ListStamp`, sparse deltas, RRTL compression, direct chunk swap).
- **Strict Isolation of Rollback (`F6`/`F7`) vs Interactive Scrubbing (`U`):**
  - Both share `apply_timeline_snapshot_on_server` (`RollbackOrigin::DiskSlot` vs `RollbackOrigin::ScrubSnapshot`).
  - Regular checkpoint rollback works deterministically and **must NOT be altered** when fixing scrubbing. All scrub-specific behavior in `restore*.rs` must be gated behind `is_scrub` / `slot == "scrub"`.
- **Zero-Cost Scrubbing When Disabled:**
  - `scrub_enabled` defaults to `false` (toggled in mod settings or `/rreturn scrub on|off`).
  - When `false`, all tick/chunk-unload/scroll/render/HUD hooks must early-return immediately, and `game.audio.capture = None`, `game.audio.record_heard = false`.
- **Permissions:** Always respect the world's `Allow Cheats` setting (verify via integrated server `allow_commands()` in singleplayer, not just client `game.allow_commands()`).
- **Silent Chat & Clean Logs:**
  - Send mod chat messages strictly through `mod_chat(game, ...)` (outputs only when `dev_mode == true`, except `/rreturn dev` toggle confirmation).
  - No unconditional `eprintln!` debug spam in release code.
  - All log files must stay inside `*/logs/rreturn/` (`<instance>/logs/rreturn/rreturn.log`, `<world>/logs/rreturn/benchmark.log`), never in root world/instance directories.
- **Localization:** Use `tr()` / `tr_args()` with keys in `assets/rreturn/lang/en_us.json` and `ru_ru.json`. Keep Russian strings idiomatic without English duplicates in parentheses.
- **Modularity:** Keep modules focused by domain; avoid growing monolithic files in `src/runtime/` or dumping unrelated tests into a single catch-all test file.
- **Horizon Registry Compliance (`MOD-POLICY.md`):** Keep `license = "GPL-3.0-only"` in `horizon-mod.toml` and `Cargo.toml` matching `LICENSE`; no Mojang assets; no hidden network activity; SemVer `X.Y.Z` everywhere.

## 4. Repository Layout

- `horizon-mod.toml` & `src/hooks/hook_*.rs` — Unified manifest, mixin declarations, and thin hook helpers injected into `horizon-client`.
- `src/ext/*_ext.rs` — Engine extension traits (`dragon_ext.rs`, `end_ext.rs`, `fishing_ext.rs`, `potions_ext.rs`, `remote_ext.rs`, `stats_ext.rs`).
- `src/runtime/` — Modular runtime implementation:
  - `types.rs` — `RReturnState`, `AnchorMode`, `RollbackOrigin`, permissions, `mod_chat`, localization.
  - `policy.rs` — `RollbackPolicy` (35 deterministic rollback flags).
  - `snapshots.rs`, `ui_snapshot.rs` — `TimelineSnapshot`, `LevelSnapshot`, `GuestSnapshot`, `ClientBlockSnapshot`, `ClientUiSnapshot` & cursor/menu/chat persistence, `ROLLBACK_EPOCH`.
  - `capture.rs` — In-memory and disk checkpoint capture for active and parked dimensions.
  - `storage.rs` — Slot directories, atomic/incremental copying, pruning of extra disk files.
  - `persistence.rs` — Atomic checkpoint serialization to `saves/<World>/rreturn/` (`save_slot_to_disk`).
  - `load_disk.rs` — Deserialization of `meta.json`, `rreturn_globals.dat`, and player NBT (`load_slot_from_disk`).
  - `hydration_chunks.rs`, `hydration_entities.rs`, `hydration.rs` — Lazy disk hydration for chunks, entities, and slots.
  - `restore_level.rs`, `restore_players.rs`, `restore_client.rs`, `restore.rs` — Unified rollback core (`apply_timeline_snapshot_on_server`, `nearest-first` client mesher diffing).
  - `rrtl.rs`, `scrub_codec.rs` — RRTL binary timeline format, compact patches, particle/mob SoA codec.
  - `scrub_server_types.rs`, `scrub_server_journal.rs`, `scrub_server_diff.rs`, `scrub_server.rs` — Server-side CoW timeline journal, unloaded chunk baseline tracking, differential capture, and snapshot materialization.
  - `scrub_audio.rs`, `scrub_timeline.rs`, `scrub_interpolate.rs`, `scrub_hud.rs`, `scrub.rs`, `scrub_perf.rs` — Client-side sub-tick ring buffer, 60–144 FPS interpolation, async reverse OGG playback, dark floating Scrub HUD controller, and phase profiler.
  - `adaptive.rs`, `tracker.rs`, `benchmark.rs` — Adaptive safety auto-saves, F3+B `LoopTracker`, phase timers (`logs/rreturn/`).
  - `commands.rs`, `telemetry.rs`, `ui.rs`, `settings_catalog.rs`, `settings_ui_draw.rs`, `settings_ui_icons.rs`, `settings_ui_tabs.rs`, `settings_ui_players.rs`, `settings_ui.rs`, `hooks.rs` — `/rreturn` command routing, JSON telemetry, settings sync, RReturn menu, icon quick-jump bar, per-player overrides UI, lifecycle/input hooks.
- `src/lib.rs` — Standalone unit-test crate for slot validation and atomic checkpoint I/O helpers.
- `tools/` — `package.py` (deterministic `.hmod` packager), `pr_ready.py` (pre-commit suite), `rreturn_harness.py` (control socket client), `rreturn_e2e.py` & `tools/tests/` (modular E2E suite), `rreturn_bench.py`, `rreturn_spark.py`.

## 5. Testing & Quality Standards

- **Isolation:** Run tests exclusively in temporary `rreturn_e2e_*` worlds via `RReturnHarness`. Always shut down Horizon cleanly with `quit`, never `SIGKILL`.
- **Test-Driven Bugfixing:** Every bug fix requires a reproducing E2E test in the matching thematic module under `tools/tests/` registered in `tools/rreturn_e2e.py`.
- **Strict Assertions (No False-Positive Tests):**
  - Test parsers must return `None` / fail on malformed output — never substitute passing defaults (e.g., never default health to `20.0`).
  - Always assert `h.has_entity(...)` before checking parsed coordinates; never rely on `"15." not in raw_str` (passes if entity disappeared).
  - Disk persistence tests MUST call `h.rreturn("purge_memory")` before `load`.
  - Sync scrub rewind steps to real `gametime` or `scrub rewind_ticks <N>` (remember `scrub step` applies wheel multiplier `1.5 * jog_accel`).
  - Horizon 26.3 test specifics: use full namespaced IDs (`minecraft:speed`), double quotes in SNBT, `NoAI: 0b` for active AI (`1b` disables AI), light > 8 for passive spawners, `worldborder center` before shrinking at distant coords, keep `instant_health` amplifier ≤ 5.
- **Git Hygiene:** Keep every commit bisect-safe (compiles cleanly); never commit personal paths, local instance names, or generated `src/runtime.rs`.

## 6. Verification Commands

```sh
python3 tools/pr_ready.py --quick        # cargo check, package .hmod, clippy -D warnings, cargo test, python syntax
python3 tools/package.py --deploy        # deploy .hmod to active launcher instance
horizon-launcher native-build --trust-native-code  # compile native Horizon binary with RReturn
python3 tools/rreturn_e2e.py --fast      # 5-test smoke suite
python3 tools/rreturn_e2e.py --test <N>  # single test
python3 tools/rreturn_e2e.py --group <g> # thematic group (scrubbing, determinism, mobs, portals, etc.)
python3 tools/rreturn_e2e.py             # full E2E regression suite
```
