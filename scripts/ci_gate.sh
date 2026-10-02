#!/usr/bin/env bash
# Linux source gate; native cross-platform and real browser jobs also required.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -n "${COVERAGE_BASE:-}" ]]; then
  base="$COVERAGE_BASE"
elif [[ "${CI:-}" == true ]]; then
  echo 'COVERAGE_BASE is required in CI; refusing an unknown diff baseline' >&2
  exit 2
else
  base=$(git hash-object -t tree --stdin </dev/null)
fi
python3 scripts/check_static.py
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/build_ui.py
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets --features usage-lens-ui/ssr -- -D warnings
cargo clippy --locked -p usage-lens-ui --lib --target wasm32-unknown-unknown --no-default-features --features csr -- -D warnings
mkdir -p coverage/rust
cargo llvm-cov --locked --workspace --all-targets --features usage-lens-ui/ssr --lcov --output-path coverage/rust/lcov.info
cargo build --locked -p usage-lens
args=(--base "$base" --report coverage/rust/lcov.info . --total-min 95 --changed-min 95 --json-output coverage/gate-summary.json)
if [[ "${CI:-}" != true ]]; then args+=(--working-tree); fi
python3 scripts/check_coverage.py "${args[@]}"
