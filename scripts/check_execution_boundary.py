#!/usr/bin/env python3
"""Reject alternate execution surfaces in the Git index or working tree."""
import argparse
import pathlib
import re
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARIES = {
    "crates/devlish_core/Cargo.toml": ("devlish-core", "src/main.rs"),
    "crates/devlish_audit/Cargo.toml": ("devlish-audit", "src/main.rs"),
}
MAIN_FILES = {str(pathlib.PurePosixPath(p).parent / target[1])
              for p, target in BINARIES.items()}
# Existing compiler/VM wrappers, developer tooling and historical data fixtures.
# New host scripts require explicit human review, never automatic allowlisting.
HOST_SCRIPTS = {
    "bin/devlish", "install.sh", ".githooks/pre-commit",
    "scripts/install_git_hooks.sh", "scripts/check_execution_boundary.py",
    "scripts/tests/test_execution_boundary.py",
    "scripts/install-loop-hook.sh", "scripts/git-hooks/post-commit",
    "scripts/generate_xlsx_expected_cells_fixture.mjs",
    "scripts/generate_xlsx_due_diligence_fixture.mjs",
    "scripts/build_wasm_runner.sh",
    "crates/devlish_wasm_compiler/build.sh",
    "apps/course/scripts/build-manifest.mjs",
    "packages/devlish-runtime/scripts/inline-wasm.mjs",
    "packages/devlish-runtime/test/runtime.test.mjs",
    "examples/bytecode_wasm/node/run.mjs",
    "examples/archived_samples/medication-invoice-verifier/gateway/app.rb",
    "examples/archived_samples/medication-invoice-verifier/gateway/invoice_verification_job.rb",
    "examples/archived_samples/medication-invoice-verifier/scripts/load_example.sh",
    "crates/devlish_wasm_runner/js/index.mjs",
    "crates/devlish_wasm_runner/js/benchmark.mjs",
    "crates/devlish_wasm_runner/bin/devlish-runner.mjs",
    "crates/devlish_wasm_runner/test/benchmark.test.mjs",
    "crates/devlish_wasm_compiler/test/compile.test.mjs",
    "crates/devlish_wasm_compiler/js/compiler.mjs",
}


def violations(files):
    errors = []
    for name, source in files.items():
        path = pathlib.PurePosixPath(name)
        if path.suffix in {".py", ".sh", ".mjs", ".js", ".rb"} or name.startswith("bin/"):
            if name not in HOST_SCRIPTS:
                errors.append(f"{name}: unapproved host script or runner")
        if path.suffix == ".rs":
            if "examples" in path.parts:
                errors.append(f"{name}: standalone Rust examples are prohibited")
            if path.name == "build.rs" and "src" not in path.parts:
                errors.append(f"{name}: unapproved build-time execution")
            if re.search(r"\bfn\s+main\s*\(", source) and name not in MAIN_FILES:
                errors.append(f"{name}: unapproved native entry point")
        if path.name != "Cargo.toml":
            continue
        try:
            manifest = tomllib.loads(source)
        except tomllib.TOMLDecodeError as exc:
            errors.append(f"{name}: invalid manifest: {exc}")
            continue
        package = manifest.get("package", {})
        if not package:
            continue
        if package.get("autoexamples") is not False or package.get("autobins") is not False:
            errors.append(f"{name}: autobins and autoexamples must both be false")
        if package.get("build") not in (None, False):
            errors.append(f"{name}: unapproved build script")
        if manifest.get("example"):
            errors.append(f"{name}: Cargo example targets are prohibited")
        expected = BINARIES.get(name)
        targets = manifest.get("bin", [])
        if expected:
            if len(targets) != 1 or (targets[0].get("name"), targets[0].get("path")) != expected:
                errors.append(f"{name}: only the approved native target is permitted")
        elif targets:
            errors.append(f"{name}: additional native executables are prohibited")
        for kind in ("test", "bench"):
            if any(target.get("harness") is False for target in manifest.get(kind, [])):
                errors.append(f"{name}: custom executable {kind} harness is prohibited")
    return errors


def read_files(root, staged):
    # The index, rather than git diff text, detects renamed and custom-path targets.
    output = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=root)
    files = {}
    for entry in output.split(b"\0"):
        if not entry:
            continue
        metadata, raw_name = entry.split(b"\t", 1)
        mode, oid, stage = metadata.split()
        name = raw_name.decode("utf-8")
        if stage != b"0":
            raise ValueError(f"{name}: unresolved index conflict")
        if mode in (b"120000", b"160000"):
            raise ValueError(f"{name}: symlinks and embedded repositories need explicit boundary review")
        relevant = pathlib.PurePosixPath(name).suffix in {".rs", ".py", ".sh", ".mjs", ".js", ".rb"} or name.endswith("Cargo.toml") or name.startswith("bin/")
        if not relevant:
            continue
        if staged:
            data = subprocess.check_output(["git", "cat-file", "blob", oid.decode()], cwd=root)
        else:
            path = root / name
            if not path.exists():
                continue
            data = path.read_bytes()
        files[name] = data.decode("utf-8")
    if not staged:
        for raw_name in subprocess.check_output(["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=root).split(b"\0"):
            if raw_name:
                name = raw_name.decode("utf-8")
                path = root / name
                if path.suffix in {".rs", ".py", ".sh", ".mjs", ".js", ".rb"} or path.name == "Cargo.toml":
                    if path.is_symlink():
                        raise ValueError(f"{name}: source symlink is prohibited")
                    files[name] = path.read_text()
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--staged", action="store_true")
    args = parser.parse_args()
    try:
        errors = violations(read_files(ROOT, args.staged))
    except (ValueError, OSError, subprocess.CalledProcessError) as exc:
        print(f"Execution boundary check failed: {exc}", file=sys.stderr)
        return 1
    if errors:
        print("Prohibited execution surface. User code must stay inside the governed runtime:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print("Execution boundary check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
