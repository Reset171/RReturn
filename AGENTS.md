Native timeline-rewind mod for **Horizon** (Minecraft: Java Edition **26.3**).

- **Flat .hmod package format:** Single manifest `horizon-mod.toml`. Edit `src/runtime/*.rs`, `src/ext/*.rs`, `src/hooks/*.rs`, and `horizon-mod.toml`; `build.rs` generates `src/runtime.rs` and packages `target/rreturn-<version>.hmod` via `tools/package.py` on `cargo check` / `cargo build` / `cargo test`.
- **Isolation:** Keep test runs in temporary `rreturn_e2e_*` worlds via `RReturnHarness` (`tools/rreturn_harness.py`). Shut down the game with `quit`, not `SIGKILL`.
- **Regression tests:** Every bug fix requires a reproducing E2E test in `tools/tests/test_<category>.py` registered in `tools/rreturn_e2e.py`.

- `horizon-mod.toml` & `src/hooks/hook_*.rs` — Manifest, mixin declarations, and hook helpers injected into `horizon-client`.
- `src/ext/*_ext.rs` — Engine extensions (`dragon_ext.rs`, `end_ext.rs`, `fishing_ext.rs`, `potions_ext.rs`, `remote_ext.rs`, `stats_ext.rs`).
- `src/runtime/` — Modular runtime implementation:
  - `types.rs` — `RReturnState`, `AnchorMode`, `AnchorSettings`, permissions, and localization.
  - `policy.rs` — `RollbackPolicy` (35 deterministic rollback flags).
  - `snapshots.rs` — `TimelineSnapshot`, `LevelSnapshot`, `GuestSnapshot`, `ClientBlockSnapshot`, `ROLLBACK_EPOCH`.
  - `capture.rs` — `create_slot_checkpoint_on_server`, level and parked dimension snapshot capture.
  - `storage.rs` — Slot directories, atomic/incremental file copying, and pruning of extra disk files.
  - `persistence.rs` — Atomic checkpoint serialization to `saves/<World>/rreturn/` (`save_slot_to_disk`).
  - `load_disk.rs` — Deserialization of `meta.json`, `rreturn_globals.dat`, and player NBT from disk (`load_slot_from_disk`).
  - `hydration_chunks.rs` — Lazy chunk, section, and block entity hydration from disk (`hydrate_chunks_from_disk`).
  - `hydration_entities.rs` — Entity spawning, ID remapping, and runtime state hydration from disk (`hydrate_entities_from_disk`).
  - `hydration.rs` — Level and slot hydration coordinator (`hydrate_level_from_disk`, `hydrate_slot_from_disk_on_server`).
  - `restore_level.rs` — In-place server level restoration (`apply_level_snapshot_in_place`).
  - `restore_players.rs` — Player motion, offline files, and LAN guest state restoration (`restore_players_on_rollback`).
  - `restore_client.rs` — Client `nearest-first` section diffing and mesher invalidation (`apply_client_level_rollback`).
  - `restore.rs` — Server rollback coordinator (`rollback_slot_checkpoint_on_server`).
  - `adaptive.rs` — Player safety checks, escape flood-fill, threat calculation, and interval/adaptive auto-saving.
  - `commands.rs` — `/rreturn` command routing and subcommand handlers.
  - `telemetry.rs` — Text and JSON status, threat metrics, and benchmark formatting.
  - `ui.rs` — Native mod settings and `config/rreturn.json` loading.
  - `tracker.rs` — `LoopTracker` F3+B hitbox color comparison across loops.
  - `scrub_audio.rs` — Timeline audio prewarming, reversal, and background playback.
  - `scrub_timeline.rs` — Sub-tick ring buffer, MobPoseRecord, SoundRecord, and delta recording.
  - `scrub_server.rs` — Server-side COW snapshotting, delta compression, and scrub preview.
  - `scrub_interpolate.rs` — Sub-tick interpolation for mob poses, animations, and sound triggers.
  - `scrub.rs` — Interactive timeline scrubbing controller, UI overlay, and jog wheel handling.
  - `scrub_perf.rs` — Microsecond phase breakdown and performance diagnostics for scrub timeline.
  - `benchmark.rs` — Phase timers and log output (`logs/rreturn.log`, `saves/<World>/rreturn/benchmark.log`).
  - `hooks.rs` — `handle_lifecycle`, `handle_tick`, `intercept_guest_die`, `handle_key`.
- `src/lib.rs` — Standalone unit-test crate for slot name validation and atomic checkpoint I/O helpers.
- `tools/` — `package.py` (deterministic `.hmod` packager), `pr_ready.py` (pre-commit check), `rreturn_harness.py` (control socket client), `rreturn_e2e.py` & `tools/tests/` (104 E2E tests), `rreturn_bench.py` (stress benchmark), `rreturn_spark.py` (CPU profiler).

```sh
python3 tools/pr_ready.py --quick
python3 tools/pr_ready.py --e2e
python3 tools/rreturn_e2e.py --fast
python3 tools/rreturn_e2e.py --test <N>
python3 tools/rreturn_e2e.py --group <name>
python3 tools/rreturn_e2e.py
```
