#!/usr/bin/env bash
# Compiles every app and face, then compares ekv and sequential-storage as the store for them; run inside nix develop.
set -euo pipefail

repository="$(cd "$(dirname "$0")/.." && pwd)"
benchmark="$repository/tools/storage-benchmark"
apps="$repository/neotime/target/storage-benchmark-apps"
days="${DAYS:-120}"

# ============================================================================
# Every app and face, compiled as the watch installs them
# ============================================================================

cargo build -q -p neotime-compiler --manifest-path "$repository/neotime/Cargo.toml"
mkdir -p "$apps"
for source in "$repository"/neotime/apps/*/app.tsx "$repository"/neotime/faces/*/face.tsx; do
    app_id="$(basename "$(dirname "$source")")"
    [ "$app_id" = template ] && continue
    "$repository/neotime/target/debug/neotime-compiler" "$source" -o "$apps/$app_id.ntapp" >/dev/null
done

# ============================================================================
# Runs
# ============================================================================

run() {
    local chunk="$1"
    shift
    # Cargo reads ekv's settings from the benchmark's .cargo/config.toml only when run from its folder.
    (cd "$benchmark" && EKV_MAX_CHUNK_SIZE="$chunk" cargo run -q --release --target-dir "target/chunk-$chunk" \
        -- "$apps" --days "$days" "$@")
}

echo "# Apps split into keys"
run 4096 --only-littlefs
run 4096 --only-sequential | sed -n '/^| sequential/p'
for chunk in 4096 1024 256; do
    run "$chunk" --only-ekv | sed -n '/^| ekv/p'
done

echo
echo "# Each app as one whole image, in 3 KB parts"
run 256 --whole-images

echo
echo "# Grouped: each app one value, its state one value, settings and notifications one value each"
run 256 --grouped --only-ekv

run 256 --grouped --only-custom | sed -n '/^| custom/p; /most erases/p'
for branching in 3 4; do
    (cd "$benchmark" && EKV_BRANCHING_FACTOR="$branching" EKV_MAX_CHUNK_SIZE=256 cargo run -q --release \
        --target-dir "target/branching-$branching" -- "$apps" --days "$days" --grouped --only-ekv) |
        sed -n "/^| ekv/s/^| ekv/| ekv, branching factor $branching,/p"
done

echo
echo "# The custom store with a power cut at every write and erase"
run 256 --power-cut-test

echo
echo "# Only the data that changes, in a 512 KB store"
run 256 --saved-state-only --flash-kib 512

# ============================================================================
# Code size on the watch: the probe built with each store, less the plain build
# ============================================================================

echo
echo "# Code size on the watch"
probe="$benchmark/size-probe"
llvm_size="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-size"
plain=0
for store in plain ekv sequential littlefs flash-store; do
    features=""
    [ "$store" != plain ] && features="--features $store"
    # shellcheck disable=SC2086
    (cd "$probe" && cargo build -q --release $features --target-dir "target/$store")
    text="$("$llvm_size" "$probe/target/$store/thumbv7em-none-eabihf/release/neotime-storage-size-probe" | awk 'NR == 2 { print $1 }')"
    [ "$store" = plain ] && plain="$text"
    echo "$store: $text B of code and constants, $((text - plain)) B more than the plain build"
done
