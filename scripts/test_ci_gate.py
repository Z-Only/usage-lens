"""Aggregate ordering and fail-closed shell behavior, with synthetic tools."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class AggregateTests(unittest.TestCase):
    def run_gate(self, *, ci='', base='', fail=''):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'scripts').mkdir()
            shutil.copyfile(Path(__file__).with_name('ci_gate.sh'), root / 'scripts/ci_gate.sh')
            bin_dir = root / 'bin'
            bin_dir.mkdir()
            for name in ['git', 'python3', 'cargo']:
                command = bin_dir / name
                command.write_text('#!/usr/bin/env bash\nset -eu\n'
                                   f'printf "{name} %s\\n" "$*" >> "$TRACE"\n'
                                   f'[[ "{name} $*" != "$FAIL_STEP" ]]\n'
                                   + ('printf "empty-tree\\n"\n' if name == 'git' else ''))
                command.chmod(0o755)
            env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ['PATH'],
                       TRACE=str(root / 'trace'), FAIL_STEP=fail, CI=ci, COVERAGE_BASE=base)
            result = subprocess.run(['bash', str(root / 'scripts/ci_gate.sh')], cwd='/', env=env,
                                    text=True, capture_output=True)
            trace = root / 'trace'
            return result.returncode, trace.read_text().splitlines() if trace.exists() else []

    def test_local_default_and_required_steps(self):
        code, trace = self.run_gate()
        self.assertEqual(code, 0)
        self.assertTrue(trace[0].startswith('git hash-object -t tree'))
        self.assertEqual(trace[1:5], ['python3 scripts/check_static.py',
                                     'python3 -m unittest discover -s scripts -p test_*.py',
                                     'python3 scripts/build_ui.py', 'cargo fmt --all --check'])
        self.assertIn('--all-targets --features usage-lens-ui/ssr -- -D warnings', trace[5])
        self.assertIn('--target wasm32-unknown-unknown', trace[6])
        self.assertIn('cargo llvm-cov --locked --workspace --all-targets', trace[7])
        self.assertEqual(trace[8], 'cargo build --locked -p usage-lens')
        self.assertIn('--base empty-tree', trace[-1])
        self.assertIn('--working-tree', trace[-1])
        self.assertIn('--total-min 95 --changed-min 95', trace[-1])
        self.assertNotIn('coverage/lcov.info', trace[-1])

    def test_ci_uses_explicit_base_and_committed_source(self):
        code, trace = self.run_gate(ci='true', base='fixed-base')
        self.assertEqual(code, 0)
        self.assertFalse(any(line.startswith('git ') for line in trace))
        self.assertIn('--base fixed-base', trace[-1])
        self.assertNotIn('--working-tree', trace[-1])

    def test_ci_missing_base_fails_before_any_work(self):
        code, trace = self.run_gate(ci='true')
        self.assertNotEqual(code, 0)
        self.assertEqual(trace, [])

    def test_failure_stops_later_checks(self):
        code, trace = self.run_gate(base='base', fail='cargo fmt --all --check')
        self.assertNotEqual(code, 0)
        self.assertEqual(trace[-1], 'cargo fmt --all --check')
        self.assertFalse(any('llvm-cov' in line for line in trace))


if __name__ == '__main__':
    unittest.main()
