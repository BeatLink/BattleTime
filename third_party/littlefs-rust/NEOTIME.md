# NeoTime's copy of littlefs-rust

This is [littlefs-rust](https://github.com/light-player/littlefs-rust) 0.1.0 from crates.io
(BSD-3-Clause), with one change: it depends on `littlefs-rust-core` without that crate's default
`loop_limits` feature.

`loop_limits` stops with a panic once a loop runs more times than a fixed cap that the C littlefs
does not have. One cap is 256 tags per metadata block. A file system written by the C littlefs,
with about 40 files in one folder, holds more than that in one 4 KB block, so mounting it panicked.
Files written by littlefs-rust itself have not tripped it in tests, but images built on a computer
with the usual littlefs tools are C-written, and on the watch a panic at start means a reset loop.

The workspace `Cargo.toml` swaps this copy in for the crates.io one with `[patch.crates-io]`.
Drop it once upstream turns `loop_limits` off by default or lets users opt out.
