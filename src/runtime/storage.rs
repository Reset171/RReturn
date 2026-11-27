use super::types::*;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

pub fn global_settings_path() -> PathBuf {
    Path::new("config").join("rreturn.json")
}

pub fn world_settings_path(world_root: &Path) -> PathBuf {
    world_root.join("rreturn").join("settings.json")
}

pub fn slot_dir(world_root: &Path, slot: &str) -> PathBuf {
    let clean = slot.trim();
    if clean.is_empty() || clean == "default" || !is_valid_slot_name(clean) {
        world_root.join("rreturn").join("checkpoint")
    } else {
        world_root.join("rreturn").join("slots").join(clean)
    }
}

pub fn checkpoint_dir(world_root: &Path) -> PathBuf {
    slot_dir(world_root, "default")
}

pub fn has_disk_checkpoint(world_root: &Path) -> bool {
    has_disk_slot(world_root, "default")
}

pub fn has_disk_slot(world_root: &Path, slot: &str) -> bool {
    slot_dir(world_root, slot).join("meta.json").is_file()
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> usize {
    copy_dir_recursive_opts(src, dst, false)
}

pub fn copy_dir_recursive_opts(src: &Path, dst: &Path, force: bool) -> usize {
    let mut mca_ignored = false;
    copy_dir_recursive_track_mca(src, dst, force, &mut mca_ignored)
}

fn copy_dir_recursive_track_mca(
    src: &Path,
    dst: &Path,
    force: bool,
    mca_changed: &mut bool,
) -> usize {
    if !src.is_dir() {
        return 0;
    }
    let mut copied = 0;
    let _ = fs::create_dir_all(dst);
    if let Ok(entries) = fs::read_dir(src) {
        for entry in entries.flatten() {
            let p = entry.path();
            let target = dst.join(entry.file_name());
            if p.is_dir() {
                copied += copy_dir_recursive_track_mca(&p, &target, force, mca_changed);
            } else if p.is_file() {
                if !force {
                    if let (Ok(m_src), Ok(m_dst)) = (p.metadata(), target.metadata()) {
                        if m_src.len() == m_dst.len() {
                            if let (Ok(t_src), Ok(t_dst)) = (m_src.modified(), m_dst.modified()) {
                                if t_src == t_dst {
                                    continue;
                                }
                            }
                        }
                    }
                }
                if fs::copy(&p, &target).is_ok() {
                    copied += 1;
                    if p.extension().and_then(|e| e.to_str()) == Some("mca") {
                        *mca_changed = true;
                    }
                    if let (Ok(m), Ok(f)) =
                        (p.metadata(), fs::File::options().write(true).open(&target))
                    {
                        if let Ok(mtime) = m.modified() {
                            let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
                        }
                    }
                }
            }
        }
    }
    copied
}

pub fn collect_files_recursive(
    dir: &Path,
    map: &mut HashMap<PathBuf, Vec<u8>>,
    target_base: &Path,
    rel_prefix: &Path,
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            let fname = entry.file_name();
            let next_rel = rel_prefix.join(fname);
            if p.is_dir() {
                collect_files_recursive(&p, map, target_base, &next_rel);
            } else if p.is_file() {
                if let Ok(bytes) = fs::read(&p) {
                    map.insert(target_base.join(&next_rel), bytes);
                }
            }
        }
    }
}

pub fn try_copy_file_preserve_mtime(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    if let (Ok(m), Ok(f)) = (src.metadata(), fs::File::options().write(true).open(dst)) {
        if let Ok(mtime) = m.modified() {
            let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
        }
    }
    Ok(())
}

pub fn try_copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<usize> {
    if !src.exists() {
        return Ok(0);
    }
    let mut copied = 0;
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let p = entry.path();
        let target = dst.join(entry.file_name());
        if p.is_dir() {
            copied += try_copy_dir_recursive(&p, &target)?;
        } else if p.is_file() {
            try_copy_file_preserve_mtime(&p, &target)?;
            copied += 1;
        }
    }
    Ok(copied)
}

pub fn prune_extra_dat_json_recursive(src: &Path, dst: &Path) -> usize {
    if !dst.is_dir() {
        return 0;
    }
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(dst) {
        for entry in entries.flatten() {
            let p = entry.path();
            let src_entry = src.join(entry.file_name());
            if p.is_dir() {
                removed += prune_extra_dat_json_recursive(&src_entry, &p);
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if (ext == "dat" || ext == "json")
                    && !src_entry.exists()
                    && fs::remove_file(&p).is_ok()
                {
                    removed += 1;
                }
            }
        }
    }
    removed
}

pub fn prune_extra_nbt_recursive(src: &Path, dst: &Path) -> usize {
    if !dst.is_dir() {
        return 0;
    }
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(dst) {
        for entry in entries.flatten() {
            let p = entry.path();
            let src_entry = src.join(entry.file_name());
            if p.is_dir() {
                removed += prune_extra_nbt_recursive(&src_entry, &p);
            } else if p.is_file() {
                let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "nbt" && !src_entry.exists() && fs::remove_file(&p).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    removed
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DiskRestoreReport {
    pub changed: usize,
    pub mca_changed: bool,
}

pub fn restore_disk_files_for_slot(world_root: &Path, slot: &str) -> usize {
    restore_disk_files_for_slot_opts(world_root, slot, false)
}

pub fn restore_disk_files_for_slot_opts(world_root: &Path, slot: &str, force: bool) -> usize {
    restore_disk_files_for_slot_report(world_root, slot, force).changed
}

pub fn restore_disk_files_for_slot_report(
    world_root: &Path,
    slot: &str,
    force: bool,
) -> DiskRestoreReport {
    let mut rep = DiskRestoreReport::default();
    if !is_valid_slot_name(slot.trim()) {
        return rep;
    }
    let cp_dir = slot_dir(world_root, slot);
    if !cp_dir.join("meta.json").is_file() {
        return rep;
    }
    let cp_level = cp_dir.join("level.dat");
    let target_level = world_root.join("level.dat");
    if cp_level.is_file() {
        let mut skip = false;
        if !force {
            if let (Ok(m_src), Ok(m_dst)) = (cp_level.metadata(), target_level.metadata()) {
                if m_src.len() == m_dst.len() {
                    if let (Ok(t_src), Ok(t_dst)) = (m_src.modified(), m_dst.modified()) {
                        if t_src == t_dst {
                            skip = true;
                        }
                    }
                }
            }
        }
        if !skip && fs::copy(&cp_level, &target_level).is_ok() {
            rep.changed += 1;
            if let (Ok(m), Ok(f)) = (
                cp_level.metadata(),
                fs::File::options().write(true).open(&target_level),
            ) {
                if let Ok(mtime) = m.modified() {
                    let _ = f.set_times(std::fs::FileTimes::new().set_modified(mtime));
                }
            }
        }
    }
    let cp_players = cp_dir.join("players");
    let target_players = world_root.join("players");
    rep.changed += prune_extra_dat_json_recursive(&cp_players, &target_players);
    if cp_players.is_dir() {
        rep.changed += copy_dir_recursive_track_mca(
            &cp_players,
            &target_players,
            force,
            &mut rep.mca_changed,
        );
    }
    let cp_data = cp_dir.join("data");
    let target_data = world_root.join("data");
    rep.changed += prune_extra_dat_json_recursive(&cp_data, &target_data);
    if cp_data.is_dir() {
        rep.changed +=
            copy_dir_recursive_track_mca(&cp_data, &target_data, force, &mut rep.mca_changed);
    }
    let cp_dims = cp_dir.join("dimensions");
    let target_dims = world_root.join("dimensions");
    rep.changed += prune_extra_dat_json_recursive(&cp_dims, &target_dims);
    if cp_dims.is_dir() {
        rep.changed +=
            copy_dir_recursive_track_mca(&cp_dims, &target_dims, force, &mut rep.mca_changed);
    }
    let cp_generated = cp_dir.join("generated");
    let target_generated = world_root.join("generated");
    rep.changed += prune_extra_nbt_recursive(&cp_generated, &target_generated);
    if cp_generated.is_dir() {
        rep.changed += copy_dir_recursive_track_mca(
            &cp_generated,
            &target_generated,
            force,
            &mut rep.mca_changed,
        );
    }
    rep
}

pub fn restore_disk_files_on_rollback(world_root: &Path) {
    restore_disk_files_for_slot(world_root, "default");
}

pub fn list_disk_slots(world_root: &Path) -> Vec<serde_json::Value> {
    let mut list = Vec::new();
    let def_meta = checkpoint_dir(world_root).join("meta.json");
    if let Ok(s) = fs::read_to_string(&def_meta) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
            list.push(serde_json::json!({
                "slot": "default",
                "game_time": val.get("game_time"),
                "saved_at_unix": val.get("saved_at_unix"),
                "guests_count": val.get("guests").and_then(|g| g.as_array()).map(|a| a.len()).unwrap_or(0)
            }));
        }
    }

    let slots_base = world_root.join("rreturn").join("slots");
    if let Ok(entries) = fs::read_dir(slots_base) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !is_valid_slot_name(&name) {
                    continue;
                }
                let meta_f = p.join("meta.json");
                if let Ok(s) = fs::read_to_string(&meta_f) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&s) {
                        list.push(serde_json::json!({
                            "slot": name,
                            "game_time": val.get("game_time"),
                            "saved_at_unix": val.get("saved_at_unix"),
                            "guests_count": val.get("guests").and_then(|g| g.as_array()).map(|a| a.len()).unwrap_or(0)
                        }));
                    }
                }
            }
        }
    }

    list
}

pub fn delete_disk_slot(world_root: &Path, slot: &str) -> bool {
    let clean = slot.trim();
    if clean == "default" || !is_valid_slot_name(clean) {
        return false;
    }
    let dir = world_root.join("rreturn").join("slots").join(clean);
    if dir.exists() {
        fs::remove_dir_all(dir).is_ok()
    } else {
        false
    }
}

pub fn format_uuid_std(u: u128) -> String {
    let hex = format!("{:032x}", u);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}