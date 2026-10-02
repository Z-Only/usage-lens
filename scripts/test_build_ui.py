"""Build-pipeline regression tests; all compiler processes are synthetic."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import build_ui


class BuildTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.crate = self.root / 'crates/usage-lens-ui'
        self.crate.mkdir(parents=True)
        (self.crate / 'public').mkdir()
        for name in ['Cargo.toml', 'index.html', 'public/styles.css', 'public/bootstrap.js']:
            (self.crate / name).write_text(name)
        self.commands = []
        self.version = f'wasm-bindgen {build_ui.WASM_BINDGEN_VERSION}'
        self.valid_wasm = True
        self.emit_assets = True

    def run_command(self, command, **kwargs):
        self.commands.append(command)
        self.assertTrue(kwargs.get('check'))
        if command == ['wasm-bindgen', '--version']:
            return subprocess.CompletedProcess(command, 0, self.version)
        self.assertEqual(kwargs['cwd'], self.root)
        if command[0] == 'cargo':
            wasm = self.root / 'target/wasm32-unknown-unknown/release/usage_lens_ui.wasm'
            wasm.parent.mkdir(parents=True)
            wasm.write_bytes(b'\0asmversion' if self.valid_wasm else b'bad')
        elif self.emit_assets:
            output = Path(command[command.index('--out-dir') + 1])
            (output / 'usage_lens_ui.js').write_text('synthetic loader')
            (output / 'usage_lens_ui_bg.wasm').write_bytes(b'\0asm')
        return subprocess.CompletedProcess(command, 0)

    def test_build_replaces_only_stale_ui_assets(self):
        (self.root / 'dist/ui').mkdir(parents=True)
        (self.root / 'dist/ui/old-vue.js').write_text('obsolete')
        (self.root / 'dist/keep').write_text('other artifacts')
        build_ui.build(self.root, self.run_command)
        self.assertEqual(sorted(path.name for path in (self.root / 'dist/ui').iterdir()),
                         ['bootstrap.js', 'frontend-build.json', 'index.html', 'styles.css', 'usage_lens_ui.js', 'usage_lens_ui_bg.wasm'])
        self.assertTrue((self.root / 'dist/keep').is_file())
        manifest = json.loads((self.root / 'dist/ui/frontend-build.json').read_text())
        self.assertEqual(manifest['framework'], 'leptos')
        self.assertEqual(manifest['assets']['styles.css'], hashlib.sha256(b'public/styles.css').hexdigest())
        self.assertIn('--locked', self.commands[1])
        self.assertIn('csr', self.commands[1])
        self.assertIn('--no-typescript', self.commands[2])

    def test_missing_source_fails_before_compilation(self):
        (self.crate / 'index.html').unlink()
        with self.assertRaisesRegex(RuntimeError, 'Missing Leptos'):
            build_ui.build(self.root, self.run_command)
        self.assertEqual(self.commands, [])

    def test_mismatched_tool_version_fails_before_compilation(self):
        self.version = 'wasm-bindgen 0.1.0'
        with self.assertRaisesRegex(RuntimeError, 'Expected wasm-bindgen'):
            build_ui.build(self.root, self.run_command)
        self.assertEqual(len(self.commands), 1)

    def test_invalid_wasm_fails(self):
        self.valid_wasm = False
        with self.assertRaisesRegex(RuntimeError, 'valid WebAssembly'):
            build_ui.build(self.root, self.run_command)

    def test_missing_generated_assets_fails(self):
        self.emit_assets = False
        with self.assertRaisesRegex(RuntimeError, 'did not produce usage_lens_ui.js'):
            build_ui.build(self.root, self.run_command)
        self.assertFalse((self.root / 'dist/ui').exists())


if __name__ == '__main__':
    unittest.main()
