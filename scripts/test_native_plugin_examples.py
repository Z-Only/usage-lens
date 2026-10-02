"""Keep distributed setup examples native, inert, and correctly separated/quoted.

These source-only tests run in the existing unittest gate before Cargo builds.
They never register a plugin, run a hook, launch a collector, or open a user store.
"""
import json
from pathlib import Path, PureWindowsPath
import re
import shlex
import unittest

ROOT = Path(__file__).resolve().parents[1]
PLUGIN = ROOT / 'plugin/usage-lens'
RETIRED_ENTRYPOINT = re.compile(r'dist[\\/]+cli[\\/]+main\.js')


class NativePluginExamplesTests(unittest.TestCase):
    def test_distributed_docs_and_config_never_reference_retired_entrypoint(self):
        files = [ROOT / 'README.md']
        extensions = {'.md', '.json', '.toml', '.yaml', '.yml', '.txt'}
        for directory in [ROOT / 'docs', PLUGIN]:
            files.extend(path for path in directory.rglob('*')
                         if path.is_file() and path.suffix in extensions)
        self.assertGreater(len(files), 5)
        for path in files:
            with self.subTest(path=path.relative_to(ROOT)):
                self.assertIsNone(RETIRED_ENTRYPOINT.search(path.read_text(encoding='utf-8')))
        for separator in ['/', '\\', '\\\\']:
            self.assertIsNotNone(RETIRED_ENTRYPOINT.search(separator.join(['dist', 'cli', 'main.js'])))

    def test_mcp_command_is_native_and_args_have_no_script_or_shell_quotes(self):
        config = json.loads((PLUGIN / 'examples/mcp.config.example.json').read_text())
        server = config['mcpServers']['usage-lens-local']
        self.assertEqual(server, {
            'command': '/ABSOLUTE/usage-lens',
            'args': ['mcp', '--db', '/ABSOLUTE/usage.sqlite'],
        })
        # Command and database remain individual arguments even with spaces.
        vector = [server['command'].replace('ABSOLUTE', 'ABSOLUTE path'),
                  *[arg.replace('ABSOLUTE', 'ABSOLUTE path') for arg in server['args']]]
        self.assertEqual(len(vector), 4)
        self.assertTrue(all(not arg.startswith(('"', "'")) for arg in vector))

    def test_each_hook_invokes_native_hook_with_literal_posix_paths(self):
        config = json.loads((PLUGIN / 'examples/hooks.config.example.json').read_text())
        self.assertIn('POSIX-shell', config['description'])
        self.assertEqual(set(config['hooks']), {'PostToolUse', 'UserPromptSubmit', 'Stop'})
        for groups in config['hooks'].values():
            for group in groups:
                for handler in group['hooks']:
                    self.assertEqual(handler['type'], 'command')
                    self.assertIs(handler['async'], True)
                    self.assertEqual(handler['timeout'], 10)
                    self.assertEqual(shlex.split(handler['command']), [
                        '/ABSOLUTE/usage-lens', 'hook', '--db',
                        '/ABSOLUTE/usage.sqlite', '--source', 'local-hooks',
                    ])
                    # A literal replacement does not split spaces or interpolate shell syntax.
                    replaced = handler['command'].replace('ABSOLUTE', 'ABSOLUTE path $x;$(ignored)')
                    parsed = shlex.split(replaced)
                    self.assertEqual(len(parsed), 6)
                    self.assertEqual(parsed[0], '/ABSOLUTE path $x;$(ignored)/usage-lens')
                    self.assertEqual(parsed[3], '/ABSOLUTE path $x;$(ignored)/usage.sqlite')

    def test_windows_mcp_json_and_powershell_examples_have_distinct_quoting(self):
        readme = (PLUGIN / 'README.md').read_text()
        blocks = re.findall(r'```json\n(.*?)\n```', readme, flags=re.S)
        self.assertEqual(len(blocks), 1)
        server = json.loads(blocks[0])
        self.assertTrue(PureWindowsPath(server['command']).is_absolute())
        self.assertTrue(server['command'].endswith('usage-lens.exe'))
        self.assertEqual(server['args'][:2], ['mcp', '--db'])
        self.assertTrue(PureWindowsPath(server['args'][2]).is_absolute())
        self.assertNotIn('"', server['command'])
        self.assertIn("& 'C:\\ABSOLUTE\\usage-lens.exe' source", readme)
        self.assertIn("& 'C:\\ABSOLUTE\\usage-lens.exe' hook", readme)
        self.assertIn('Confirm which shell the host uses', readme)

    def test_plugin_stays_inert(self):
        manifest = json.loads((PLUGIN / '.codex-plugin/plugin.json').read_text())
        for automatic_setup in ['hooks', 'mcpServers', 'mcp_servers', 'scripts']:
            self.assertNotIn(automatic_setup, manifest)
        self.assertEqual(manifest['interface']['capabilities'], [])
        self.assertFalse((PLUGIN / 'hooks/hooks.json').exists())
        self.assertFalse((PLUGIN / '.mcp.json').exists())

    def test_record_import_names_real_rust_api_and_tests(self):
        document = (ROOT / 'docs/record-import.md').read_text()
        self.assertIn('usage_lens::adapters::rollout::parse_rollout', document)
        self.assertIn('UsageStore::import_rollout', document)
        self.assertIn('importedAt', document)
        test_path = 'crates/usage-lens/tests/rollout.rs'
        self.assertIn(test_path, document)
        self.assertTrue((ROOT / test_path).is_file())
        self.assertNotIn('parseRollout', document)
        for command in ['source', 'settings', 'import-rollout']:
            self.assertIn(f'usage-lens {command}', document)
            self.assertIn(f"& 'C:\\ABSOLUTE\\usage-lens.exe' {command}", document)


if __name__ == '__main__':
    unittest.main()
