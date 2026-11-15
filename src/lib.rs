use std::{
    fs,
    io::Write,
    path::Path,
};

pub const MOD_ID: &str = "rreturn";
pub const MOD_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn is_valid_slot_name(name: &str) -> bool {
    let len = name.len();
    (1..=32).contains(&len)
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

pub fn atomic_write_json(target_dir: &Path, filename: &str, content: &[u8]) -> std::io::Result<()> {
    let Some(parent) = target_dir.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "missing parent directory",
        ));
    };
    fs::create_dir_all(parent)?;
    let dir_name = target_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("checkpoint");
    let staging = parent.join(format!(".{dir_name}.staging"));
    let backup = parent.join(format!(".{dir_name}.bak"));

    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }

    let write_res = (|| -> std::io::Result<()> {
        fs::create_dir_all(&staging)?;
        let mut f = fs::File::create(staging.join(filename))?;
        f.write_all(content)?;
        f.sync_all()?;
        Ok(())
    })();

    if let Err(err) = write_res {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }

    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }

    let had_existing = target_dir.exists();
    if had_existing && let Err(err) = fs::rename(target_dir, &backup) {
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }

    if let Err(err) = fs::rename(&staging, target_dir) {
        if had_existing {
            let _ = fs::rename(&backup, target_dir);
        }
        let _ = fs::remove_dir_all(&staging);
        return Err(err);
    }

    if had_existing {
        let _ = fs::remove_dir_all(&backup);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_name_validation_rejects_traversal_and_invalid_chars() {
        assert!(is_valid_slot_name("default"));
        assert!(is_valid_slot_name("slot_alpha-1"));
        assert!(!is_valid_slot_name(""));
        assert!(!is_valid_slot_name("../outside"));
        assert!(!is_valid_slot_name("../../.."));
        assert!(!is_valid_slot_name("a/b"));
        assert!(!is_valid_slot_name("a\\b"));
        assert!(!is_valid_slot_name(&"a".repeat(33)));
    }

    #[test]
    fn atomic_checkpoint_write_preserves_known_good_on_io_failure() {
        let tmp = std::env::temp_dir().join(format!("rreturn_atomic_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let slot_dir = tmp.join("checkpoint");
        assert!(atomic_write_json(&slot_dir, "meta.json", br#"{"game_time":100}"#).is_ok());

        let blocker = tmp.join(".checkpoint.staging").join("nested");
        fs::create_dir_all(&blocker).unwrap();
        fs::write(blocker.join("file"), b"x").unwrap();

        let saved = fs::read_to_string(slot_dir.join("meta.json")).unwrap();
        assert_eq!(saved, r#"{"game_time":100}"#);

        let bad_target = tmp.join("unwritable_parent_file").join("slot");
        fs::write(tmp.join("unwritable_parent_file"), b"not_a_dir").unwrap();
        assert!(atomic_write_json(&bad_target, "meta.json", br#"{"game_time":200}"#).is_err());

        let still_good = fs::read_to_string(slot_dir.join("meta.json")).unwrap();
        assert_eq!(still_good, r#"{"game_time":100}"#);

        let _ = fs::remove_dir_all(&tmp);
    }
}