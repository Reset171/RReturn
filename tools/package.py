import argparse
import hashlib
import io
import json
import os
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def build_package(target_dir: Path, skip_check: bool = False, quiet: bool = False) -> dict:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    cargo_version = cargo["package"]["version"]
    native_path = ROOT / "horizon-native.json"
    native_manifest = json.loads(native_path.read_text(encoding="utf-8"))
    mod_id = native_manifest["id"]
    mod_version = native_manifest["version"]

    bundle_dir = ROOT / "mixins" / mod_id
    mixin_toml_path = bundle_dir / "horizon-mixin.toml"
    if not skip_check and (not mixin_toml_path.is_file() or not (bundle_dir / "runtime.rs").is_file()):
        subprocess.run(["cargo", "check"], cwd=ROOT, check=True)

    if not mixin_toml_path.is_file():
        raise FileNotFoundError(f"Missing {mixin_toml_path}; run cargo check first")

    mixin_manifest = tomllib.loads(mixin_toml_path.read_text(encoding="utf-8"))
    mixin_id = mixin_manifest["mod"]["id"]
    mixin_version = mixin_manifest["mod"]["version"]
    mixin_rev = mixin_manifest.get("compatibility", {}).get("horizon_revision")

    if mixin_id != mod_id:
        raise ValueError(
            f"Mod id mismatch: horizon-native.json has {mod_id!r}, horizon-mixin.toml has {mixin_id!r}"
        )
    if mod_version != cargo_version or mixin_version != cargo_version:
        raise ValueError(
            f"Version mismatch: Cargo.toml={cargo_version!r}, horizon-native.json={mod_version!r}, horizon-mixin.toml={mixin_version!r}"
        )
    if native_manifest.get("horizon") != mixin_rev:
        raise ValueError(
            f"Horizon revision mismatch: horizon-native.json={native_manifest.get('horizon')!r}, horizon-mixin.toml={mixin_rev!r}"
        )

    files = {"horizon-native.json": native_path.read_bytes()}
    for path in sorted(bundle_dir.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(bundle_dir).as_posix()
        if path.name != "LICENSE" and path.suffix not in (".rs", ".toml"):
            raise ValueError(f"Unexpected file inside mixins/{mod_id}/: {rel}")
        files[f"mixins/{mod_id}/{rel}"] = path.read_bytes()

    for required in (
        f"mixins/{mod_id}/horizon-mixin.toml",
        f"mixins/{mod_id}/LICENSE",
        f"mixins/{mod_id}/runtime.rs",
    ):
        if required not in files:
            raise FileNotFoundError(f"Missing required bundle entry: {required}")

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
    out_path = target_dir / f"{mod_id}-{mod_version}.hnmod"
    if not out_path.is_file() or out_path.read_bytes() != zip_bytes:
        tmp_path = target_dir / f".{mod_id}-{mod_version}.hnmod.tmp"
        tmp_path.write_bytes(zip_bytes)
        os.replace(tmp_path, out_path)

    result = {
        "file": str(out_path),
        "size": len(zip_bytes),
        "sha256": hashlib.sha256(zip_bytes).hexdigest(),
    }
    if not quiet:
        print(json.dumps(result, indent=2))
    return result


def main():
    parser = argparse.ArgumentParser(description="Package RReturn into a deterministic .hnmod bundle")
    parser.add_argument("--target-dir", type=Path, default=ROOT / "target")
    parser.add_argument("--skip-check", action="store_true")
    parser.add_argument("--quiet", action="store_true")
    args = parser.parse_args()
    build_package(args.target_dir, skip_check=args.skip_check, quiet=args.quiet)


if __name__ == "__main__":
    main()