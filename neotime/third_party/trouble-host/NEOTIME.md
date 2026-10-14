# NeoTime's copy of trouble-host

This is [trouble-host](https://github.com/embassy-rs/trouble) 0.8.0 from crates.io (Apache-2.0 OR
MIT), with four changes, each marked "NeoTime patch".

The attribute server builds a Read By Type or Read By Group Type response by reading each matching
attribute straight into the space left in the response and comparing the length it got with the
first entry's. When the space left is exactly the first entry's length, a longer attribute is cut
to fit and passes as equal. On the watch this sent the legacy DFU control point's 128-bit UUID cut
to 16 bits (`0000d123-…`), so the update tools could not find it. The fix reads each later entry
into a scratch buffer one byte longer than the first, so a longer attribute shows as a different
length and starts the next response instead.

`AttributeServer::init_in_place` builds a server straight into its static, so it is never copied
through the stack; `third_party/trouble-host-macros` generates an `init_in_place` for each GATT
server that uses it.

The attribute table is built in a constant, so it lives in flash (`src/attribute.rs`). Upstream
builds it at start-up into a `heapless::Vec` of 44-byte attributes in RAM, 2,068 B for the watch's
47, with every service and characteristic declaration, UUID and constant value in it.
`AttributeTableBuilder` is a `const fn` builder the macros call instead, and its finished
`[Attribute; N]` becomes a `static`, 32 B an attribute. What changes at run time stays in RAM:

- **Stored values**, the characteristics and descriptors that are written or set, are kept in one
  byte array the builder lays out, with two bytes of length before each variable-length one. Each
  starts from what an `InitialValue` writes, the `value =` or the type's default, when the table is
  made, as upstream's builder copied it.
- **Borrowed values**, only the GAP service's device name and appearance, are lent by `GapConfig`
  when the server is built (`BORROWED_VALUE_SLOTS`).
- **CCCDs** stay in `ClientAttTables`, per client, as upstream keeps them.

A declaration's value is worked out when it is read: the properties, the next handle, which is
always the characteristic's value, and that value attribute's UUID. A service declaration keeps the
service's UUID in the attribute's own UUID slot and reports the declaration type, so neither needs
a second UUID. `&'static` read-only values are `Constant` attributes, a function returning their
bytes, and cost no RAM. Each read, write and discovery answer is the same, byte for byte, as from
upstream's table: `drivers/bluetooth/src/tests.rs` plays several hundred ATT requests against a
server laid out as the watch's and compares every answer with `src/gatt_transcript.txt`, which
upstream's table produced, and this crate's own tests still pass, the database hash among them.

The run-time builder API (`AttributeTable::add_service`, `ServiceBuilder`, `CharacteristicBuilder`,
`add_included_service`, `Service`) is gone, since nothing built at run time can be in flash, and so
is `GapConfig::build`; `gap::add_gap_attributes` adds the GAP and GATT services to a builder. The
crate's tests build their tables with `AttributeTableBuilder` through `attribute::test_table`.
`types::uuid::ConstUuid` turns the UUID expressions the macros accept into a `Uuid` in a constant,
and `__export` also exports `static_cell` for the macros.

`Connection::send_att` sends one ATT PDU the caller built, and `Connection::receive_att_from_server`
copies out the peer's next ATT response, notification or indication, from the queue upstream's
`GattClient` reads. They let the firmware's small client for the phone's own services
(`services/phone_client`) carry its requests without `GattClient`, which cost about 12.6 KB of
flash in the watch's firmware against about 3 KB for the small client.

The test and example targets are left out of this copy, since their sources are not included.
The workspace and the firmware swap this copy in with `[patch.crates-io]`. Drop it once upstream
has the fix.
