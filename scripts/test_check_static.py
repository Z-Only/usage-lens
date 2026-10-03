"""Regression checks for the narrow, intentionally non-security-complete guard."""
from pathlib import Path
import json
import tempfile
import unittest

import check_static as guard


class StaticTests(unittest.TestCase):
    def version_fixture(self, root):
        for name in guard.OWNED_CARGO_PACKAGES:
            path = root / f'crates/{name}/Cargo.toml'
            path.parent.mkdir(parents=True)
            path.write_text(f'[package]\nname = "{name}"\nversion = "0.4.0"\n')
        for name in guard.OWNED_JSON_VERSIONS:
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(json.dumps({'name': 'usage-lens', 'version': '0.4.0'}))
        (root / 'Cargo.lock').write_text(''.join(
            f'[[package]]\nname = "{name}"\nversion = "0.4.0"\n'
            for name in guard.OWNED_CARGO_PACKAGES) +
            '[[package]]\nname = "unrelated-dependency"\nversion = "0.3.0"\n')

    def test_owned_versions_match_without_rewriting_dependencies(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.version_fixture(root)
            self.assertEqual(guard.check_versions(root), [])
            for name in [*guard.OWNED_JSON_VERSIONS,
                         *[f'crates/{name}/Cargo.toml' for name in guard.OWNED_CARGO_PACKAGES],
                         'Cargo.lock']:
                path = root / name
                original = path.read_text()
                path.write_text(original.replace('0.4.0', '0.3.0', 1))
                with self.subTest(name=name):
                    self.assertTrue(guard.check_versions(root))
                path.write_text(original)

    def test_owned_version_inventory_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertTrue(guard.check_versions(root))
            self.version_fixture(root)
            lock = root / 'Cargo.lock'
            original = lock.read_text()
            for content in ['invalid TOML', original.replace('usage-lens-ui', 'missing-owned-ui'),
                            original + '[[package]]\nname = "usage-lens"\nversion = "0.4.0"\n']:
                lock.write_text(content)
                self.assertTrue(guard.check_versions(root))
            lock.write_text(original)
            (root / 'package.json').write_text('{"name":"wrong","version":"0.4.0"}')
            self.assertTrue(guard.check_versions(root))

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
