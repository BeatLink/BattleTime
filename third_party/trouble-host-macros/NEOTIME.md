# NeoTime's copy of trouble-host-macros

This is [trouble-host-macros](https://github.com/embassy-rs/trouble) 0.6.0 from crates.io
(Apache-2.0 OR MIT), with one change in `src/server.rs`, marked "NeoTime patch".

`#[gatt_server]` also generates `init_in_place`, which builds the server straight into a
`MaybeUninit` through `AttributeServer::init_in_place` in `third_party/trouble-host`, so the
attribute table is never copied through the stack. The workspace and the firmware swap this copy in
with `[patch.crates-io]`. Drop it if upstream gains a way to build a server in place.
