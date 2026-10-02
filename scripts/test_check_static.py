"""Regression checks for the narrow, intentionally non-security-complete guard."""
from pathlib import Path
import tempfile
import unittest

import check_static as guard


class StaticTests(unittest.TestCase):
    def test_allow_collection_storage_and_same_origin_fetch(self):
        source = "const body = input.prompt; store.save(body); fetch('/api/overview');"
        self.assertEqual(guard.inspect_source(source), [])

    def test_reject_dangerous_patterns_and_keep_lines(self):
        for source, reason in [
            ('eval(input)', 'dynamic code execution'),
            ('new Function(input)', 'dynamic code execution'),
            ('node.innerHTML = body', 'unsafe HTML insertion'),
            ('<div v-html="body"/>', 'unsafe HTML insertion'),
            ('// @ts-nocheck', 'disabled type safety'),
            ('x as any', 'disabled type safety'),
            ('/* v8 ignore next */', 'coverage suppression'),
            ('#[coverage(off)] fn skipped() {}', 'coverage suppression'),
            ('#[cfg(not(coverage))] fn skipped() {}', 'coverage suppression'),
            ('-----BEGIN PRIVATE KEY-----', 'embedded private key'),
        ]:
            with self.subTest(source=source):
                self.assertEqual(guard.inspect_source('\n' + source), [(2, reason)])

    def test_empty_and_unreadable_source_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertTrue(guard.check(root))
            (root / 'src').mkdir()
            (root / 'src/main.ts').write_bytes(b'\xff')
            self.assertIn('unreadable source', guard.check(root)[0])

    def test_uncompiled_frontend_runtime_cannot_hide_in_measured_file(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'crates/usage-lens-ui/src/views.rs'
            source.parent.mkdir(parents=True)
            source.write_text('#[cfg(target_arch = "wasm32")] fn hidden() {}')
            self.assertIn('feature/target-gated UI runtime', guard.check(root)[0])
            source.write_text('#[cfg(target_arch = "wasm32")] mod browser;')
            self.assertEqual(guard.check(root), [])
            source.unlink()
            (source.parent / 'browser.rs').write_text('#[cfg(target_arch = "wasm32")] fn accounted() {}')
            self.assertEqual(guard.check(root), [])

    def test_scans_backend_ui_and_cli(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for filename in ['src/cli/main.ts', 'src/ui/App.vue', 'src/core/query.ts', 'crates/app/src/main.rs']:
                path = root / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('eval(data)\n')
            self.assertEqual(len(guard.check(root)), 4)


if __name__ == '__main__':
    unittest.main()
