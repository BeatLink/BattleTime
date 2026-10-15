# Flash store

`neotime-flash-store` (`services/flash_store/`) keeps NeoTime's data in the external flash; what the
watch keeps there, and how the firmware reaches it, is in [storage.md](storage.md). Why it exists,
and how it compares with littlefs, ekv and sequential-storage, is in
[architecture-assessment.md](architecture-assessment.md#apps-as-chunks-in-flash).

The store knows how data is written, not what it is, so a new kind of data needs no new format and
no new firmware. It survives a power cut at any point, spreads its wear, is `async` over
`embedded-storage-async`, and keeps fixed tables; it borrows 4 KB of heap while it mounts, and a
file written a part at a time holds about 200 B of heap while it is written.

## The three containers

| Container | For | Examples |
|---|---|---|
| Files | Data named by path, written whole, read whole or from an offset; blank files programmed in place | app images, the note, `/remote/buttons`, the step, heart rate and battery blocks |
| Records | Small values, up to 2,112 bytes, rewritten often | every file of 1 KB or less, such as the settings or an app's saved state |
| Logs | Data appended over time, with the oldest dropped | nothing yet; kept for recordings too long to rewrite whole |

The firmware reaches all of it through paths: the files API keeps a small file as a record by
itself, and the step, heart rate and battery logs are blank files programmed a slot at a time
([storage.md](storage.md#logs)), so nothing in the firmware calls the record or log APIs directly.

### Files

A file is a list of up to 64 sectors of 4 KB, in any order, so up to 256 KB, and the file area
never fragments. The first sector starts with a 224-byte header: the path (up to 64 bytes), the
length, a checksum of the contents, the sector list and a checksum of the header. A write puts the
data into free sectors first and the header last, so until the header is whole the new version
does not exist, and a power cut leaves only unused sectors. A newer version retires the older by
writing zeros into a word of its header, which needs no erase.

`write_file` writes a whole file at once. `begin_file`, `write_file_part` and `finish_file` write
one a part at a time, as a Bluetooth upload arrives; `abandon_file` drops an unfinished upload. One
upload runs at a time.

A whole file of 1 KB or less is kept as a record instead, under the reserved owner `0xffe`, with
its path stored before its data, so it costs no sector. When a file moves between the two forms,
the new copy is written before the old is removed; files and records share one sequence of numbers,
so while both exist the newer is read.

Paths are found through a table in RAM of 16 bits of each path's hash and the file's first sector;
the header's full path settles any clash. The store keeps the last header it read, parsed, so
reading a file again, or asking its length, reads only its data; any file retired, deleted or
replaced, and every mount, drops it. A small file's record header and path come in one read.
`for_each_file` lists every path under a prefix, and
`for_each_file_with_length` gives each one's length too.

`program_file` writes up to 4 bytes into a blank file in place, making the file first when it is
missing: one sector of erased bytes, 3,872 long after its header. Programming only clears bits, so
it needs no erase, and bytes already written are left as they are. A blank file's header carries
the checksum of no bytes, which a whole read does not check, since its contents change after the
header is written. A power cut while programming spoils at most the bytes being written. The step,
heart rate and battery logs keep their days this way, so a slot every six minutes costs no erase.

`read_file` reads a file whole or from an offset and checks the contents' checksum when it reads
the whole file. `file_extents` instead says where a part of a file lies in the flash, as up to a
given number of runs of addresses, so a caller can read it itself, such as by DMA while the screen
draws; it gives none for a file kept as a record or one that needs more runs, and bytes read this
way skip the checksum.

### Records

A record key is a `u32`: the owner's app number in the top 12 bits, and 20 bits the owner chooses.
Owners from `0xf00` up are the watch's own. Records are appended to a ring of sectors, each with its
key, a sequence number, its length and flags, its data, and a checksum after the data, so a record a
power cut cut short fails its checksum. A table in RAM, sorted by key, holds where each key's
newest copy lies, in 8 bytes.

When the ring needs space it cleans one sector: it copies that sector's newest copies forward,
keeping their sequence numbers, then erases it at once. It picks the sector that gives the most
space back for its age, so rarely changed records are not copied on every lap but every sector is
cleaned in time. Two sectors are always kept free, so cleanup can finish even after a power cut
spoiled one. `prepare_room`, for an idle moment, erases the sector each ring takes next and, for
records, cleans a sector early when starting that one would use the reserve, so a later write only
programs pages. A deletion writes a tombstone, which is dropped during cleanup once no older copy of
its key is left anywhere.

Writes are refused before the ring gets too full to clean well:

| Refusal | When |
|---|---|
| `Full` | The newest copies would take more than half the ring's usable space |
| `OverOwnerLimit` | An app's records would take more than 4 KB of flash; the watch's own owners have no limit |
| `KeyTableFull` | A new key, with the key table full |
| `TooLarge` | A record over 2,112 bytes; larger values belong in a file |

### Logs

A log entry is a key in the log ring. `append_log` adds bytes to it,
batched in RAM: up to 4 streams of 64 bytes each, written out when a batch fills, when a fifth
stream starts, or on `flush_logs`. A power cut loses at most what has not been flushed. When the
log ring needs space it drops and erases its oldest sector, so the oldest entries lose their start
first. `read_log` reads an entry back by scanning the ring, with what is still batched.

Raw accelerometer recordings, about 13 MB a day, need a ring of their own, so that a recording
cannot push out months of history.

## On the flash

`Layout` gives the store's start address and how many sectors each container gets. On the watch
it is the 3 MB file system region: 512 sectors (2 MB) of files, 64 (256 KB) of records and
192 (768 KB) of logs.

`FlashStore::unmounted` builds a store with nothing read yet, which `mount_in_place` fills where it
lies, such as in a static, so the mount's future never holds a copy of the store; `mount` does both.
The flash must read and write single bytes and erase 4 KB sectors, which the build checks.
`flash_mut` lends the flash driver, such as for the firmware to switch how it waits.

Every ring sector starts with a 16-byte header: magic, sequence number, erase count, format version
and checksum. Every file header carries the format version too. A store of another version is
refused with `OtherFormatVersion` rather than overwritten, so a future firmware migrates it
deliberately. A flash read error is returned, never treated as blank, so a bus fault cannot wipe
the watch.

## Size

Built for the watch with 128 files, 128 record keys and rings of up to 192 sectors
(`FlashStore<Flash, 128, 128, 192>`):

| | Bytes |
|---|---:|
| RAM, fixed when built, no heap | 2,744 |
| Code and constants | about 26,200 |
| Stack, at most, while cleaning a ring | about 1,000 |

The RAM is mostly the key table (1 KB), the file table (512 B), the rings' sector order (384 B),
the last file header read (about 220 B), the log batches and a map of the 512 file sectors (64 B), the most the file area may have. An
upload's state, its path and sector list, is boxed on the heap from `begin_file` to its finish or
abandonment, and a heap without room answers `OutOfMemory`. littlefs takes 23.7 KB of code for comparison. The
firmware's measured share is in [memory-map.md](memory-map.md).

An `async` call's future holds every buffer of every call beneath it, and an Embassy task keeps
its future in RAM for good. So the firmware awaits the store in one task only, and builds and
mounts the store outside any `async` function.

Mounting reads every file header and checks every record in both rings, about 1 MB of reads,
roughly a second at start-up; a checkpoint of the RAM tables could shorten it later. Each used ring
sector is read whole, in one read, into a 4 KB buffer on the heap that lives only while the store
mounts, and its records are checked from there.

## Tests

`src/tests.rs` covers each container, every refusal, the format version, read errors and a
corrupted file. `src/power_cut_tests.rs` runs a scripted mix of record writes, whole and streamed
files that move between small and large, deletes and log appends, in which both rings wrap. It cuts
the power at every single write and erase of the run in turn, leaving that one half done, mounts
again and checks that every record, file, listing and the log is as before the interrupted
operation or as it left it (an older log entry may only have lost its start). Each cut is repeated
with a second cut during the recovery. A second run programs a blank file a slot at a time and
cuts the power at every write, checking that only the slot being written can be spoiled.
