import argparse
import json
import os
import py_compile
import re
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

EXTERNAL_HORIZON_TYPES = (
    "Game",
    "Mob",
    "LanHost",
    "Remote",
    "Dragon",
    "Fight",
    "Stats",
    "Hook",
    "EndState",
    "PotionState",
)

FORBIDDEN_PUBLIC_STRINGS = (
    "/home/reset171",
    ".gemini/antigravity",
    "horizon-2",
    "OneUI",
    "One UI",
    "Samsung",
    "Баги 2.md",
)

FORBIDDEN_RUNTIME_DEBUG = (
    "eprintln!(",
    "audit_rng",
    "DET_TRACE",
)


def fail(msg):
    print(f"[FAIL] {msg}", file=sys.stderr)
    sys.exit(1)


def run_step(label, cmd):
    print(f"==> {label}: {' '.join(cmd)}", flush=True)
    proc = subprocess.run(cmd, cwd=ROOT)
    if proc.returncode != 0:
        fail(f"{label} failed with exit code {proc.returncode}")


def check_python_syntax():
    py_files = sorted(ROOT.glob("tools/**/*.py"))
    print(f"==> Python syntax check ({len(py_files)} files)", flush=True)
    for path in py_files:
        py_compile.compile(str(path), doraise=True)


def check_runtime_assembly():
    print("==> Pre-flight: runtime assembly, inline super::, monoliths", flush=True)
    build_rs = (ROOT / "build.rs").read_text(encoding="utf-8")
    mod_rs = (ROOT / "src" / "runtime" / "mod.rs").read_text(encoding="utf-8")

    m = re.search(r"const\s+RUNTIME_FILES\s*:\s*&\[&str\]\s*=\s*&\[(.*?)\];", build_rs, re.S)
    if not m:
        fail("Could not parse RUNTIME_FILES in build.rs")
    build_files = re.findall(r'"([^"]+\.rs)"', m.group(1))
    mod_files = [f"{name}.rs" for name in re.findall(r"^\s*pub\s+mod\s+([a-zA-Z0-9_]+)\s*;", mod_rs, re.M)]
    disk_files = sorted(
        p.name for p in (ROOT / "src" / "runtime").glob("*.rs") if p.name != "mod.rs"
    )

    if set(build_files) != set(disk_files):
        missing = sorted(set(disk_files) - set(build_files))
        extra = sorted(set(build_files) - set(disk_files))
        fail(f"build.rs RUNTIME_FILES drift: missing={missing}, extra={extra}")
    if set(mod_files) != set(disk_files):
        missing = sorted(set(disk_files) - set(mod_files))
        extra = sorted(set(mod_files) - set(disk_files))
        fail(f"src/runtime/mod.rs drift: missing={missing}, extra={extra}")

    inline_super_re = re.compile(r"(?<!::)\bsuper::(?!super::)[a-z_][a-z0-9_]*::")
    for fname in disk_files:
        path = ROOT / "src" / "runtime" / fname
        lines = path.read_text(encoding="utf-8").splitlines()
        if len(lines) > 950:
            print(f"[WARN] Large runtime module: src/runtime/{fname} ({len(lines)} lines)", flush=True)
        in_use = False
        for lineno, line in enumerate(lines, 1):
            stripped = line.strip()
            if stripped.startswith("use "):
                if not stripped.endswith(";"):
                    in_use = True
                continue
            if in_use:
                if stripped.endswith(";"):
                    in_use = False
                continue
            if inline_super_re.search(line):
                fail(f"Inline sibling super:: call in src/runtime/{fname}:{lineno}: {stripped}")


def check_mixins_and_exts():
    print("==> Pre-flight: horizon-mod.toml mixins, bindings, extension traits", flush=True)
    manifest = tomllib.loads((ROOT / "horizon-mod.toml").read_text(encoding="utf-8"))
    native_cfg = manifest.get("native", {})

    for mod in native_cfg.get("module", []):
        target_file = mod.get("file", "")
        if not target_file.startswith("crates/horizon-client/src/game/"):
            fail(f"Disallowed [[native.module]] destination '{target_file}' in id={mod.get('id')}")

    field_proj_re = re.compile(r"&\s*(?:mut\s+)?\*?\s*self\.[A-Za-z_]")
    mixins = native_cfg.get("mixin", [])
    for mx in mixins:
        mid = mx.get("id", "<unnamed>")
        kind = mx.get("kind", "")
        bindings = mx.get("bindings", [])
        val_idx = None
        self_mut_idx = None
        for idx, b in enumerate(bindings):
            bval = b.get("value", "")
            bty = b.get("ty", "")
            if field_proj_re.search(bval):
                fail(f"Mixin '{mid}' binding '{b.get('name')}' projects field from self: {bval}")
            if bval == "$value":
                val_idx = idx
            if bval == "&mut *self":
                self_mut_idx = idx
            if kind == "WRAP_OPERATION" and bval == "$operation":
                if "->" not in bty:
                    fail(f"Mixin '{mid}' WRAP_OPERATION $operation missing explicit return type: {bty}")
                if ",)" in bty.replace(" ", ""):
                    fail(f"Mixin '{mid}' WRAP_OPERATION $operation has trailing comma: {bty}")
        if kind == "RETURN" and val_idx is not None and self_mut_idx is not None:
            if val_idx > self_mut_idx:
                fail(f"Mixin '{mid}' RETURN binds $value after &mut *self (causes E0499/E0503)")

    inherent_impl_re = re.compile(
        r"^\s*impl\b(?![^{]*\bfor\b)\s+(?:" + "|".join(EXTERNAL_HORIZON_TYPES) + r")\b",
        re.M,
    )
    for ext_file in sorted((ROOT / "src" / "ext").glob("*.rs")):
        text = ext_file.read_text(encoding="utf-8")
        if inherent_impl_re.search(text):
            fail(f"Inherent impl on external Horizon type in {ext_file.relative_to(ROOT)} (use Extension Trait)")

    data_home = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local/share"))
    funcs_path = data_home / "horizon-launcher" / "native" / "prebuilt-package" / "functions.json"
    if funcs_path.is_file():
        try:
            data = json.loads(funcs_path.read_text(encoding="utf-8"))
            funcs_raw = data.get("functions")
            if isinstance(funcs_raw, dict):
                known_targets = set(funcs_raw.keys())
            elif isinstance(funcs_raw, list):
                known_targets = {
                    item["path"] for item in funcs_raw if isinstance(item, dict) and "path" in item
                }
            else:
                known_targets = set()
            refused_raw = data.get("refused")
            refused_map = refused_raw if isinstance(refused_raw, dict) else {}
            anchor_sym = "horizon_client::game::native_hooks::base::Game<'_>::native_command"
            if known_targets and anchor_sym in known_targets:
                missing_targets = [
                    f"{mx.get('id')}: {mx.get('target')}"
                    for mx in mixins
                    if mx.get("target") not in known_targets
                    and (mx.get("kind", "HEAD") == "HEAD" or mx.get("target") not in refused_map)
                ]
                if missing_targets:
                    fail(
                        "Mixin targets missing from local prebuilt functions.json:\n  "
                        + "\n  ".join(missing_targets)
                    )
                refused_targets = [
                    f"{mx.get('id')}: {mx.get('target')} ({refused_map[mx.get('target')].get('code', '?')})"
                    for mx in mixins
                    if mx.get("kind", "HEAD") != "HEAD" and mx.get("target") in refused_map
                ]
                if refused_targets:
                    fail(
                        "Body-rewrite mixin targets refused by prebuilt functions.json:\n  "
                        + "\n  ".join(refused_targets)
                    )
        except Exception as err:
            print(f"[WARN] Could not inspect local functions.json: {err}", flush=True)


def check_lang_parity():
    print("==> Pre-flight: localization key parity", flush=True)
    en = json.loads((ROOT / "assets" / "rreturn" / "lang" / "en_us.json").read_text(encoding="utf-8"))
    ru = json.loads((ROOT / "assets" / "rreturn" / "lang" / "ru_ru.json").read_text(encoding="utf-8"))
    en_only = sorted(set(en) - set(ru))
    ru_only = sorted(set(ru) - set(en))
    if en_only or ru_only:
        fail(f"Localization key mismatch: en_only={en_only}, ru_only={ru_only}")


def check_repo_hygiene():
    print("==> Pre-flight: privacy and debug hygiene scan", flush=True)
    scan_files = []
    for pattern in ("src/**/*.rs", "tools/**/*.py", "assets/**/*.json"):
        for p in sorted(ROOT.glob(pattern)):
            if p.name == "runtime.rs" or p == ROOT / "tools" / "pr_ready.py":
                continue
            scan_files.append(p)
    for name in ("horizon-mod.toml", "Cargo.toml", "AGENTS.md", "README.md", "README.ru.md"):
        p = ROOT / name
        if p.is_file():
            scan_files.append(p)

    for path in scan_files:
        text = path.read_text(encoding="utf-8", errors="replace")
        rel = path.relative_to(ROOT).as_posix()
        for bad in FORBIDDEN_PUBLIC_STRINGS:
            if bad in text:
                fail(f"Forbidden private/brand string {bad!r} found in {rel}")
        if rel.startswith("src/runtime/"):
            for dbg in FORBIDDEN_RUNTIME_DEBUG:
                if dbg in text:
                    fail(f"Forbidden runtime debug artifact {dbg!r} found in {rel}")


def check_bundle_and_package():
    print("==> Checking generated src/runtime.rs and .hmod package", flush=True)
    run_step("cargo check", ["cargo", "check"])
    proc = subprocess.run(
        [sys.executable, "tools/package.py", "--skip-check"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    info = json.loads(proc.stdout)
    hmod_path = Path(info["file"])
    if not hmod_path.is_file() or not hmod_path.name.endswith(".hmod"):
        fail(f"Missing or invalid package output: {hmod_path}")
    with zipfile.ZipFile(hmod_path, "r") as zf:
        names = set(zf.namelist())
        tomllib.loads(zf.read("horizon-mod.toml").decode("utf-8"))

    required = {
        "horizon-mod.toml",
        "Cargo.toml",
        "LICENSE",
        "src/lib.rs",
        "src/runtime.rs",
    }
    missing = sorted(required - names)
    if missing:
        fail(f"Incomplete .hmod archive, missing: {missing}")

    forbidden_roots = {"horizon-native.json", "horizon-mixin.toml", "mixins", "native", "rust"}
    invalid = sorted(
        n
        for n in names
        if n.split("/")[0] in forbidden_roots
        or (n.startswith("src/") and not n.endswith((".rs", ".toml")))
    )
    if invalid:
        fail(f"Disallowed files in .hmod archive: {invalid}")
    print(
        f"    Packaged {hmod_path.name} ({info['size']} bytes, sha256={info['sha256']})",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description="RReturn self-check before committing or opening a PR")
    parser.add_argument("--quick", action="store_true", help="Skip live game E2E smoke tests")
    parser.add_argument("--e2e", action="store_true", help="Run fast live E2E smoke tests")
    args = parser.parse_args()

    check_runtime_assembly()
    check_mixins_and_exts()
    check_lang_parity()
    check_repo_hygiene()
    check_bundle_and_package()
    run_step("cargo clippy", ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    run_step("cargo test", ["cargo", "test"])
    check_python_syntax()

    if args.e2e and not args.quick:
        run_step(
            "rreturn_e2e --fast",
            [sys.executable, "tools/rreturn_e2e.py", "--isolated", "--workers", "1", "--fast"],
        )

    print("\n=======================================================")
    print(" pr_ready: ALL CHECKS PASSED")
    print("=======================================================")


if __name__ == "__main__":
    main()