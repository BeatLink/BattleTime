#!/usr/bin/env python3
"""Builds what the companion's app store offers into battletime/dist/store/.

Every app in `battletime/apps` and face in `battletime/faces` is compiled to `<id>.ntapp`, and every plugin
in `companion/plugins` is packed into `<id>.ntplugin`. `store.json` lists them with a description
taken from the docs, and `battletime-store.zip` holds all of it in one file, which is what the companion
downloads from a release. The shared library is written beside them for the companion's bundle, but
stays out of the list and the zip because it ships inside the firmware.

Run it inside `nix develop`; plugins also need `npm ci` in `companion/`, or `--no-plugins`.
docs/app-store.md describes the result.
"""

import argparse
import hashlib
import json
import re
import shutil
import struct
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BATTLETIME = ROOT / "battletime"
COMPANION = ROOT / "companion"
DOCS = ROOT / "docs"

# The version of store.json; the companion refuses a list in a format it does not know.
STORE_FORMAT = 1

STORE_ZIP = "battletime-store.zip"
STORE_INDEX = "store.json"
LIBRARY_FILE = "shared.ntlib"

# Folders under apps/ that hold no app of their own: the starting point for new apps.
NOT_APPS = {"template"}

# A fixed date for every zip entry, so the same images always make the same zip.
ZIP_DATE = (1980, 1, 1, 0, 0, 0)


# ============================================================================
# Reading an image's header
# ============================================================================

# The layout is runtime/src/image.rs, as companion/src/protocol/app-image.ts reads it.
IMAGE_MAGIC = b"NTAP"
IMAGE_HEADER_BYTES = 20
SECTION_ENTRY_BYTES = 12
STRING_TABLE_ENTRY_BYTES = 4
STRINGS_SECTION = 1
LAUNCHER_SECTION = 11
IMAGE_KINDS = ["app", "face", "library"]


def read_image_header(image: bytes) -> dict:
    """The kind, id, name and launcher group an image declares."""
    if len(image) < IMAGE_HEADER_BYTES or image[:4] != IMAGE_MAGIC:
        raise ValueError("not a BattleTime app image")
    kind = IMAGE_KINDS[image[6]]
    (section_count,) = struct.unpack_from("<H", image, 10)
    id_string, name_string = struct.unpack_from("<HH", image, 16)
    strings = None
    group_string = None
    for index in range(section_count):
        entry = IMAGE_HEADER_BYTES + index * SECTION_ENTRY_BYTES
        number, count, offset, length = struct.unpack_from("<HHII", image, entry)
        if number == STRINGS_SECTION:
            strings = (image[offset : offset + length], count)
        elif number == LAUNCHER_SECTION and length >= 2:
            (group_string,) = struct.unpack_from("<H", image, offset)
    if strings is None:
        raise ValueError("the image has no strings")
    section, count = strings
    data = section[count * STRING_TABLE_ENTRY_BYTES :]

    def string(which: int) -> str:
        start, length = struct.unpack_from("<HH", section, which * STRING_TABLE_ENTRY_BYTES)
        return data[start : start + length].decode()

    return {
        "kind": kind,
        "id": string(id_string),
        "name": string(name_string),
        "group": "" if group_string is None else string(group_string),
    }


# ============================================================================
# Descriptions from the docs
# ============================================================================


def plain_text(markdown: str) -> str:
    """Markdown as plain words: links become their text, and emphasis and code marks go."""
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", markdown)
    return re.sub(r"\*\*|`", "", " ".join(text.split()))


def app_description(app_id: str) -> str:
    """The first paragraph of the app's page in docs/apps."""
    page = DOCS / "apps" / f"{app_id}.md"
    if not page.exists():
        return ""
    paragraph = []
    for line in page.read_text().splitlines():
        if line.startswith("#") or line.startswith("!["):
            if paragraph:
                break
            continue
        if not line.strip():
            if paragraph:
                break
            continue
        paragraph.append(line)
    return plain_text(" ".join(paragraph))


def face_descriptions() -> dict:
    """Each face's notes from the table in docs/faces.md, or where it came from when it has none."""
    descriptions = {}
    for line in (DOCS / "faces.md").read_text().splitlines():
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 3 or "`" not in cells[0]:
            continue
        ids, source, notes = cells
        for face_id in re.findall(r"`([^`]+)`", ids):
            # A row of several faces names its sources together, as "wasp-os faces of the same names".
            origin = plain_text(source).replace("faces of the same names", face_id)
            descriptions[face_id] = plain_text(notes) or f"From {origin}."
    return descriptions


# ============================================================================
# Building
# ============================================================================


def run(command: list, cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def sources() -> list:
    """Every app and face source, as (folder kind, id, path), sorted by id within each kind."""
    found = []
    for folder, file_name in (("apps", "app.tsx"), ("faces", "face.tsx")):
        for directory in sorted((BATTLETIME / folder).iterdir()):
            source = directory / file_name
            if source.exists() and not (folder == "apps" and directory.name in NOT_APPS):
                found.append((folder, directory.name, source))
    return found


def file_entry(path: Path) -> dict:
    data = path.read_bytes()
    return {"file": path.name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}


def build_apps(compiler: Path, out: Path) -> list:
    faces = face_descriptions()
    entries = []
    for folder, folder_id, source in sources():
        image = out / f"{folder_id}.ntapp"
        print(f"{folder}/{folder_id}")
        run([str(compiler), str(source), "-o", str(image)], BATTLETIME)
        header = read_image_header(image.read_bytes())
        if header["id"] != folder_id:
            sys.exit(f"{folder}/{folder_id} declares the id {header['id']}; a folder and its app share one id.")
        description = faces.get(folder_id, "") if header["kind"] == "face" else app_description(folder_id)
        entries.append({**header, **file_entry(image), "description": description})
    ids = [entry["id"] for entry in entries]
    duplicates = sorted({app_id for app_id in ids if ids.count(app_id) > 1})
    if duplicates:
        sys.exit(f"Two packages share the id {', '.join(duplicates)}, so their files would collide.")
    return entries


def build_plugins(out: Path) -> list:
    entries = []
    for folder in sorted((COMPANION / "plugins").iterdir()):
        if not (folder / "plugin.json").exists():
            continue
        run(["node", "scripts/build-plugin.mjs", str(folder), str(out)], COMPANION)
        manifest = json.loads((folder / "plugin.json").read_text())
        package = out / f"{manifest['id']}.ntplugin"
        with zipfile.ZipFile(package) as archive:
            packed = json.loads(archive.read("manifest.json"))
            watch = [read_image_header(archive.read(path))["id"] for path in packed.get("watch", [])]
        entries.append(
            {
                "id": manifest["id"],
                "name": manifest["name"],
                "version": manifest["version"],
                "description": manifest.get("description", ""),
                "watch": watch,
                **file_entry(package),
            }
        )
    return entries


def git(*arguments: str) -> str:
    try:
        return subprocess.run(["git", *arguments], cwd=ROOT, check=True, capture_output=True, text=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return ""


def write_zip(out: Path, names: list) -> None:
    with zipfile.ZipFile(out / STORE_ZIP, "w", zipfile.ZIP_DEFLATED) as archive:
        for name in names:
            info = zipfile.ZipInfo(name, ZIP_DATE)
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, (out / name).read_bytes())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out", type=Path, default=BATTLETIME / "dist" / "store", help="the folder to fill")
    parser.add_argument("--no-plugins", action="store_true", help="leave the companion's plugins out")
    arguments = parser.parse_args()

    out = arguments.out.resolve()
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    run(["cargo", "build", "-q", "--release", "-p", "battletime-compiler"], BATTLETIME)
    compiler = BATTLETIME / "target" / "release" / "battletime-compiler"
    run([str(compiler), "library", "-o", str(out / LIBRARY_FILE)], BATTLETIME)

    apps = build_apps(compiler, out)
    plugins = [] if arguments.no_plugins else build_plugins(out)

    version = re.search(r'^version = "(.*)"', (BATTLETIME / "firmware" / "Cargo.toml").read_text(), re.M)
    index = {
        "format": STORE_FORMAT,
        "version": version.group(1) if version else "",
        "commit": git("rev-parse", "--short=7", "HEAD"),
        "apps": apps,
        "plugins": plugins,
    }
    (out / STORE_INDEX).write_text(json.dumps(index, indent=2, ensure_ascii=False) + "\n")
    write_zip(out, [STORE_INDEX, *(entry["file"] for entry in apps + plugins)])

    sums = [f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}" for path in sorted(out.iterdir())]
    (out / "SHA256SUMS").write_text("\n".join(sums) + "\n")
    print(f"{len(apps)} apps and faces and {len(plugins)} plugins in {out}")


if __name__ == "__main__":
    main()
