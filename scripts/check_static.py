#!/usr/bin/env python3
"""Small fail-closed source guardrails, not a complete security scanner.

Local collection and same-origin HTTP queries are intentional. These checks do
not forbid raw body fields: privacy boundaries belong in explicit validated
projections, local-only detail routes, and their executable security tests.
"""
from pathlib import Path
import json
import re
import sys
import tomllib

from check_coverage import rust_module_only

RULES = {
    'dynamic code execution': r'\beval\s*\(|\bnew\s+Function\s*\(',
    'unsafe HTML insertion': r'\bv-html\s*=|\.\s*(?:innerHTML|outerHTML)\s*=|\binsertAdjacentHTML\s*\(',
    'disabled type safety': r'@ts-(?:ignore|nocheck)|\bas\s+any\b|:\s*any\b',
    'coverage suppression': r'(?:v8|c8|istanbul)\s+ignore\b|coverage\s*\(\s*off\s*\)|cfg\s*\(\s*(?:not\s*\(\s*)?coverage(?:_nightly)?\b',
    'embedded private key': r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
}
EXTENSIONS = {'.ts', '.tsx', '.js', '.jsx', '.mts', '.cts', '.mjs', '.cjs', '.vue'}
OWNED_CARGO_PACKAGES = ('usage-lens', 'usage-lens-ui')
OWNED_JSON_VERSIONS = ('package.json', 'plugin/usage-lens/.codex-plugin/plugin.json')


def check_versions(root: Path) -> list[str]:
    """Only owned product identities track the release; dependencies do not."""
    versions = {}
    try:
        for name in OWNED_CARGO_PACKAGES:
            path = f'crates/{name}/Cargo.toml'
            package = tomllib.loads((root / path).read_text(encoding='utf-8'))['package']
            if package['name'] != name:
                raise ValueError('Unexpected owned Cargo package name')
            versions[path] = package['version']
        for path in OWNED_JSON_VERSIONS:
            package = json.loads((root / path).read_text(encoding='utf-8'))
            if package['name'] != 'usage-lens':
                raise ValueError('Unexpected owned JSON package name')
            versions[path] = package['version']
        lock = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf-8'))['package']
        if not isinstance(lock, list) or not all(isinstance(package, dict) for package in lock):
            raise ValueError('Expected Cargo.lock package entries to be an array of tables')
        for name in OWNED_CARGO_PACKAGES:
            matches = [package for package in lock if package.get('name') == name]
            if len(matches) != 1 or 'source' in matches[0]:
                raise ValueError('Expected one workspace lockfile entry per owned package')
            versions[f'Cargo.lock:{name}'] = matches[0]['version']
        expected = versions['crates/usage-lens/Cargo.toml']
        if not isinstance(expected, str) or not re.fullmatch(r'(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)', expected):
            raise ValueError('Expected a stable product SemVer')
        return [f'{path}: owned product version must match {expected}'
                for path, actual in versions.items() if actual != expected]
    except (OSError, UnicodeError, ValueError, TypeError, KeyError) as exc:
        return [f'Owned product version inventory cannot be validated: {exc}']


def inspect_source(text: str) -> list[tuple[int, str]]:
    return sorted((text.count('\n', 0, match.start()) + 1, name)
                  for name, pattern in RULES.items()
                  for match in re.finditer(pattern, text))


def check(root: Path) -> list[str]:
    sources = sorted(path for path in (root / 'src').rglob('*') if path.suffix in EXTENSIONS)
    sources += sorted(path for path in (root / 'crates').glob('*/src/**/*.rs') if path.is_file())
    sources += sorted(path for path in (root / 'crates/usage-lens-ui/public').rglob('*') if path.suffix in EXTENSIONS)
    if not sources:
        return ['No application source files found; static checks cannot pass an empty application']
    failures = []
    for path in sources:
        try:
            content = path.read_text(encoding='utf-8')
        except (OSError, UnicodeError) as exc:
            failures.append(f'{path.relative_to(root)}: unreadable source: {exc}')
            continue
        failures.extend(f'{path.relative_to(root)}:{line}: {reason}' for line, reason in inspect_source(content))
        relative = path.relative_to(root).as_posix()
        if relative.startswith('crates/usage-lens-ui/src/') and path.name != 'browser.rs':
            if re.search(r'#\s*\[\s*cfg(?:_attr)?\b|\bcfg!\s*\(', content) and not rust_module_only(content):
                failures.append(f'{relative}: feature/target-gated UI runtime outside the conservatively accounted browser bridge')
    return failures


def main(root: Path | None = None) -> int:
    root = root or Path(__file__).resolve().parents[1]
    failures = check(root) + check_versions(root)
    for failure in failures:
        print(f'ERROR: {failure}', file=sys.stderr)
    if failures:
        return 1
    print('Static unsafe-code, type-suppression, coverage-suppression, and owned-version checks passed')
    return 0


if __name__ == '__main__':
    sys.exit(main())
