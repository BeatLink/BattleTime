# BattleTime's copy of trouble-host-macros

This is [trouble-host-macros](https://github.com/embassy-rs/trouble) 0.6.0 from crates.io
(Apache-2.0 OR MIT), with changes in `src/server.rs`, `src/service.rs`, `src/uuid.rs` and
`src/characteristic.rs`, marked "BattleTime patch". They build the attribute table in a constant, so
it lives in flash; see `third_party/trouble-host/BATTLETIME.md`.

- `#[gatt_service]` generates a `const fn build` that adds the service's attributes to an
  `AttributeTableBuilder` and returns the service with its handles. A `&'static` read-only
  characteristic or descriptor becomes a constant attribute in flash; every other one is a stored
  value in RAM, starting from its `value =` or its type's default. `ATTRIBUTE_COUNT` and
  `CCCD_COUNT` count each characteristic under its `cfg`. A service may hold only characteristics,
  since it is built in a constant.
- `#[gatt_server]` builds the whole table in a constant (`__<SERVER>_LAYOUT`), keeps its attributes in a
  `static` (`__<SERVER>_ATTRIBUTES`) and its stored values in a `ConstStaticCell`
  (`__<SERVER>_STORED_VALUES`, `__<SERVER>_STORED_BYTES` long), so a server can be built only once.
  Each service gets its own constant, so building the server never copies the layout out of flash.
- `new_default`, `new_with_config` and `init_in_place`, which builds the server straight into a
  `MaybeUninit` through `AttributeServer::init_in_place`, all take the GAP service's name and
  appearance from a `GapConfig`. `new(table)`, which took a table built at run time, is gone.
- UUIDs given as expressions go through `trouble_host::types::uuid::ConstUuid`, so they work in a
  constant.

The workspace and the firmware swap this copy in with `[patch.crates-io]`. Drop it, with the
trouble-host copy, if upstream gains a table built at compile time.
