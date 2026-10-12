#!/usr/bin/env bash
# Compiles the watch face, the apps and the shared library that the firmware builds in, into
# firmware/apps/. Run it inside `nix develop` after changing any of them, and commit the results.

set -euo pipefail
cd "$(dirname "$0")/.."

face=digital
apps=(stopwatch timer flashlight dice calculator snake puzzle15 steps heart settings)
out=firmware/apps

rm -rf "$out"
mkdir -p "$out"
cargo build -q -p neotime-compiler
compiler=target/debug/neotime-compiler

"$compiler" "faces/$face/face.tsx" -o "$out/face.ntapp"
"$compiler" "apps/launcher/app.tsx" -o "$out/launcher.ntapp"
for app in "${apps[@]}"; do
  "$compiler" "apps/$app/app.tsx" -o "$out/$app.ntapp"
done
"$compiler" library -o "$out/shared.ntlib"

# The firmware's list of apps, in launcher order.
{
  echo "// Written by tools/build_watch_apps.sh."
  echo "pub static FACE: &[u8] = include_bytes!(\"../apps/face.ntapp\");"
  echo "pub static LAUNCHER: &[u8] = include_bytes!(\"../apps/launcher.ntapp\");"
  echo "pub static LIBRARY: &[u8] = include_bytes!(\"../apps/shared.ntlib\");"
  echo "pub static APPS: [&[u8]; ${#apps[@]}] = ["
  for app in "${apps[@]}"; do
    echo "    include_bytes!(\"../apps/$app.ntapp\"),"
  done
  echo "];"
} > firmware/src/built_in_apps.rs
