import argparse
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

PIN_GAME = "26.3"
PIN_HORIZON = "8f83e5c85ae3b9a117f742e8d91170c5c9080a06"
PIN_MIXINS = "e59da9094f0c2fdda6c627fe9eff22dec6a906f0"


def deploy_to_instance(
    out_path: Path, mod_id: str, mod_version: str, instance: str | None = None
) -> list[Path]:
    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    targets: list[Path] = []
    if instance:
        if instance == "horizon":
            targets.append(data_home / "horizon" / "horizon-native-mods")
        else:
            targets.append(data_home / "horizon" / "instances" / instance / "horizon-native-mods")
    else:
        # Check launcher's selected instance or default to global
        selected_dir = None
        try:
            out = subprocess.check_output(
                ["horizon-launcher", "--json", "instance", "list"],
                stderr=subprocess.DEVNULL,
                text=True,
                timeout=5,
            )
            for item in json.loads(out):
                if item.get("selected") and item.get("home"):
                    selected_dir = Path(item["home"]) / "horizon-native-mods"
                    break
        except Exception:
            pass
        if selected_dir and selected_dir.parent.is_dir():
            targets.append(selected_dir)
        else:
            targets.append(data_home / "horizon" / "horizon-native-mods")

    deployed: list[Path] = []
    for d in targets:
        d.mkdir(parents=True, exist_ok=True)
        # Clean retired formats and old versions
        for old in d.glob(f"{mod_id}*.hnmod"):
            try:
                old.unlink()
            except OSError:
                pass
        for old in d.glob(f"{mod_id}*.hmod"):
            if old.name != f"{mod_id}-{mod_version}.hmod":
                try:
                    old.unlink()
                except OSError:
                    pass
        dest = d / f"{mod_id}-{mod_version}.hmod"
        shutil.copy2(out_path, dest)
        deployed.append(dest)
    return deployed


def build_package(
    target_dir: Path,
    skip_check: bool = False,
    quiet: bool = False,
    deploy: str | None = None,
) -> dict:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    cargo_version = cargo["package"]["version"]

    manifest_path = ROOT / "horizon-mod.toml"
    if not manifest_path.is_file():
        raise FileNotFoundError(f"Missing {manifest_path}")

    manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
    mod_id = manifest["id"]
    mod_version = manifest["version"]

    if mod_version != cargo_version:
        raise ValueError(
            f"Version mismatch: Cargo.toml={cargo_version!r}, horizon-mod.toml={mod_version!r}"
        )

    native_cfg = manifest.get("native")
    if not native_cfg:
        raise ValueError("Missing [native] section in horizon-mod.toml")

    if native_cfg.get("game") != PIN_GAME:
        raise ValueError(f"Game pin mismatch: expected {PIN_GAME!r}, got {native_cfg.get('game')!r}")
    if native_cfg.get("horizon") != PIN_HORIZON:
        raise ValueError(f"Horizon pin mismatch: expected {PIN_HORIZON!r}, got {native_cfg.get('horizon')!r}")
    if native_cfg.get("mixins") != PIN_MIXINS:
        raise ValueError(f"Mixins pin mismatch: expected {PIN_MIXINS!r}, got {native_cfg.get('mixins')!r}")

    runtime_path = ROOT / "src" / "runtime.rs"
    if not skip_check and not runtime_path.is_file():
        subprocess.run(["cargo", "check"], cwd=ROOT, check=True)

    if not runtime_path.is_file():
        raise FileNotFoundError(f"Missing {runtime_path}; run cargo check first")

    files: dict[str, bytes] = {}

    # Root files
    files["horizon-mod.toml"] = manifest_path.read_bytes()
    cargo_text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    if "horizon-mod-api" not in cargo_text:
        # Launcher requires horizon-mod-api in native mod Cargo.toml
        if "[dependencies]" in cargo_text:
            cargo_text = cargo_text.replace(
                "[dependencies]",
                '[dependencies]\nhorizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }',
            )
        else:
            cargo_text += '\n[dependencies]\nhorizon-mod-api = { path = ".horizon/sdk/api/crates/horizon-mod-api" }\n'
    files["Cargo.toml"] = cargo_text.encode("utf-8")
    license_path = ROOT / "LICENSE"
    if license_path.is_file():
        files["LICENSE"] = license_path.read_bytes()
    lock_path = ROOT / "Cargo.lock"
    if lock_path.is_file():
        files["Cargo.lock"] = lock_path.read_bytes()
    icon_path = ROOT / "icon.png"
    if icon_path.is_file():
        files["icon.png"] = icon_path.read_bytes()

    # Source files under src/
    src_dir = ROOT / "src"
    for path in sorted(src_dir.rglob("*")):
        if not path.is_file():
            continue
        if any(part in (".git", "target", "__pycache__") for part in path.parts):
            continue
        rel = path.relative_to(ROOT).as_posix()
        if not (rel.endswith(".rs") or rel.endswith(".toml")):
            continue
        files[rel] = path.read_bytes()

    # Assets & data
    for dname in ("assets", "data"):
        dpath = ROOT / dname
        if dpath.is_dir():
            for path in sorted(dpath.rglob("*")):
                if not path.is_file():
                    continue
                if any(part in (".git", "target", "__pycache__") for part in path.parts):
                    continue
                rel = path.relative_to(ROOT).as_posix()
                files[rel] = path.read_bytes()

    if "src/lib.rs" in files:
        lib_text = files["src/lib.rs"].decode("utf-8")
        if "pub fn hooks" not in lib_text:
            lib_text += """

extern crate mod_api;

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl mod_api::Hooks for Mod {
    fn register(&self, r: &mut dyn mod_api::Registrar, _: &mod_api::RegistrationContext) {
        use mod_api::{KeyConflictContext, KeyRegistration, SettingSpec};

        r.register_key_mapping(
            KeyRegistration::new("scrub", "key.keyboard.u")
                .label("key.rreturn.scrub")
                .category("key.category.rreturn")
                .context(KeyConflictContext::InGame),
        );
        r.register_key_mapping(
            KeyRegistration::new("save", "key.keyboard.f6")
                .label("key.rreturn.save")
                .category("key.category.rreturn")
                .context(KeyConflictContext::InGame),
        );
        r.register_key_mapping(
            KeyRegistration::new("load", "key.keyboard.f7")
                .label("key.rreturn.load")
                .category("key.category.rreturn")
                .context(KeyConflictContext::InGame),
        );
        r.register_key_mapping(
            KeyRegistration::new("anchor_menu", "key.keyboard.f8")
                .label("key.rreturn.anchor_menu")
                .category("key.category.rreturn")
                .context(KeyConflictContext::InGame),
        );

        let _ = r.register_setting(
            SettingSpec::toggle("auto_save", "rreturn.setting.auto_save", true)
                .tooltip("rreturn.setting.auto_save.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::toggle("adaptive_mode", "rreturn.setting.adaptive_mode", true)
                .tooltip("rreturn.setting.adaptive_mode.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::range(
                "fixed_interval_sec",
                "rreturn.setting.fixed_interval_sec",
                5.0,
                600.0,
                5.0,
                300.0,
            )
            .tooltip("rreturn.setting.fixed_interval_sec.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::choice(
                "anchor_mode",
                "rreturn.setting.anchor_mode",
                [
                    "config.rreturn.anchorMode.fixed",
                    "config.rreturn.anchorMode.multi",
                    "config.rreturn.anchorMode.everyone",
                    "config.rreturn.anchorMode.rotating",
                ],
                0,
            )
            .tooltip("rreturn.setting.anchor_mode.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::range(
                "rollback_cooldown_sec",
                "rreturn.setting.rollback_cooldown_sec",
                0.0,
                60.0,
                1.0,
                0.0,
            )
            .tooltip("rreturn.setting.rollback_cooldown_sec.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::toggle(
                "save_on_boss_defeat",
                "rreturn.setting.save_on_boss_defeat",
                true,
            )
            .tooltip("rreturn.setting.save_on_boss_defeat.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::toggle(
                "f3_loop_hitboxes",
                "rreturn.setting.f3_loop_hitboxes",
                true,
            )
            .tooltip("rreturn.setting.f3_loop_hitboxes.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::toggle("logs", "rreturn.setting.logs", false)
                .tooltip("rreturn.setting.logs.tooltip"),
        );
        let _ = r.register_setting(
            SettingSpec::toggle("dev_mode", "rreturn.setting.dev_mode", false)
                .tooltip("rreturn.setting.dev_mode.tooltip"),
        );
    }
}
"""
            files["src/lib.rs"] = lib_text.encode("utf-8")

    # Validate against forbidden paths
    for name in files:
        first = name.split("/")[0]
        if first in ("horizon-native.json", "horizon-mixin.toml", "mixins", "native", "rust"):
            raise ValueError(f"Disallowed path inside package: {name}")

    # Validate required modules and mixin helpers
    for mod in native_cfg.get("module", []):
        src_file = mod.get("source")
        if src_file not in files:
            raise FileNotFoundError(f"Missing support module source: {src_file}")

    for mixin in native_cfg.get("mixin", []):
        helper_file = mixin.get("helper")
        if helper_file not in files:
            raise FileNotFoundError(f"Missing mixin helper: {helper_file}")

    # Build deterministic flat .hmod archive
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            info.create_system = 3
            archive.writestr(info, content)

    zip_bytes = buf.getvalue()
    target_dir.mkdir(parents=True, exist_ok=True)
    out_path = target_dir / f"{mod_id}-{mod_version}.hmod"
    if not out_path.is_file() or out_path.read_bytes() != zip_bytes:
        tmp_path = target_dir / f".{mod_id}-{mod_version}.hmod.tmp"
        tmp_path.write_bytes(zip_bytes)
        os.replace(tmp_path, out_path)

    deployed_paths: list[str] = []
    if deploy is not None:
        target_instance = deploy if deploy else None
        deployed = deploy_to_instance(out_path, mod_id, mod_version, target_instance)
        deployed_paths = [str(p) for p in deployed]

    result = {
        "file": str(out_path),
        "size": len(zip_bytes),
        "sha256": hashlib.sha256(zip_bytes).hexdigest(),
    }
    if deployed_paths:
        result["deployed"] = deployed_paths
    if not quiet:
        print(json.dumps(result, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description="Package RReturn into a deterministic .hmod bundle")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    parser.add_argument("--skip-check", action="store_true")
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument(
        "--deploy",
        nargs="?",
        const="",
        metavar="INSTANCE",
        help="Deploy packaged .hmod to instance horizon-native-mods (default: active instance or global)",
    )
    args = parser.parse_args()
    build_package(
        args.target_dir,
        skip_check=args.skip_check,
        quiet=args.quiet,
        deploy=args.deploy,
    )


if __name__ == "__main__":
    main()
