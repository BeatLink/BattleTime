"""Finds a symbol's address in the firmware's ELF file, so a suite can poke a static or jump to a function."""

import struct


def symbol_address(path, name_part):
    """The address of the one symbol whose name holds `name_part`, with the Thumb bit cleared."""
    with open(path, "rb") as file:
        data = file.read()
    if data[:4] != b"\x7fELF" or data[4] != 1:
        raise ValueError(f"{path} is not a 32-bit ELF file")
    section_offset, = struct.unpack_from("<I", data, 0x20)
    section_size, section_count, names_index = struct.unpack_from("<HHH", data, 0x2E)
    sections = [struct.unpack_from("<IIIIIIIIII", data, section_offset + index * section_size) for index in range(section_count)]
    matches = set()
    for section in sections:
        kind, offset, size, link, entry_size = section[1], section[4], section[5], section[6], section[9]
        if kind != 2:
            continue
        strings_offset = sections[link][4]
        for place in range(offset, offset + size, entry_size):
            name_offset, value = struct.unpack_from("<II", data, place)
            end = data.index(b"\0", strings_offset + name_offset)
            name = data[strings_offset + name_offset:end].decode("utf-8", "replace")
            if name_part in name and value:
                matches.add(value & ~1)
    if len(matches) != 1:
        raise ValueError(f"{len(matches)} symbols match {name_part}")
    return matches.pop()
