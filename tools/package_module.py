#!/usr/bin/env python3
"""Packs a native module's ELF, linked at 0 with --emit-relocs, into the module file the firmware runs.

The layout matches `battletime_module_abi`: magic "NTMD", format and ABI version, the module's name,
the image's length (code, read-only data and data), the zeroed data after it, the code's length,
the entry's offset, the relocation count, the stack the module asks for and a CRC-32 of everything
after the 52-byte header. A module asks for a stack of its own with a `MODULE_STACK_BYTES` static,
a multiple of 8; without one it runs on the firmware's. Then
come the image and one word per relocation: the offset of a 32-bit address, or, with the top bit
set, the offset of a MOVW whose MOVT's offset is the next word. PC-relative relocations need no
moving and are left out; any other kind is refused, as the firmware could not place it.

    python3 -I tools/package_module.py ELF NAME -o FILE
"""

import argparse
import struct
import sys
import zlib
from pathlib import Path

MAGIC = b"NTMD"
FORMAT_VERSION = 2
ABI_VERSION = 2
NAME_BYTES = 16
HEADER = struct.Struct("<4sHH16sIIIIIII")
STACK_SYMBOL = "MODULE_STACK_BYTES"
ALIGNMENT_BYTES = 8
MOVE_PAIR_BIT = 1 << 31

SHT_PROGBITS = 1
SHT_SYMTAB = 2
SHT_NOBITS = 8
SHT_REL = 9
SHF_ALLOC = 2
SHN_ABS = 0xFFF1

R_ARM_ABS32 = 2
R_ARM_TARGET1 = 38
R_ARM_THM_MOVW_ABS_NC = 47
R_ARM_THM_MOVT_ABS = 48
# Relocations relative to the instruction itself, which stay right wherever the module lands.
PC_RELATIVE = {0, 3, 10, 11, 30, 40, 42, 51, 53, 54, 102, 103}


# ============================================================================
# Reading the ELF
# ============================================================================


class Section:
    def __init__(self, elf: bytes, at: int, names_at: int | None):
        (
            self.name_at,
            self.kind,
            self.flags,
            self.address,
            self.offset,
            self.size,
            self.link,
            self.info,
            self.alignment,
            self.entry_size,
        ) = struct.unpack_from("<IIIIIIIIII", elf, at)
        self.name = ""
        if names_at is not None:
            end = elf.index(b"\0", names_at + self.name_at)
            self.name = elf[names_at + self.name_at : end].decode()

    def data(self, elf: bytes) -> bytes:
        return elf[self.offset : self.offset + self.size]


def sections_of(elf: bytes) -> list[Section]:
    if elf[:4] != b"\x7fELF" or elf[4] != 1 or elf[5] != 1:
        sys.exit("not a 32-bit little-endian ELF")
    table_at, = struct.unpack_from("<I", elf, 0x20)
    entry_size, count, names_index = struct.unpack_from("<HHH", elf, 0x2E)
    names = Section(elf, table_at + names_index * entry_size, None)
    return [Section(elf, table_at + index * entry_size, names.offset) for index in range(count)]


def symbol_value(elf: bytes, sections: list[Section], wanted: str) -> int | None:
    """The address of the symbol named `wanted`, or none."""
    table = next(section for section in sections if section.kind == SHT_SYMTAB)
    names = sections[table.link]
    data = table.data(elf)
    for at in range(0, len(data), 16):
        name_at, value = struct.unpack_from("<II", data, at)
        start = names.offset + name_at
        if elf[start : elf.index(b"\0", start)].decode(errors="replace") == wanted:
            return value
    return None


def symbol_sections(elf: bytes, sections: list[Section]) -> list[int]:
    """Each symbol's section index, by symbol number."""
    table = next(section for section in sections if section.kind == SHT_SYMTAB)
    data = table.data(elf)
    return [struct.unpack_from("<IIIBBH", data, at)[5] for at in range(0, len(data), 16)]


# ============================================================================
# Packing
# ============================================================================


def relocation_words(elf: bytes, sections: list[Section], loaded: set[int]) -> list[int]:
    """The offsets to move, MOVW and MOVT paired by the symbol they load."""
    symbols = symbol_sections(elf, sections)
    words: list[int] = []
    moves_low: list[tuple[int, int]] = []
    moves_high: list[tuple[int, int]] = []
    for section in sections:
        if section.kind != SHT_REL or section.info not in loaded:
            continue
        data = section.data(elf)
        for at in range(0, len(data), 8):
            offset, info = struct.unpack_from("<II", data, at)
            kind, symbol = info & 0xFF, info >> 8
            if kind in PC_RELATIVE or symbols[symbol] == SHN_ABS:
                continue
            if kind in (R_ARM_ABS32, R_ARM_TARGET1):
                words.append(offset)
            elif kind == R_ARM_THM_MOVW_ABS_NC:
                moves_low.append((offset, symbol))
            elif kind == R_ARM_THM_MOVT_ABS:
                moves_high.append((offset, symbol))
            else:
                sys.exit(f"relocation kind {kind} at {offset:#x} in {section.name} cannot be placed")
    for low, symbol in sorted(moves_low):
        partners = [high for high in moves_high if high[1] == symbol]
        if not partners:
            sys.exit(f"the MOVW at {low:#x} has no MOVT")
        following = [high for high in partners if high[0] > low] or partners
        nearest = min(following, key=lambda high: abs(high[0] - low))
        moves_high.remove(nearest)
        words += [low | MOVE_PAIR_BIT, nearest[0]]
    if moves_high:
        sys.exit(f"the MOVT at {moves_high[0][0]:#x} has no MOVW")
    return words


def package(elf: bytes, name: str) -> bytes:
    sections = sections_of(elf)
    entry, = struct.unpack_from("<I", elf, 0x18)
    loaded = {index for index, section in enumerate(sections) if section.flags & SHF_ALLOC and section.size}
    for index in loaded:
        if sections[index].alignment > ALIGNMENT_BYTES:
            sys.exit(f"{sections[index].name} needs {sections[index].alignment}-byte alignment")
    filled = [sections[index] for index in loaded if sections[index].kind == SHT_PROGBITS]
    zeroed = [sections[index] for index in loaded if sections[index].kind == SHT_NOBITS]
    image_bytes = max((section.address + section.size for section in filled), default=0)
    ram_bytes = max([image_bytes] + [section.address + section.size for section in zeroed])
    text = next(section for section in sections if section.name == ".text")
    if text.address != 0:
        sys.exit(".text must start at 0")
    image = bytearray(image_bytes)
    for section in filled:
        image[section.address : section.address + section.size] = section.data(elf)
    relocations = relocation_words(elf, sections, loaded)
    stack_at = symbol_value(elf, sections, STACK_SYMBOL)
    stack_bytes = 0 if stack_at is None else struct.unpack_from("<I", image, stack_at)[0]
    if stack_bytes % 8:
        sys.exit(f"{STACK_SYMBOL} must be a multiple of 8")

    encoded_name = name.encode()
    if not 0 < len(encoded_name) <= NAME_BYTES:
        sys.exit(f"a module name is 1 to {NAME_BYTES} bytes")
    body = bytes(image) + b"".join(struct.pack("<I", word) for word in relocations)
    header = HEADER.pack(
        MAGIC,
        FORMAT_VERSION,
        ABI_VERSION,
        encoded_name.ljust(NAME_BYTES, b"\0"),
        image_bytes,
        ram_bytes - image_bytes,
        text.size,
        entry,
        len(relocations),
        stack_bytes,
        zlib.crc32(body),
    )
    return header + body


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("elf", type=Path)
    parser.add_argument("name")
    parser.add_argument("-o", "--output", type=Path, required=True)
    arguments = parser.parse_args()
    module = package(arguments.elf.read_bytes(), arguments.name)
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_bytes(module)
    print(f"{arguments.name}: {len(module)} bytes")


if __name__ == "__main__":
    main()
