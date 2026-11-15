use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::Command;

const RUNTIME_FILES: &[&str] = &[
    "types.rs",
    "policy.rs",
    "snapshots.rs",
    "benchmark.rs",
    "adaptive.rs",
    "capture.rs",
    "storage.rs",
    "persistence.rs",
    "load_disk.rs",
    "hydration.rs",
    "restore_level.rs",
    "restore_players.rs",
    "restore_client.rs",
    "restore.rs",
    "telemetry.rs",
    "commands.rs",
    "ui.rs",
    "tracker.rs",
    "hooks.rs",
];

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=horizon-native.json");
    println!("cargo:rerun-if-changed=LICENSE");
    println!("cargo:rerun-if-changed=src/horizon-mixin.toml");
    println!("cargo:rerun-if-changed=src/runtime");
    println!("cargo:rerun-if-changed=src/ext");
    println!("cargo:rerun-if-changed=src/hooks");
    println!("cargo:rerun-if-changed=assets/rreturn/lang/en_us.json");
    println!("cargo:rerun-if-changed=assets/rreturn/lang/ru_ru.json");
    println!("cargo:rerun-if-changed=tools/package.py");
    for file_name in RUNTIME_FILES {
        println!("cargo:rerun-if-changed=src/runtime/{file_name}");
    }

    let bundle_dir = Path::new("mixins/rreturn");
    fs::create_dir_all(bundle_dir)
        .unwrap_or_else(|e| panic!("Could not create {}: {e}", bundle_dir.display()));

    let mut expected = HashSet::from([
        "runtime.rs".to_owned(),
        "horizon-mixin.toml".to_owned(),
        "LICENSE".to_owned(),
    ]);

    sync_dir_files(Path::new("src/ext"), bundle_dir, &mut expected);
    sync_dir_files(Path::new("src/hooks"), bundle_dir, &mut expected);
    sync_file(
        Path::new("src/horizon-mixin.toml"),
        &bundle_dir.join("horizon-mixin.toml"),
    );
    sync_file(Path::new("LICENSE"), &bundle_dir.join("LICENSE"));
    assemble_runtime(bundle_dir);
    prune_extra_bundle_files(bundle_dir, &expected);

    let target_dir = resolve_target_dir();
    run_packager(&target_dir);

    println!("cargo:rerun-if-changed=mixins/rreturn/horizon-mixin.toml");
    println!("cargo:rerun-if-changed=mixins/rreturn/LICENSE");
    println!("cargo:rerun-if-changed=mixins/rreturn/runtime.rs");
    let hnmod_path = target_dir.join(format!("rreturn-{}.hnmod", env!("CARGO_PKG_VERSION")));
    println!("cargo:rerun-if-changed={}", hnmod_path.display());
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

fn sync_file(src: &Path, dst: &Path) {
    let content =
        fs::read(src).unwrap_or_else(|e| panic!("Could not read {}: {e}", src.display()));
    write_if_changed(dst, &content);
}

fn sync_dir_files(src_dir: &Path, dst_dir: &Path, expected: &mut HashSet<String>) {
    if !src_dir.is_dir() {
        return;
    }
    let _ = fs::create_dir_all(dst_dir);
    let Ok(entries) = fs::read_dir(src_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        if let Some(name) = entry.file_name().to_str() {
            expected.insert(name.to_owned());
        }
        let target = dst_dir.join(entry.file_name());
        sync_file(&path, &target);
    }
}

fn prune_extra_bundle_files(bundle_dir: &Path, expected: &HashSet<String>) {
    let Ok(entries) = fs::read_dir(bundle_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let keep = entry
            .file_name()
            .to_str()
            .is_some_and(|name| expected.contains(name));
        if !keep {
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

fn assemble_runtime(bundle_dir: &Path) {
    let runtime_dir = Path::new("src/runtime");
    if !runtime_dir.is_dir() {
        return;
    }

    let header = r#"use super::super::{
    Dim, ExperienceOrb, FallingBlock, Game, ItemEntity, Mob,
    boats::Boat,
    commands::OtherEntity,
    dragon::{Crystal, Dragon},
    lan::LanHost,
    machines::HopperState,
    minecarts::Minecart,
    native_hooks::NativeMod,
    native_lifecycle::WorldPhase,
    tnt::PrimedTnt,
};
use horizon_client::{
    advancements::Progress,
    blocks::Fluid,
    bossbar::BossBars,
    chunk::Chunk,
    containers::BlockEntity,
    cooldowns::Cooldowns,
    effects::{EffectInstance, PlayerEffects, SavedEffect},
    entities::{ArmorStand, Cushion, ItemFrame, Painting},
    fast_hash::FxHashMap,
    gamerules::GameRules,
    items::ItemStack,
    lan::guest::SavedBody,
    lang,
    mobs::Species,
    recipe_book::Settings as RecipeBookSettings,
    save::{item_from_nbt, read_dat, write_dat, Level},
    scoreboard::Scoreboard,
    sculk::WardenSpawnTracker,
    world::{HealthRules, Weather},
    worldborder::WorldBorder,
};
use horizon_core::{
    geometry::{Direction, Vec3},
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
    attachments::Attachments,
    ground_support::GroundState,
    level::{BlockEventData, BlockId, MovingPiston, Ticks},
    nbt::Tag,
    registry::BlockStateId,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, RwLock,
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

    let target = bundle_dir.join("runtime.rs");
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