#!/usr/bin/env python3
"""Generate core/src/cart/overrides.rs from ROMs you have plus a board database.

The override table corrects what a ROM says about itself, so a wrong row is worse
than no row: it silently mis-configures a real game. This therefore never guesses.
It reads each ROM you point it at, works out what the header claims, looks the ROM
up in a published board database, and emits a row *only* where the two disagree.

    python3 scripts/build-header-db.py --roms ~/roms --db nescartdb.xml
    python3 scripts/build-header-db.py --roms ~/roms --db corrections.tsv
    python3 scripts/build-header-db.py --self-test

Two database formats:

  * **TSV** — the explicit path, and the one to prefer. One row per board:

        sha1_of_prg <TAB> mapper <TAB> mirroring <TAB> battery <TAB> region <TAB> name

    Any field may be `-` to say nothing about it. `mirroring` is one of
    horizontal, vertical, single-lo, single-hi, four. `region` is ntsc, pal, dendy.

  * **XML** — best effort against a NesCartDB-style dump, matching `<prg sha1=…>`
    inside a `<board mapper=…>`. The schema is read defensively and the run prints
    how many boards it understood, because a dump whose shape differs would
    otherwise look like a database with nothing wrong in it. Check that number.

ROMs are matched on the SHA-1 of their PRG, which is what these databases key on.
The generated table is keyed on the emulator's own identity hash instead — FNV-1a
over everything after the header — because that is what `Cartridge::load` has in
hand before it has decided whether to believe the sizes.
"""

from __future__ import annotations

import argparse
import hashlib
import sys
import tempfile
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "core/src/cart/overrides.rs"

MIRRORING = {
    "horizontal": "Mirroring::Horizontal",
    "vertical": "Mirroring::Vertical",
    "single-lo": "Mirroring::SingleScreenLo",
    "single-hi": "Mirroring::SingleScreenHi",
    "four": "Mirroring::FourScreen",
}
REGION = {"ntsc": "Region::Ntsc", "pal": "Region::Pal", "dendy": "Region::Dendy"}


@dataclass
class Claim:
    """What a board database says a cartridge really is. `None` means it is silent."""

    name: str
    mapper: int | None = None
    mirroring: str | None = None
    battery: bool | None = None
    region: str | None = None


@dataclass
class Rom:
    """What a ROM file says about itself."""

    path: Path
    identity: int
    prg_sha1: str
    mapper: int
    mirroring: str
    battery: bool
    region: str
    nes2: bool


def fnv1a(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for b in data:
        h = ((h ^ b) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def read_rom(path: Path) -> Rom | None:
    """Parse the header the same way core/src/cart/mod.rs does, before any repair."""
    raw = path.read_bytes()
    if len(raw) < 16 or raw[0:4] != b"NES\x1a":
        return None
    flags6, flags7 = raw[6], raw[7]
    nes2 = (flags7 & 0x0C) == 0x08
    mapper = (flags7 & 0xF0) | (flags6 >> 4)
    if nes2:
        mapper |= (raw[8] & 0x0F) << 8
    prg = raw[4] * 16384
    payload = 16 + (512 if flags6 & 0x04 else 0)
    if payload > len(raw):
        return None
    if flags6 & 0x08:
        mirroring = "four"
    elif flags6 & 0x01:
        mirroring = "vertical"
    else:
        mirroring = "horizontal"
    region = "ntsc"
    if nes2:
        region = {1: "pal", 3: "dendy"}.get(raw[12] & 0x03, "ntsc")
    elif len(raw) > 9 and raw[9] & 1:
        region = "pal"
    return Rom(
        path=path,
        identity=fnv1a(raw[payload:]),
        prg_sha1=hashlib.sha1(raw[payload : payload + prg]).hexdigest().lower(),
        mapper=mapper,
        mirroring=mirroring,
        battery=bool(flags6 & 0x02),
        region=region,
        nes2=nes2,
    )


def load_tsv(path: Path) -> dict[str, Claim]:
    claims: dict[str, Claim] = {}
    for number, line in enumerate(path.read_text().splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = [p.strip() for p in line.split("\t")]
        if len(parts) < 6:
            raise SystemExit(f"{path}:{number}: expected 6 tab-separated fields")
        sha1, mapper, mirroring, battery, region, name = parts[:6]
        if mirroring != "-" and mirroring not in MIRRORING:
            raise SystemExit(f"{path}:{number}: unknown mirroring {mirroring!r}")
        if region != "-" and region not in REGION:
            raise SystemExit(f"{path}:{number}: unknown region {region!r}")
        claims[sha1.lower()] = Claim(
            name=name,
            mapper=None if mapper == "-" else int(mapper),
            mirroring=None if mirroring == "-" else mirroring,
            battery=None if battery == "-" else battery.lower() in ("1", "true", "yes"),
            region=None if region == "-" else region,
        )
    return claims


def load_xml(path: Path) -> dict[str, Claim]:
    """Best effort over a NesCartDB-style dump. Tags are matched anywhere in the
    tree rather than at a fixed depth, so a differently nested dump still reads."""
    claims: dict[str, Claim] = {}
    root = ET.parse(path).getroot()
    for cartridge in root.iter():
        if cartridge.tag != "cartridge":
            continue
        system = (cartridge.get("system") or "").upper()
        region = "pal" if "PAL" in system else "dendy" if "DENDY" in system else "ntsc"
        for board in cartridge.iter("board"):
            mapper = board.get("mapper")
            prg = next((p for p in board.iter("prg") if p.get("sha1")), None)
            if mapper is None or prg is None:
                continue
            mirroring = None
            pad = next(board.iter("pad"), None)
            if pad is not None and pad.get("h") is not None and pad.get("v") is not None:
                # A pad shorted vertically mirrors horizontally, and vice versa.
                mirroring = "horizontal" if pad.get("v") == "1" else "vertical"
            battery = any(w.get("battery") == "1" for w in board.iter("wram"))
            claims[(prg.get("sha1") or "").lower()] = Claim(
                name=(cartridge.getparent().get("name") if hasattr(cartridge, "getparent") else None)
                or board.get("type")
                or "unknown board",
                mapper=int(mapper),
                mirroring=mirroring,
                battery=battery,
                region=region,
            )
    return claims


def disagreements(rom: Rom, claim: Claim) -> dict[str, str]:
    """Only the fields the database and the header actually differ on."""
    out: dict[str, str] = {}
    if claim.mapper is not None and claim.mapper != rom.mapper:
        out["mapper"] = f"Some({claim.mapper})"
    if claim.mirroring is not None and claim.mirroring != rom.mirroring:
        out["mirroring"] = f"Some({MIRRORING[claim.mirroring]})"
    if claim.battery is not None and claim.battery != rom.battery:
        out["battery"] = f"Some({str(claim.battery).lower()})"
    if claim.region is not None and claim.region != rom.region:
        out["region"] = f"Some({REGION[claim.region]})"
    return out


def render(rows: list[tuple[int, str, dict[str, str]]]) -> str:
    head = f"""// @generated by scripts/build-header-db.py -- do not edit by hand.
//
// Header corrections keyed by ROM identity, sorted by identity so `lookup` can
// binary-search. Included by fixup.rs rather than compiled as its own module,
// because it is data rather than code.
//
// {len(rows)} row(s). Each one exists because a board database and the ROM's own
// header disagreed; nothing here is a guess. Regenerate with:
//
//     python3 scripts/build-header-db.py --roms <dir> --db <db>
//
// See docs/HEADER-DATABASE.md.

pub const TABLE: &[Override] = &[
"""
    body = []
    for identity, name, fields in rows:
        escaped = name.replace("\\", "\\\\").replace('"', '\\"')
        # Rust requires the `..base` of a struct update to come last.
        line = ["    Override { "]
        for key, value in sorted(fields.items()):
            line.append(f"{key}: {value}, ")
        line.append(f'..Override::for_rom(0x{identity:016x}, "{escaped}") }},')
        body.append("".join(line))
    return head + "\n".join(body) + ("\n" if body else "") + "];\n"


def self_test() -> int:
    """Runs the whole pipeline over a ROM and a database built here, so the script
    is verifiable without anybody's collection."""
    with tempfile.TemporaryDirectory() as work:
        roms = Path(work) / "roms"
        roms.mkdir()
        # An NROM-shaped file whose header claims mapper 1 and vertical mirroring.
        rom = bytearray(16 + 32768 + 8192)
        rom[0:4] = b"NES\x1a"
        rom[4] = 2
        rom[5] = 1
        rom[6] = 0x11  # mapper low nibble 1, vertical
        (roms / "test.nes").write_bytes(bytes(rom))

        parsed = read_rom(roms / "test.nes")
        assert parsed is not None and parsed.mapper == 1, parsed
        assert parsed.mirroring == "vertical"

        db = Path(work) / "db.tsv"
        db.write_text(f"{parsed.prg_sha1}\t0\thorizontal\tfalse\tntsc\tTest Cartridge\n")
        claims = load_tsv(db)
        fields = disagreements(parsed, claims[parsed.prg_sha1])
        assert fields == {"mapper": "Some(0)", "mirroring": "Some(Mirroring::Horizontal)"}, fields

        table = render([(parsed.identity, "Test Cartridge", fields)])
        assert "1 row(s)" in table
        assert f"0x{parsed.identity:016x}" in table
        assert "mapper: Some(0)" in table
        # The `..base` of a struct update has to come last or the Rust will not
        # compile, which a generator is exactly the wrong place to get wrong.
        assert table.index("mapper: Some(0)") < table.index("..Override::for_rom")

        # A database that agrees with the header must emit nothing at all.
        db.write_text(f"{parsed.prg_sha1}\t1\tvertical\tfalse\tntsc\tTest Cartridge\n")
        assert disagreements(parsed, load_tsv(db)[parsed.prg_sha1]) == {}
        assert "pub const TABLE: &[Override] = &[\n];" in render([])
        print("self-test ok: parse, match, disagree, and render all behave")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--roms", type=Path, help="directory of .nes files to consider")
    parser.add_argument("--db", type=Path, help="board database (.tsv or .xml)")
    parser.add_argument("--out", type=Path, default=OUT)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.roms or not args.db:
        parser.error("--roms and --db are both required (or use --self-test)")

    claims = load_tsv(args.db) if args.db.suffix.lower() == ".tsv" else load_xml(args.db)
    print(f"database: {len(claims)} board(s) understood from {args.db}")
    if not claims:
        print("nothing was understood — check the dump's shape before trusting this", file=sys.stderr)
        return 1

    files = sorted(p for p in args.roms.rglob("*") if p.suffix.lower() == ".nes")
    rows: list[tuple[int, str, dict[str, str]]] = []
    seen: dict[int, Path] = {}
    unmatched = 0
    for path in files:
        rom = read_rom(path)
        if rom is None:
            print(f"skipped (not iNES): {path}")
            continue
        claim = claims.get(rom.prg_sha1)
        if claim is None:
            unmatched += 1
            continue
        fields = disagreements(rom, claim)
        if not fields:
            continue
        if rom.identity in seen:
            print(f"duplicate identity, keeping the first: {seen[rom.identity]} / {path}", file=sys.stderr)
            continue
        seen[rom.identity] = path
        rows.append((rom.identity, claim.name, fields))
        print(f"correcting {path.name}: {', '.join(sorted(fields))}")

    rows.sort(key=lambda row: row[0])
    args.out.write_text(render(rows))
    print(
        f"{len(files)} rom(s), {unmatched} not in the database, {len(rows)} correction(s) "
        f"written to {args.out}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
