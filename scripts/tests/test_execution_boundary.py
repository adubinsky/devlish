"""Regression tests for prohibited runners and staged-content enforcement."""
import importlib.util
import pathlib
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "check_execution_boundary.py"
SPEC = importlib.util.spec_from_file_location("boundary", SCRIPT)
boundary = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(boundary)


class BoundaryTest(unittest.TestCase):
    def test_approved_runtime_and_verifier_are_allowed(self):
        files = {}
        for manifest, (name, path) in boundary.BINARIES.items():
            files[manifest] = f'[package]\nname="{name}"\nautobins=false\nautoexamples=false\n[[bin]]\nname="{name}"\npath="{path}"\n'
        for path in boundary.MAIN_FILES:
            files[path] = "fn main() {}"
        self.assertEqual(boundary.violations(files), [])

    def test_standalone_examples_are_rejected_even_without_main(self):
        errors = boundary.violations({"crates/provider/examples/demo.rs": "include!(\"runner.inc\");"})
        self.assertTrue(any("examples are prohibited" in error for error in errors))

    def test_renamed_rust_runner_is_rejected(self):
        self.assertTrue(boundary.violations({"tools/hidden.rs": "fn\nmain () {}"}))

    def test_arbitrary_command_wrapper_is_rejected(self):
        self.assertTrue(boundary.violations({"scripts/toolrun.py": "import subprocess"}))

    def test_removed_live_model_script_is_rejected(self):
        self.assertTrue(boundary.violations({"scripts/llm_serve_harness.sh": "curl localhost/v1/run"}))

    def test_custom_binary_paths_are_rejected(self):
        files = {"crates/provider/Cargo.toml": '[package]\nname="provider"\nautobins=false\nautoexamples=false\n[[bin]]\nname="demo"\npath="custom/runner.rs"\n'}
        self.assertTrue(boundary.violations(files))

    def test_extra_binary_in_approved_crate_is_rejected(self):
        source = (SCRIPT.parents[1] / "crates/devlish_core/Cargo.toml").read_text()
        self.assertTrue(boundary.violations({"crates/devlish_core/Cargo.toml": source + '\n[[bin]]\nname="demo"\npath="custom.rs"\n'}))

    def test_automatic_cargo_targets_are_rejected(self):
        self.assertTrue(boundary.violations({"crates/provider/Cargo.toml": '[package]\nname="provider"\n'}))

    def test_explicit_cargo_examples_are_rejected(self):
        self.assertTrue(boundary.violations({"crates/provider/Cargo.toml": '[package]\nname="provider"\nautobins=false\nautoexamples=false\n[[example]]\nname="demo"\npath="custom.rs"\n'}))

    def test_custom_test_runner_is_rejected(self):
        self.assertTrue(boundary.violations({"crates/provider/Cargo.toml": '[package]\nname="provider"\nautobins=false\nautoexamples=false\n[[test]]\nname="demo"\nharness=false\n'}))

    def test_build_runner_is_rejected(self):
        self.assertTrue(boundary.violations({"crates/provider/build.rs": "fn main() {}"}))

    def test_pure_adapter_library_is_allowed(self):
        self.assertEqual(boundary.violations({"crates/provider/src/lib.rs": "pub fn decode() {}"}), [])

    def test_staged_runner_cannot_be_hidden_by_unstaged_fix(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            source = root / "runner.rs"
            source.write_text("fn main() {}")
            subprocess.run(["git", "add", "runner.rs"], cwd=root, check=True)
            source.write_text("pub fn decode() {}")
            self.assertTrue(boundary.violations(boundary.read_files(root, staged=True)))
            self.assertEqual(boundary.violations(boundary.read_files(root, staged=False)), [])

    def test_installed_hook_blocks_real_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            subprocess.run(["git", "init", "-q", directory], check=True)
            (root / "scripts").mkdir()
            shutil.copy(SCRIPT, root / "scripts" / SCRIPT.name)
            hooks = root / ".git/hooks"
            shutil.copy(SCRIPT.parents[1] / ".githooks/pre-commit", hooks / "pre-commit")
            (hooks / "pre-commit").chmod(0o755)
            (root / "runner.rs").write_text("fn main() {}")
            subprocess.run(["git", "add", "."], cwd=root, check=True)
            result = subprocess.run(["git", "-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-m", "test: prohibited runner"], cwd=root, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unapproved native entry point", result.stderr)
            self.assertNotEqual(subprocess.run(["git", "rev-parse", "--verify", "HEAD"], cwd=root, capture_output=True).returncode, 0)


if __name__ == "__main__":
    unittest.main()
