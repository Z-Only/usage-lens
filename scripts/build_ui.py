#!/usr/bin/env python3
"""Build Leptos browser assets with pinned Rust/wasm-bindgen, without Node.

The output is embedded by the native binary's build.rs. The browser's generated
JavaScript loader is served locally; Node and Bun are not end-user prerequisites.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

WASM_BINDGEN_VERSION = '0.2.129'


def build(root: Path, run=subprocess.run) -> None:
    crate = root / 'crates/usage-lens-ui'
    for name in ['Cargo.toml', 'index.html', 'public/styles.css', 'public/bootstrap.js']:
        if not (crate / name).is_file():
            raise RuntimeError(f'Missing Leptos source asset: {crate / name}')
    version = run(['wasm-bindgen', '--version'], check=True, capture_output=True, text=True).stdout.strip()
    if version != f'wasm-bindgen {WASM_BINDGEN_VERSION}':
        raise RuntimeError(f'Expected wasm-bindgen {WASM_BINDGEN_VERSION}, found {version!r}')
    run(['cargo', 'build', '--locked', '-p', 'usage-lens-ui', '--lib', '--target',
         'wasm32-unknown-unknown', '--no-default-features', '--features', 'csr', '--release'],
        cwd=root, check=True)
    wasm = root / 'target/wasm32-unknown-unknown/release/usage_lens_ui.wasm'
    if not wasm.is_file() or wasm.read_bytes()[:4] != b'\0asm':
        raise RuntimeError('The frontend compiler did not produce a valid WebAssembly module')
    dist = root / 'dist'
    dist.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='leptos-build-', dir=dist) as temporary:
        output = Path(temporary)
        run(['wasm-bindgen', '--target', 'web', '--no-typescript', '--out-dir', str(output),
             '--out-name', 'usage_lens_ui', str(wasm)], cwd=root, check=True)
        for name in ['usage_lens_ui.js', 'usage_lens_ui_bg.wasm']:
            if not (output / name).is_file():
                raise RuntimeError(f'wasm-bindgen did not produce {name}')
        shutil.copyfile(crate / 'index.html', output / 'index.html')
        shutil.copyfile(crate / 'public/styles.css', output / 'styles.css')
        shutil.copyfile(crate / 'public/bootstrap.js', output / 'bootstrap.js')
        hashes = {path.relative_to(output).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
                  for path in sorted(output.rglob('*')) if path.is_file()}
        (output / 'frontend-build.json').write_text(json.dumps({
            'schemaVersion': 1, 'framework': 'leptos', 'frameworkVersion': '0.8.21',
            'wasmBindgenVersion': WASM_BINDGEN_VERSION, 'assets': hashes,
        }, indent=2) + '\n', encoding='utf-8')
        destination = dist / 'ui'
        if destination.exists():
            shutil.rmtree(destination)
        shutil.copytree(output, destination)
    print('Built Leptos WebAssembly and local assets in dist/ui')


if __name__ == '__main__':
    build(Path(__file__).resolve().parents[1])
