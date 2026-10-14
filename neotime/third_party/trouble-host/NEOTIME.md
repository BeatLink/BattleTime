# NeoTime's copy of trouble-host

This is [trouble-host](https://github.com/embassy-rs/trouble) 0.8.0 from crates.io (Apache-2.0 OR
MIT), with two changes in `src/attribute_server.rs`, each marked "NeoTime patch".

The attribute server builds a Read By Type or Read By Group Type response by reading each matching
attribute straight into the space left in the response and comparing the length it got with the
first entry's. When the space left is exactly the first entry's length, a longer attribute is cut
to fit and passes as equal. On the watch this sent the legacy DFU control point's 128-bit UUID cut
to 16 bits (`0000d123-…`), so the update tools could not find it. The fix reads each later entry
into a scratch buffer one byte longer than the first, so a longer attribute shows as a different
length and starts the next response instead.

`AttributeServer::init_in_place` builds a server straight into its static. Built by value, the
watch's server (2.5 KB) was copied on its way there through a 6.9 KB stack frame, the deepest the
firmware reaches; `third_party/trouble-host-macros` generates an `init_in_place` for each GATT server
that uses it.

The test and example targets are left out of this copy, since their sources are not included.
The workspace and the firmware swap this copy in with `[patch.crates-io]`. Drop it once upstream
has the fix.
