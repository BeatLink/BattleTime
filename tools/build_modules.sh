#!/usr/bin/env bash
# Builds every native module under battletime/modules and packs each into dist/modules/<name>, the file
# the companion puts at /system/modules/<name>. Run it inside `nix develop`.

set -euo pipefail
cd "$(dirname "$0")/../battletime"

mkdir -p dist/modules
(cd modules && cargo build -q --release)
for manifest in modules/*/Cargo.toml; do
  crate=$(sed -n 's/^name = "\(.*\)"/\1/p' "$manifest" | head -1)
  name=${crate#battletime-module-}
  python3 -I ../tools/package_module.py "modules/target/thumbv7em-none-eabihf/release/$crate" "$name" -o "dist/modules/$name"
done
