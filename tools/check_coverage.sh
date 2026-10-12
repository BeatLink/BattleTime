#!/usr/bin/env bash
# Runs every host test with coverage and fails if line coverage drops below its floor.
# Run it inside `nix develop`; CI runs the same script, so raise a floor whenever coverage rises.

set -euo pipefail
cd "$(dirname "$0")/.."

rust_minimum_line_percent=89

# The two command-line entry points only parse arguments and open windows, so they are left out.
untested_entry_points='(compiler|simulator)/src/main\.rs$'

echo "rust: cargo llvm-cov over the workspace (at least $rust_minimum_line_percent% of lines)"
cargo llvm-cov --workspace \
  --ignore-filename-regex "$untested_entry_points" \
  --lcov --output-path target/coverage-rust.lcov
cargo llvm-cov report \
  --ignore-filename-regex "$untested_entry_points" \
  --summary-only --fail-under-lines "$rust_minimum_line_percent"

# The TypeScript floors live in vite.config.ts, under coverage.thresholds.
echo "typescript: vitest over the apps, faces and SDK"
npm run coverage
