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
PIN_HORIZON = "dd67b76ff23e28c6d8e21859df600d31572f81a0"
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
        # Check launcher's selected instance or global
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
            lib_text += (
                "\n\n"
                "extern crate mod_api;\n\n"
                "#[derive(Default)]\n"
                "struct Mod;\n\n"
                "#[mod_api::horizon_mod]\n"
                "impl mod_api::Hooks for Mod {}\n"
            )
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
