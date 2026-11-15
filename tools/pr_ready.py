import argparse
import json
import py_compile
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def run_step(label, cmd):
    print(f"==> {label}: {' '.join(cmd)}", flush=True)
    proc = subprocess.run(cmd, cwd=ROOT)
    if proc.returncode != 0:
        print(f"[FAIL] {label} failed with exit code {proc.returncode}", file=sys.stderr)
        sys.exit(proc.returncode)


def check_python_syntax():
    py_files = sorted(ROOT.glob("tools/**/*.py"))
    print(f"==> Python syntax check ({len(py_files)} files)", flush=True)
    for path in py_files:
        py_compile.compile(str(path), doraise=True)


def check_bundle_and_package():
    print("==> Checking generated mixins/rreturn/ and .hnmod package", flush=True)
    run_step("cargo check", ["cargo", "check"])
    proc = subprocess.run(
        [sys.executable, "tools/package.py"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    info = json.loads(proc.stdout)
    hnmod_path = Path(info["file"])
    if not hnmod_path.is_file():
        print(f"[FAIL] Missing package output: {hnmod_path}", file=sys.stderr)
        sys.exit(1)
    with zipfile.ZipFile(hnmod_path, "r") as zf:
        names = set(zf.namelist())
        manifest = json.loads(zf.read("horizon-native.json"))
    bundle_prefix = f"mixins/{manifest['id']}/"
    required = {
        "horizon-native.json",
        bundle_prefix + "horizon-mixin.toml",
        bundle_prefix + "LICENSE",
        bundle_prefix + "runtime.rs",
    }
    missing = sorted(required - names)
    if missing:
        print(f"[FAIL] Incomplete .hnmod archive, missing: {missing}", file=sys.stderr)
        sys.exit(1)
    invalid = sorted(
        n
        for n in names
        if n != "horizon-native.json"
        and n != bundle_prefix + "LICENSE"
        and not (n.startswith(bundle_prefix) and n.endswith((".rs", ".toml")))
    )
    if invalid:
        print(f"[FAIL] Disallowed files in .hnmod archive: {invalid}", file=sys.stderr)
        sys.exit(1)
    print(
        f"    Packaged {hnmod_path.name} ({info['size']} bytes, sha256={info['sha256']})",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description="RReturn self-check before committing or opening a PR")
    parser.add_argument("--quick", action="store_true", help="Skip live game E2E smoke tests")
    parser.add_argument("--e2e", action="store_true", help="Run fast live E2E smoke tests")
    args = parser.parse_args()

    check_bundle_and_package()
    run_step("cargo clippy", ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    run_step("cargo test", ["cargo", "test"])
    check_python_syntax()

    if args.e2e and not args.quick:
        run_step("rreturn_e2e --fast", [sys.executable, "tools/rreturn_e2e.py", "--fast"])

    print("\n=======================================================")
    print(" pr_ready: ALL CHECKS PASSED")
    print("=======================================================")


if __name__ == "__main__":
    main()