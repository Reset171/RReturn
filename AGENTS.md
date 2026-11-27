# RReturn — Agent & Contributor Guide

Native timeline-rewind mod for **Horizon**.

## 1. Package & Build Pipeline (Critical)

- **Flat `.hmod` format:** `horizon-mod.toml` is the **Single Source of Truth** for `game`, `horizon`, and `mixins` pins. Never duplicate version pins or commit hashes in scripts or docs.
- **Generated runtime:** `src/runtime.rs` is assembled by `build.rs` from `src/runtime/*.rs` and is **gitignored** — never commit or edit it directly.
- **`cargo check` / `cargo build` does NOT compile the mod against Horizon!** It only checks `src/lib.rs`, regenerates `src/runtime.rs`, and packages `target/rreturn-<version>.hmod`.
- **Do NOT run `horizon-launcher native-build .`:** `tools/package.py` injects `horizon-mod-api` and `impl mod_api::Hooks` (keybindings + settings) into the packaged `.hmod`.
- **Automated pre-flight & build pipeline after any Rust / manifest edit:**
  - `tools/pr_ready.py --quick` enforces runtime assembly sync, inline `super::` ban, mixin AST/binding rules, local `functions.json` target check, localization key parity, privacy/debug hygiene, `clippy -D warnings`, unit tests, and Python syntax.
  - `tools/package.py --deploy` regenerates `src/runtime.rs` (unless `--skip-check`), packages `.hmod`, deploys to the active instance, and supports `--native-build` to compile in one step:
  ```sh
  python3 tools/pr_ready.py --quick && python3 tools/package.py --deploy --native-build
  ```
- **Single `.hmod` invariant:** Keep strictly one `rreturn-*.hmod` in `horizon-native-mods/` (duplicates appear as separate mods).

## 2. Build Assembly & Horizon Mixin Pre-Flight Rules

Verify ALL of these **before** running `native-build` (saves 35 s per failed build):

1. **Register every new `src/runtime/<mod>.rs` in 3 places:** `RUNTIME_FILES` in `build.rs` (dependency order), `src/runtime/mod.rs`, and this `AGENTS.md`.
2. **Flattened runtime rules (`build.rs`):**
   - `build.rs` **STRIPS all top-level `use super::...;`, `use horizon_...;`, `use std::...;`** lines when merging `src/runtime/*.rs` into one module inside `autosave.rs`!
   - Every external type/function/trait used unqualified in `src/runtime/*.rs` MUST be added to `build.rs`'s generated header (or written by fully-qualified path `horizon_client::...`, `horizon_core::...`).
   - **NO inline `super::<sibling>::fn()` calls inside function bodies** — import at file top and call unqualified.
   - All top-level item names across `src/runtime/*.rs` must be globally unique.
   - Avoid giant `serde_json::json!({ ... })` literals (recursion limit) — build large objects via `serde_json::Map`.
3. **Engine extension constraints (`src/ext/*_ext.rs`):**
   - `[[native.module]]` is ONLY allowed inside `crates/horizon-client/src/game/...` (cannot target `mobs/`, `lan/`, `horizon-client-render`, or `horizon-gameplay`).
   - Extensions on Horizon types MUST use **Extension Traits** (`pub trait FooExt`), never inherent `impl Foo` (`E0116` in `horizon_patched`).
   - `.game-area` subdirectories (`world/`, `mobs/`, `network/`, `items/`, `interface/`, `modding/`) are flattened in Rust module paths.
4. **Horizon Mixin AST Limits (`horizon-mixins`):**
   - Always check the active build's `functions.json` **before** adding a mixin target to confirm exact symbol path and replaceability.
   - `HEAD` (including `cancellable = true`) bypasses body AST rewriting.
   - `RETURN`, `BEFORE_CALL`, `AFTER_CALL`, `WRAP_OPERATION`, and `MODIFY_EXPRESSION_VALUE` rewrite the target body and FAIL with:
     - `E_UNSUPPORTED_CFG` if the target body contains `#[cfg(...)]`;
     - `E_UNSUPPORTED_MACRO` if the target body contains unqualified/prelude macros (`format!`, `matches!`, `assert!`, `log::warn!`, etc.);
     - `E_UNSUPPORTED_CONTROL_FLOW` if the target body contains `?`, closures, `async`, or `unsafe`;
     - `E_MIXIN_NOT_REPLACEABLE` if the target has `impl Trait` / generic / `const` / `unsafe` / `#[inline(always)]` signatures.
   - Work around body-rewrite rejections by targeting macro-free callers or using cancellable `HEAD` mixins.
   - Exact binding rules:
     - `WRAP_OPERATION` `$operation` MUST specify explicit return type (`impl FnOnce(...) -> ()`, **no trailing comma** in arg list).
     - `RETURN` `$value` MUST be bound **before** `&mut *self` (`_val: T, game: &mut Game<'_>`) to avoid two-phase borrow errors (`E0499`/`E0503`).
     - Binding `value` expressions can only bind parameters/`&*self`/`&mut *self` — never field projections like `&mut *self.uuids` (`E_INVALID_BINDING`).
   - Input blocking: target `mod_movement_keys` (`cancellable = true`); in `hook_key_mapping_pre`, blacklist gameplay actions only (never block `U`, `Esc`, `Enter`, arrows).
5. **Threading & Deadlock Safety:**
   - Horizon structs (`Mob`, `PlayerMotion`, `LevelSnapshot`, etc.) are `!Send + !Sync` → keep snapshots, journals, and rings in `thread_local!`.
   - **NEVER hold a `Mutex` (e.g. `session_lock()`) across `game.integrated.call(...)`** — use `AtomicBool` for cross-thread flags and `drop(guard)` before `server.call`.

## 3. Core Architecture, Determinism & UX Invariants

- **Modularity & No Fragile Hacks:**
  - Keep `src/runtime/*.rs` modules focused and compact (target ≤ 400–600 lines; split before exceeding ~700 lines).
  - Never build brittle `Debug`-string AST parsers (`format!("{:?}", ...)`) to bypass private upstream trait objects or fields outside `crates/horizon-client/src/game/`.
- **100% Byte-Exact Determinism & Zero-Loss:** Optimize via true Copy-on-Write (`Arc`, `ListStamp`, sparse deltas, RRTL) — never drop state or precision.
  - **Server PRNG (`det_rng.rs`):** ALWAYS call `clear_det_rng_transient_caches()` on every rollback/restore (prevents stale `DET_PRESEEDED_MOBS` from skipping respawned entities that reuse IDs); isolate `DET_RNG_UNIQUIFIER` around `game.load_raids()`.
  - **Disk hydration (`hydration_entities.rs`, `hydration_chunks.rs`):**
    - Shift hydrated entity IDs by `DISK_ID_MOD_STRIDE = 24_000` (preserves `% 2..1200` tick phases and prevents client `Hub::entities` visual mob-swap); background pre-hydration must restore live `game.next_entity`.
    - When reusing live chunk sections during disk hydration, compare **`section.light`** in addition to blocks and biomes — otherwise future-timeline lighting leaks into rolled-back disk chunks.
  - **Level, block entities & streaming restoration (`restore.rs`, `restore_level.rs`, `restore_players.rs`):**
    - Call `enter_level(target_dim)` before recreating `Streaming`/`SaveDir`; call `game.streaming.abort_generation()` and preserve `forced` when recreating `Streaming` (also recreate if `streaming.busy() || terrain_loading() || pending() > 0`); call `sync_streaming_lit_from_world` (prevents `/tp` `tp_held` freeze); update `streaming.center` to restored player coords.
    - Promote pending block entity tickers (`promote_world_block_entity_tickers`) before capture and scrub recording; restore `be_ticker_order` and `be_asleep`; NEVER call `game.world.block_entities.values_mut()` globally during rollback (it wakes every sleeping block entity — target exact coordinates with `get_mut(&pos)`).
    - Preserve `game.died`, `OrderedTicks`, `ChunkTicks` `pending`/`queued` split, and `RaidRuntimeSnapshot` (`loaded`, `omen_position`, `omen_loaded`, `unseeded`, `bar_hurts`, `homes`).
    - On guest rollback, clear `g.moves` and `g.move_packet_queue`, and send `C::SetEntityMotion` when restoring non-zero velocity.
- **Rollback (`F6`/`F7`) vs Interactive Scrubbing (`U`) Isolation:**
  - Regular rollback works deterministically and must NOT be altered when fixing scrubbing; gate scrub-only logic behind `is_scrub` / `slot == "scrub"`.
  - Scrubbing is opt-in (`scrub_enabled = false` default) with zero background cost when off (`game.audio.capture = None`, `record_heard = false`).
  - Scrub sync: anchor `anchor_tick` to authoritative server `game_time` (prevents +1 client-ahead skew), gate `poll_remote` (`hook_poll_remote_pre`), drain loopback up to `CustomPayload("rreturn:fence")` on start/commit, clear server `block_log`/`bulk_log`/`light_log` on solo scrub commit, re-mark modified chunks on commit, and rewind `reference_chunks` on `SetTime` resync without `ROLLBACK_EPOCH` bump.
  - UI/container rollback: stash & clear `carried`/`craft_grid` before reopening server menus; use local-only container close (`rreturn_close_container_local`); mark machine-readable telemetry chat outputs via `mark_telemetry_chat_text` so they never enter `CHAT_JOURNAL`.
- **Settings Scope & UI/UX Standards:**
  - **Mods → Config** (Title Screen & Pause Menu) opens custom dark UI in **Global (`NEW WORLDS`)** mode (`config/rreturn.json`, no `Players` tab, does NOT affect active/existing worlds).
  - **`K` / `/rreturn settings`** opens full **world-scoped** UI (`<world>/rreturn/settings.json`, includes `Players` tab with 3-state `Global / Yes / No` overrides).
  - Clean, neutral, ergonomic UI copy (no third-party brand names in code/docs, no gimmick preset cards, no redundant hotkey suffixes like `(U)`/`(F6/F7)` or tab prefixes in headers, dynamic key names via `keybinds::display_name`, `64x64` PNG icons in `assets/rreturn/textures/rreturn_icons/`, integer-aligned concentric soft shadows).
- **Permissions, Chat, Logs & Policy:** Respect server `allow_commands`; chat via `mod_chat` (`dev_mode == true` only); zero `eprintln!` spam; all logs strictly in `*/logs/rreturn/`; bilingual `en_us.json` & `ru_ru.json` (clean native Russian without English parentheticals); `MOD-POLICY.md` compliant (`license = "GPL-3.0-only"`, SemVer `X.Y.Z`, no Mojang assets, no hidden network activity).

## 4. Repository Layout

- `horizon-mod.toml` & `src/hooks/hook_*.rs` — Unified manifest, mixin declarations, and thin hook helpers.
- `src/ext/*_ext.rs` — Engine extension traits:
  - Gameplay/world: `dragon_ext.rs`, `end_ext.rs`, `fishing_ext.rs`, `potions_ext.rs`, `stats_ext.rs`, `ordered_ext.rs`, `creatures_ext.rs`, `persist_ext.rs`, `raids_ext.rs`, `trader_ext.rs`, `trial_spawner_ext.rs`.
  - Client/UI/network: `remote_ext.rs`, `lan_menus_ext.rs`, `hand_ext.rs`, `perspective_ext.rs`, `pointer_capture_ext.rs`, `books_ext.rs`, `command_blocks_ext.rs`, `mount_menu_ext.rs`, `recipe_book_ext.rs`, `sign_edit_ext.rs`, `stations_ext.rs`, `startup_menu_ext.rs`.
- `src/runtime/` — Modular runtime:
  - Core & config: `types.rs`, `policy.rs`, `snapshots.rs`, `ui.rs`, `commands.rs`, `telemetry.rs`, `hooks.rs`, `adaptive.rs`, `tracker.rs`, `benchmark.rs`.
  - Persistence & hydration: `capture.rs`, `storage.rs`, `persistence.rs`, `load_disk.rs`, `hydration.rs`, `hydration_chunks.rs`, `hydration_entities.rs`, `mob_disk_extra.rs`, `det_rng.rs`.
  - Restoration: `restore.rs`, `restore_level.rs`, `restore_players.rs`, `restore_client.rs`, `ui_snapshot.rs`.
  - Scrubbing & RRTL: `rrtl.rs`, `scrub_server_types.rs`, `scrub_server_journal.rs`, `scrub_server_diff.rs`, `scrub_server.rs`, `scrub_audio.rs`, `scrub_timeline.rs`, `scrub_interpolate.rs`, `scrub_hud.rs`, `scrub.rs`, `scrub_perf.rs`.
  - Custom menu UI: `settings_catalog.rs`, `settings_ui_icons.rs`, `settings_ui_draw.rs`, `settings_ui_tabs.rs`, `settings_ui_players.rs`, `settings_ui.rs`.
- `assets/rreturn/` — Localizations (`lang/en_us.json`, `ru_ru.json`) and `64x64` UI icons (`textures/rreturn_icons/`).
- `src/lib.rs` — Standalone unit-test crate for slot validation and atomic checkpoint I/O.
- `tools/` — `package.py`, `pr_ready.py`, `rreturn_harness.py`, `rreturn_e2e.py` & `tools/tests/` (modular E2E suite), `rreturn_bench.py`, `bench_scrub_perf.py`, `rreturn_spark.py`.

## 5. Testing & Quality Standards

- **Strict Isolation (CRITICAL):**
  - `tools/rreturn_e2e.py` and `RReturnHarness()` run **isolated by default** (dedicated socket in `tempfile.gettempdir()`, `--workers 1`). Never pass `--attach-live` or touch `$XDG_RUNTIME_DIR/horizon-control.sock` from automated tests or diagnostic scripts; never run `world_settings` with `--workers > 1`.
  - `RReturnHarness` supports `with RReturnHarness(...) as h:` and provides `cleanup_stale_sockets()` for safe teardown without auto-launching Horizon. Always shut down cleanly with `quit`, never `SIGKILL`.
  - Never hardcode host/scratch paths in tests — always use `tempfile.gettempdir()`.
- **Test-Driven Bugfixing & Strict Assertions:**
  - Every bug fix requires a reproducing E2E test in the matching thematic module under `tools/tests/` registered in `tools/rreturn_e2e.py`.
  - Parsers must return `None` / fail on malformed output (never substitute passing defaults like `20.0`).
  - Assert `h.has_entity(...)` before checking parsed coordinates; call `purge_memory` before disk load assertions.
  - Because `--group` runs share one world across tests, measure **relative deltas** (e.g. trader `spawn_delay` change) rather than assuming fresh-world initial counters.
  - Performance: avoid looping `/data get entity` across dozens of mobs inside tight tick loops — use fast server-authoritative telemetry (`status --json`, `server_entities_digest`, `simulation blocks`).
  - Horizon test specifics: wait for `streaming_busy == False` before determinism checkpoints; sync scrub seeks to real `game_time`; full namespaced IDs (`minecraft:speed`); double quotes in SNBT; `NoAI: 0b` = AI ON; `ItemEntity` `Health` is `short` while living mobs use `float`; `/tick freeze` still ticks player status effects in real time (trigger timed effects via redstone command blocks or uninterrupted sprints); `/tick sprint` accelerates server ticks relative to 20 Hz client physics (shield survival observers in glass booths: `VisualShape::Empty` passes line-of-sight while solid collider blocks knockback); `TrialSpawner` intercepts `bad_omen` → `trial_omen` within range; `wait`/`type` require an active world.
- **Git Hygiene:** One logical change per bisect-safe commit; never commit personal paths, vendor UI brand names, temporary diagnostic telemetry, or generated `src/runtime.rs`.

## 6. Verification Commands

```sh
python3 tools/pr_ready.py --quick && python3 tools/package.py --deploy --native-build
python3 tools/rreturn_e2e.py --fast
python3 tools/rreturn_e2e.py --test 134 135 136
python3 tools/rreturn_e2e.py --group <exact_group_name>
python3 tools/rreturn_e2e.py
```
