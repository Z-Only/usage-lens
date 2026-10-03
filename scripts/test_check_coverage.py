"""Regression tests for the coverage gate, including real Git diffs."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import check_coverage as gate


class SourceSelectionTests(unittest.TestCase):
    def test_runtime_sources_are_included(self):
        for name in ["crates/usage-lens-ui/public/bootstrap.js", "crates/usage-lens/src/main.rs", "crates/usage-lens/src/lib.rs", "crates/usage-lens/src/adapters/worker.rs", "crates/core/src/tests.rs", "src/cli/main.ts", "src/cli/run.ts", "src/main.ts", "src/App.vue", "src/domain/index.ts", "src/core/__fixtures__/demo.ts", "src/core/tests/runtime.ts", "src/core/query.ts", "src/adapters/collector.ts", "src/server/http.ts", "src/ui/main.ts", "src/ui/App.vue", "src/adapters/worker.ts"]:
            with self.subTest(name=name):
                self.assertTrue(gate.production_source(name))

    def test_nonproduction_files_are_excluded(self):
        for name in ["src/env.d.ts", "src/App.test.ts", "tests/domain/core.test.ts", "README.md", "scripts/check_coverage.py", "frontend/vite.config.ts", "src/ui/env.d.ts", "src/ui/App.test.ts", "src/adapters/worker.spec.ts", "crates/core/tests/integration.rs", "crates/core/build.rs", "frontend/node_modules/package/index.js"]:
            with self.subTest(name=name):
                self.assertFalse(gate.production_source(name))


class RustModuleTests(unittest.TestCase):
    def test_only_module_wiring_and_reexports_are_zero_executable(self):
        for text in ['#![recursion_limit = "256"]\npub mod views;', "", "// documentation\npub mod core;\nmod server;", "/* outer /* inner */ comment */ pub(crate) mod cli;", "pub use self::store::{UsageStore, CoreError};", '#[cfg(all(feature = "csr", target_arch = "wasm32"))] mod browser;', '#[cfg(target_arch = "wasm32")] mod browser;', "use core::*;\npub use crate::adapters::Thing;"]:
            with self.subTest(text=text):
                self.assertTrue(gate.rust_module_only(text))

    def test_runtime_code_never_gets_a_missing_record_waiver(self):
        for text in ["fn main() {}", "pub const X: u8 = 1;", "pub mod core { fn run() {} }", 'include!("runtime.rs");', "#[cfg(test)] mod tests;", "macro_rules! m { () => {} }", "/* unclosed", "pub static X: u8 = 1;", "pub use foo; fn foo() {}", "impl Service {}", "pub struct Service;"]:
            with self.subTest(text=text):
                self.assertFalse(gate.rust_module_only(text))

    def test_scopes_separate_rust_and_frontend(self):
        self.assertTrue(gate.matches_scope("crates/usage-lens/src/main.rs", "rust"))
        self.assertFalse(gate.matches_scope("src/ui/main.ts", "rust"))
        self.assertTrue(gate.matches_scope("src/ui/main.ts", "frontend"))
        self.assertFalse(gate.matches_scope("src/core/store.ts", "frontend"))
        self.assertTrue(gate.matches_scope("src/core/store.ts", "all"))


class LcovTests(unittest.TestCase):
    def setUp(self):
        self.root = Path("/tmp/coverage-repo")

    def parse(self, content, source_root=None):
        return gate.parse_lcov(content, source_root or self.root, self.root)

    def test_absolute_and_relative_paths(self):
        self.assertEqual(self.parse("SF:/tmp/coverage-repo/src/core/query.ts\nDA:2,1\nend_of_record\n"), {"src/core/query.ts": {2: 1}})
        self.assertEqual(self.parse("SF:main.ts\nDA:2,0\nend_of_record\n", self.root / "src/ui"), {"src/ui/main.ts": {2: 0}})

    def test_duplicate_records_merge_without_inflating_denominator(self):
        result = self.parse("TN:first\nSF:src/ui/main.ts\nDA:2,0\nLF:1\nLH:0\nend_of_record\nTN:second\nSF:src/ui/main.ts\nDA:2,3\nDA:3,0,checksum\nLF:2\nLH:1\nend_of_record\n")
        self.assertEqual(result, {"src/ui/main.ts": {2: 3, 3: 0}})
        self.assertEqual(gate.merge_reports([result, result]), result)

    def test_llvm_function_summary_exceeds_physical_da_lines(self):
        # Regression: real LLVM exports had 243 DA lines but LF/LH of 257/257;
        # the native server had 271 covered DA lines but LF/LH of 354/326.
        report = self.parse("SF:src/core/query.ts\nDA:1,1\nDA:2,1\nLF:4\nLH:3\nend_of_record\n")
        result = gate.evaluate(set(report), {"src/core/query.ts": {1, 2}}, report)
        self.assertEqual((result.total_covered, result.total_lines), (3, 4))
        self.assertEqual((result.changed_covered, result.changed_lines), (2, 2))
        self.assertEqual(len(result.failures(95, 95)), 1)

    def test_duplicate_da_and_reports_do_not_inflate_summary(self):
        report = self.parse("SF:src/core/query.ts\nDA:1,0\nDA:1,2\nDA:2,0\nLF:3\nLH:1\nend_of_record\n")
        merged = gate.merge_reports([report, report])
        result = gate.evaluate(set(merged), {}, merged)
        self.assertEqual((result.total_covered, result.total_lines), (1, 3))
        self.assertEqual(merged["src/core/query.ts"], {1: 2, 2: 0})

    def test_summary_retains_uncovered_overlapping_function_line(self):
        report = self.parse("SF:src/core/query.ts\nDA:1,2\nDA:2,1\nLF:2\nLH:1\nend_of_record\n")
        self.assertEqual(report["src/core/query.ts"].totals(), (1, 2))

    def test_complementary_records_merge_only_proven_physical_hits(self):
        first = self.parse("SF:src/core/query.ts\nDA:1,2\nDA:2,0\nLF:4\nLH:2\nend_of_record\n")
        second = self.parse("SF:src/core/query.ts\nDA:1,0\nDA:2,1\nLF:4\nLH:2\nend_of_record\n")
        merged = gate.merge_reports([first, second])
        self.assertEqual(merged["src/core/query.ts"].totals(), (3, 4))

    def test_llvm_summary_only_without_da_is_rejected(self):
        with self.assertRaises(gate.CoverageError):
            self.parse("SF:src/core/query.ts\nLF:257\nLH:257\nend_of_record\n")

    def test_impossible_summary_is_rejected(self):
        for content in [
            "SF:a\nDA:1,1\nDA:2,1\nLF:1\nLH:1\nend_of_record\n",
            "SF:a\nDA:1,1\nLF:1\nLH:2\nend_of_record\n",
            "SF:a\nDA:1,0\nLF:1\nLH:1\nend_of_record\n",
        ]:
            with self.subTest(content=content), self.assertRaises(gate.CoverageError):
                self.parse(content)

    def test_explicit_zero_executable_record_is_preserved(self):
        self.assertEqual(self.parse("SF:src/ui/types.ts\nLF:0\nLH:0\nend_of_record\n"), {"src/ui/types.ts": {}})

    def test_malformed_or_incomplete_reports_fail_closed(self):
        invalid = ["SF:src/ui/worker.ts\nend_of_record\n", "", "TN:empty\n", "SF:\nend_of_record\n", "SF:../escape.ts\nend_of_record\n", "DA:1,1\n", "end_of_record\n", "SF:a\nDA:1,no\nend_of_record\n", "SF:a\nDA:0,1\nend_of_record\n", "SF:a\nDA:1,-1\nend_of_record\n", "SF:a\nDA:1\nend_of_record\n", "SF:a\n", "SF:a\nSF:b\n", "LF:2\n", "SF:a\nLF:-1\nend_of_record\n", "SF:a\nLF:x\nend_of_record\n", "SF:a\nDA:1,1\nLF:2\nend_of_record\n", "SF:a\nDA:1,0\nLH:1\nend_of_record\n"]
        for content in invalid:
            with self.subTest(content=content), self.assertRaises(gate.CoverageError):
                self.parse(content)


class EvaluationTests(unittest.TestCase):
    def test_exact_thresholds_pass(self):
        result = gate.CoverageResult(95, 100, 95, 100, (), {})
        self.assertEqual(result.failures(95, 95), [])

    def test_rounding_cannot_bypass_threshold(self):
        result = gate.CoverageResult(94999, 100000, 94999, 100000, (), {})
        self.assertEqual(len(result.failures(95, 95)), 2)

    def test_total_and_changed_95_percent_gates_are_independent(self):
        self.assertEqual(len(gate.CoverageResult(94, 100, 95, 100, (), {}).failures(95, 95)), 1)
        self.assertEqual(len(gate.CoverageResult(95, 100, 94, 100, (), {}).failures(95, 95)), 1)

    def test_missing_unchanged_and_changed_sources_fail(self):
        result = gate.evaluate({"a", "b", "c"}, {"a": {1}, "c": {1}}, {"a": {1: 1}})
        self.assertEqual(result.missing_files, ("b", "c"))
        self.assertEqual(len(result.failures(95, 95)), 1)

    def test_no_executable_changed_lines_is_allowed_but_empty_total_is_not(self):
        self.assertEqual(gate.CoverageResult(1, 1, 0, 0, (), {}).failures(95, 95), [])
        self.assertTrue(gate.CoverageResult(0, 0, 0, 0, (), {}).failures(95, 95))
        self.assertIn("n/a", gate.percentage(0, 0))

    def test_changed_denominator_uses_instrumented_lines(self):
        result = gate.evaluate({"a"}, {"a": {1, 2, 3, 4}}, {"a": {2: 1, 4: 0, 6: 1}, "deleted": {1: 0}})
        self.assertEqual((result.total_covered, result.total_lines), (2, 3))
        self.assertEqual((result.changed_covered, result.changed_lines), (1, 2))
        self.assertEqual(result.uncovered_changed, {"a": [4]})


class GitTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git("init", "-q")
        # Every fixture is a separate repository; checkout's config does not apply.
        # Detached post-commit maintenance can otherwise race TemporaryDirectory cleanup.
        self.git("config", "maintenance.auto", "false")
        self.git("config", "gc.auto", "0")
        self.git("config", "user.email", "ci-test@example.invalid")
        self.git("config", "user.name", "Coverage Tests")
        self.write("README.md", "initial\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()

    def git(self, *args, env=None):
        return subprocess.run(["git", "-C", str(self.root), *args], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env).stdout

    def write(self, name, text):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")

    def commit(self):
        self.git("add", "--all")
        self.git("commit", "-qm", "test")

    def changes(self, working_tree=False):
        return gate.changed_lines(self.root, self.base, None if working_tree else "HEAD")

    def test_fixture_commits_do_not_start_automatic_maintenance(self):
        self.assertEqual(self.git("config", "--get", "maintenance.auto").strip(), "false")
        self.assertEqual(self.git("config", "--get", "gc.auto").strip(), "0")
        trace = self.root / ".git" / "fixture-trace.json"
        env = {**os.environ, "GIT_TRACE2_EVENT": str(trace)}
        self.git("commit", "--allow-empty", "-qm", "isolated fixture", env=env)

        def housekeeping_children():
            records = [json.loads(line) for line in trace.read_text(encoding="utf-8").splitlines()]
            self.assertTrue(any(record.get("event") == "start" for record in records))
            return [record for record in records if record.get("event") == "child_start"
                    and any(argument in ("maintenance", "gc") for argument in record.get("argv", []))]

        self.assertEqual(housekeeping_children(), [])
        # Positive control proves trace matching works, while explicitly staying synchronous.
        trace.unlink()
        self.git("-c", "maintenance.auto=true", "-c", "maintenance.autoDetach=false",
                 "-c", "gc.autoDetach=false", "commit", "--allow-empty", "-qm", "trace control", env=env)
        self.assertTrue(housekeeping_children())

    def test_added_source_counts_all_lines(self):
        self.write("src/ui/main.ts", "export const first = 1\nexport const second = 2\n")
        self.commit()
        self.assertEqual(self.changes(), {"src/ui/main.ts": {1, 2}})
        self.assertEqual(gate.source_inventory(self.root, "HEAD"), {"src/ui/main.ts"})

    def test_modified_deleted_and_renamed_lines(self):
        self.write("src/ui/old.ts", "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n")
        self.write("src/ui/deleted.ts", "remove\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()
        self.git("mv", "src/ui/old.ts", "src/ui/renamed.ts")
        self.write("src/ui/renamed.ts", "a\nb\nc\nchanged\ne\nf\ng\nh\ni\nj\n")
        self.git("rm", "src/ui/deleted.ts")
        self.commit()
        self.assertEqual(self.changes(), {"src/ui/renamed.ts": {4}})

    def test_unchanged_rename_has_no_changed_lines(self):
        self.write("src/ui/old.ts", "export const a = 1\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()
        self.git("mv", "src/ui/old.ts", "src/ui/new.ts")
        self.commit()
        self.assertEqual(self.changes(), {"src/ui/new.ts": set()})

    def test_deletion_only_has_no_added_lines(self):
        self.write("src/ui/main.ts", "a\nb\nc\n")
        self.commit()
        self.base = self.git("rev-parse", "HEAD").strip()
        self.write("src/ui/main.ts", "a\nc\n")
        self.commit()
        self.assertEqual(self.changes(), {"src/ui/main.ts": set()})

    def test_working_tree_includes_staged_unstaged_and_untracked_but_not_tests(self):
        self.write("src/ui/staged.ts", "one\n")
        self.git("add", "src/ui/staged.ts")
        self.write("src/ui/staged.ts", "one\ntwo\n")
        self.write("src/adapters/worker.ts", "worker\n")
        self.write("src/adapters/worker.test.ts", "test\n")
        self.assertEqual(self.changes(True), {"src/ui/staged.ts": {1, 2}, "src/adapters/worker.ts": {1}})
        self.assertEqual(self.changes(), {})
        self.assertEqual(gate.source_inventory(self.root, None), {"src/ui/staged.ts", "src/adapters/worker.ts"})

    def test_spaces_tabs_unicode_and_pathspec_characters(self):
        name = "src/ui/space\tü [*].ts"
        self.write(name, "one\ntwo\n")
        self.commit()
        self.assertEqual(self.changes(), {name: {1, 2}})

    def test_binary_source_fails(self):
        self.write("src/ui/binary.ts", "binary\0data\n")
        with self.assertRaises(gate.CoverageError):
            self.changes(True)
        self.commit()
        with self.assertRaises(gate.CoverageError):
            self.changes()

    def test_invalid_base_fails(self):
        with self.assertRaises(gate.CoverageError):
            gate.revision(self.root, "nonexistent")

    def test_empty_tree_can_be_initial_base(self):
        empty = subprocess.run(["git", "-C", str(self.root), "hash-object", "-t", "tree", "--stdin"], input="", text=True, check=True, stdout=subprocess.PIPE).stdout.strip()
        self.assertEqual(gate.revision(self.root, empty, tree=True), empty)

    def test_browser_bridge_is_counted_uncovered_even_with_native_hits(self):
        name = "crates/usage-lens-ui/src/browser.rs"
        self.write(name, "// Browser runtime\n\npub fn mount() {\n    work();\n}\n")
        self.commit()
        reports, unmeasured = gate.apply_conservative_bounds(self.root, "HEAD", {name}, {name: gate.LineCoverage({3: 1, 4: 1, 5: 1})})
        self.assertEqual(reports[name], {1: 0, 3: 0, 4: 0, 5: 0})
        self.assertEqual(unmeasured[0]["uncovered_lines"], 4)
        result = gate.evaluate({name}, {name: {1, 2, 3, 4, 5}}, reports)
        self.assertEqual((result.total_covered, result.total_lines, result.changed_covered, result.changed_lines), (0, 4, 0, 4))
        self.assertEqual(len(result.failures(95, 95)), 2)

    def test_conservative_bound_never_discards_existing_llvm_summary_instances(self):
        name = "crates/usage-lens-ui/src/event_bridge.rs"
        self.write(name, "fn browser_event() {}\n")
        existing = gate.LineCoverage({1: 1}, extra_lines=4, extra_uncovered=0)
        reports, unmeasured = gate.apply_conservative_bounds(self.root, None, {name}, {name: existing})
        self.assertEqual(reports[name].totals(), (0, 5))
        self.assertEqual(unmeasured[0]["uncovered_lines"], 5)

    def test_unmeasured_allowlist_never_masks_other_missing_sources(self):
        name = "crates/usage-lens-ui/src/forgotten.rs"
        self.write(name, "pub fn hidden() {}\n")
        reports, unmeasured = gate.apply_conservative_bounds(self.root, None, {name}, {})
        self.assertEqual(unmeasured, [])
        self.assertEqual(gate.evaluate({name}, {}, reports).missing_files, (name,))

    def test_missing_rust_wiring_allowed_but_main_never_excluded(self):
        self.write("crates/app/src/lib.rs", "pub mod core;\n")
        self.write("crates/app/src/main.rs", "fn main() { println!(\"run\"); }\n")
        self.commit()
        inventory = gate.source_inventory(self.root, "HEAD")
        reports, modules = gate.supply_module_records(self.root, "HEAD", inventory, {})
        self.assertEqual(modules, ["crates/app/src/lib.rs"])
        self.assertEqual(gate.evaluate(inventory, {}, reports).missing_files, ("crates/app/src/main.rs",))
        self.write("crates/app/src/lib.rs", "fn changed() {}\n")
        # The committed test target is inspected, not a different checkout file.
        self.assertEqual(gate.supply_module_records(self.root, "HEAD", inventory, {})[1], modules)
        self.assertEqual(gate.supply_module_records(self.root, None, inventory, {})[1], [])

    def test_explicit_empty_rust_runtime_report_cannot_bypass_instrumentation(self):
        name = "crates/app/src/main.rs"
        self.write(name, "fn main() { work(); }\n")
        with self.assertRaisesRegex(gate.CoverageError, "empty executable coverage"):
            gate.supply_module_records(self.root, None, {name}, {name: gate.LineCoverage()})
        self.write(name, "pub mod runtime;\n")
        reports, modules = gate.supply_module_records(self.root, None, {name}, {name: gate.LineCoverage()})
        self.assertEqual(modules, [name])
        self.assertEqual(reports[name].totals(), (0, 0))

    def test_added_rust_source_is_in_changed_inventory(self):
        self.write("crates/app/src/main.rs", "fn main() {}\n")
        self.assertEqual(self.changes(True), {"crates/app/src/main.rs": {1}})
        self.commit()
        self.assertEqual(self.changes(), {"crates/app/src/main.rs": {1}})

    def test_cli_defaults_to_95_percent_for_both_gates(self):
        name = "src/ui/main.ts"
        self.write(name, "export const value = 1\n" * 20)
        self.commit()
        args = ["--repo", str(self.root), "--base", self.base, "--report", "coverage.lcov", ".", "--json-output", str(self.root / "summary.json")]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            for covered, expected_exit in [(19, 0), (18, 1)]:
                records = "".join(f"DA:{line},{int(line <= covered)}\n" for line in range(1, 21))
                self.write("coverage.lcov", f"SF:{name}\n{records}LF:20\nLH:{covered}\nend_of_record\n")
                self.assertEqual(gate.main(args), expected_exit)
                summary = json.loads((self.root / "summary.json").read_text())
                self.assertEqual(summary["total"]["minimum"], 95)
                self.assertEqual(summary["changed"]["minimum"], 95)

    def test_cli_pass_fail_json_and_missing_report(self):
        name = "src/ui/main.ts"
        self.write(name, "export const one = 1\nexport const two = 2\n")
        self.commit()
        self.write("coverage.lcov", f"SF:{name}\nDA:1,1\nDA:2,1\nend_of_record\n")
        args = ["--repo", str(self.root), "--base", self.base, "--report", "coverage.lcov", ".", "--json-output", str(self.root / "summary.json")]
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(gate.main(args), 0)
            self.assertTrue(json.loads((self.root / "summary.json").read_text())["passed"])
            self.write("coverage.lcov", f"SF:{name}\nDA:1,1\nDA:2,0\nend_of_record\n")
            self.assertEqual(gate.main(args), 1)
            self.assertFalse(json.loads((self.root / "summary.json").read_text())["passed"])
            self.write("coverage.lcov", "")
            self.assertEqual(gate.main(args), 2)
            (self.root / "coverage.lcov").unlink()
            self.assertEqual(gate.main(args), 2)


if __name__ == "__main__":
    unittest.main()
