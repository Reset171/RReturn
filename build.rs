use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNTIME_FILES: &[&str] = &[
    "types.rs",
    "policy.rs",
    "snapshots.rs",
    "ui_snapshot.rs",
    "benchmark.rs",
    "adaptive.rs",
    "capture.rs",
    "storage.rs",
    "persistence.rs",
    "load_disk.rs",
    "hydration_chunks.rs",
    "hydration_entities.rs",
    "hydration.rs",
    "restore_level.rs",
    "restore_players.rs",
    "restore_client.rs",
    "restore.rs",
    "telemetry.rs",
    "commands.rs",
    "ui.rs",
    "tracker.rs",
    "rrtl.rs",
    "scrub_audio.rs",
    "scrub_timeline.rs",
    "scrub_server_types.rs",
    "scrub_server_journal.rs",
    "scrub_server_diff.rs",
    "scrub_server.rs",
    "scrub_interpolate.rs",
    "scrub_hud.rs",
    "scrub.rs",
    "scrub_perf.rs",
    "hooks.rs",
];

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=horizon-mod.toml");
    println!("cargo:rerun-if-changed=LICENSE");
    println!("cargo:rerun-if-changed=src/runtime");
    println!("cargo:rerun-if-changed=src/ext");
    println!("cargo:rerun-if-changed=src/hooks");
    println!("cargo:rerun-if-changed=assets/rreturn/lang/en_us.json");
    println!("cargo:rerun-if-changed=assets/rreturn/lang/ru_ru.json");
    println!("cargo:rerun-if-changed=tools/package.py");
    for file_name in RUNTIME_FILES {
        println!("cargo:rerun-if-changed=src/runtime/{file_name}");
    }

    assemble_runtime(Path::new("src"));

    let target_dir = resolve_target_dir();
    run_packager(&target_dir);

    println!("cargo:rerun-if-changed=src/runtime.rs");
    let hmod_path = target_dir.join(format!("rreturn-{}.hmod", env!("CARGO_PKG_VERSION")));
    println!("cargo:rerun-if-changed={}", hmod_path.display());
}

fn write_if_changed(dst: &Path, content: &[u8]) {
    if let Ok(current) = fs::read(dst)
        && current == content
    {
        return;
    }
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(dst, content).unwrap_or_else(|e| panic!("Could not write {}: {e}", dst.display()));
}

fn assemble_runtime(src_dir: &Path) {
    let runtime_dir = src_dir.join("runtime");
    if !runtime_dir.is_dir() {
        return;
    }

    let header = r#"use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob, Screen,
    chat::ChatDraft,
    commands::OtherEntity,
    dragon::{Crystal, Dragon},
    fishing,
    lan::LanHost,
    machines::HopperState,
};
use horizon_gameplay::{
    boats::Boat,
    minecarts::Minecart,
    tnt::PrimedTnt,
};
use mod_api::{KeyAction, WorldPhase};
use crate::game::books::rreturn_books::{BookScreenSnapshot, BooksExt};
use crate::game::command_blocks::rreturn_command_blocks::{CommandBlockUiSnapshot, CommandBlocksExt};
use crate::game::dragon::rreturn_dragon::{DragonExt, FightExt};
use crate::game::end::rreturn_end::EndExt;
use crate::game::fishing::rreturn_fishing::{GameFishingExt, HookExt};
use crate::game::mount_menu::rreturn_mount_menu::{MountMenuExt, MountMenuSnapshot};
use crate::game::potions::rreturn_potions::{PotionStateExt, ThrownPotionsSnapshot};
use crate::game::recipe_book::rreturn_recipe_book::{RecipeBookExt, RecipeUiSnapshot};
use crate::game::remote::rreturn_remote::{GameRemoteContainerExt, RemoteExt};
use crate::game::sign_edit::rreturn_sign_edit::{SignEditExt, SignEditorSnapshot};
use crate::game::stations::rreturn_stations::{StationUiSnapshot, StationsExt};
use crate::game::stats::rreturn_stats::StatsExt;
use horizon_mod_runtime::attachments::Attachments;
use horizon_client::{
    advancements::Progress,
    block_entity_anim::{BlockEntityAnims, ChestLid},
    blocks::Fluid,
    boss_overlay::BossOverlay,
    bossbar::BossBars,
    chunk::{Chunk, Section},
    command_storage::CommandStorage,
    containers::BlockEntity,
    cooldowns::Cooldowns,
    effects::{EffectInstance, PlayerEffects, SavedEffect},
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    gamerules::GameRules,
    hud::HudBatch,
    items::ItemStack,
    lan::guest::SavedBody,
    lang,
    mobs::Species,
    particles::Particle,
    raid::Raids,
    recipe_book::Settings as RecipeBookSettings,
    save::{block_entity_encode, item_from_nbt, item_to_nbt, read_dat, write_dat, Level},
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    stopwatches::Stopwatches,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::{Direction, Vec3},
    nbt::{self, Limits as NbtLimits, Tag},
    position::{BlockPos, ChunkPos},
    random::{LegacyRandom, LevelRandom, RandomSource},
    ticks::ChunkTicks,
};
use horizon_world::food::Difficulty;
use horizon_protocol::{
    packets::{
        play::{self, Clientbound as C},
        recipes,
    },
    text::literal,
};
use horizon_world::{
    compression::Compression,
    ground_support::GroundState,
    level::{BlockEventData, BlockId, MovingPiston, Ticks},
    piston_shape::MovingPistonState,
    registry::BlockStateId,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicUsize, Ordering},
        mpsc, Arc, Mutex, OnceLock, RwLock,
    },
    time::Instant,
};
"#;

    let mut out = String::from(header);
    let en_json = fs::read_to_string("assets/rreturn/lang/en_us.json")
        .expect("Could not read assets/rreturn/lang/en_us.json");
    let ru_json = fs::read_to_string("assets/rreturn/lang/ru_ru.json")
        .expect("Could not read assets/rreturn/lang/ru_ru.json");
    out.push_str(&format!(
        "\npub const LANG_EN_US_JSON: &str = {:?};\npub const LANG_RU_RU_JSON: &str = {:?};\n",
        en_json, ru_json
    ));

    for file_name in RUNTIME_FILES {
        let path = runtime_dir.join(file_name);
        let content = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Could not read {}: {e}", path.display()));

        out.push('\n');

        let mut in_use_block = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("use super::")
                || trimmed.starts_with("use horizon_")
                || trimmed.starts_with("use std::")
            {
                if trimmed.ends_with(';') {
                    continue;
                } else {
                    in_use_block = true;
                    continue;
                }
            }
            if in_use_block {
                if trimmed.ends_with(';') {
                    in_use_block = false;
                }
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
    }

    let target = src_dir.join("runtime.rs");
    write_if_changed(&target, out.as_bytes());
}

fn resolve_target_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(out_dir) = std::env::var_os("OUT_DIR") {
        let out = PathBuf::from(out_dir);
        if let Some(profile_dir) = out
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            && let Some(maybe_target) = profile_dir.parent()
        {
            if let Ok(triple) = std::env::var("TARGET")
                && maybe_target.file_name().and_then(|n| n.to_str()) == Some(triple.as_str())
                && let Some(parent) = maybe_target.parent()
            {
                return parent.to_path_buf();
            }
            return maybe_target.to_path_buf();
        }
    }
    PathBuf::from("target")
}

fn run_packager(target_dir: &Path) {
    let script = Path::new("tools").join("package.py");
    let mut last_err = None;
    for bin in ["python3", "python"] {
        match Command::new(bin)
            .arg(&script)
            .arg("--skip-check")
            .arg("--quiet")
            .arg("--target-dir")
            .arg(target_dir)
            .status()
        {
            Ok(status) => {
                if !status.success() {
                    panic!("{} {} failed with {}", bin, script.display(), status);
                }
                return;
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                last_err = Some(err);
            }
            Err(err) => {
                panic!("Failed to execute {} {}: {err}", bin, script.display());
            }
        }
    }
    panic!(
        "Could not find python3 or python to run {}: {:?}",
        script.display(),
        last_err
    );
}
