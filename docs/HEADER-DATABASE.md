# The header override database

A large fraction of the NES ROMs in circulation carry headers that disagree with
the board the game actually shipped on. PLAN.md §1 calls this "the difference
between *works* and *the graphics are garbage*", and it is not an exaggeration: a
wrong mapper number means the wrong banking hardware, and a wrong PAL flag means
an NTSC game runs its whole soundtrack at 50 Hz.

There are two different problems in there, and they want two different answers.

## 1. Damage the file itself proves

No outside knowledge is needed — the file contradicts itself. `fixup::repair` in
`core/src/cart/fixup.rs` handles these, always, for every ROM:

| What is wrong | What is done |
|---|---|
| Bytes 7-15 hold junk from a ripping tool (archaic iNES) | The high mapper bits and byte 9's PAL flag are discarded |
| The junk is specifically `DiskDude!` | Same, and named separately so a log says which tool |
| The claimed CHR runs past the end of the file, by whole banks | Trimmed to the banks that exist — usually a CHR-RAM game whose header claims a CHR bank |
| The claimed PRG runs past the end of the file, by whole banks | Trimmed to the banks that exist |
| The claimed sizes are short by *part* of a bank | **Nothing.** That is a damaged file, not a mislabelled one — trimming would throw away real data and hand back a game that renders garbage. It stays an error. |
| Mapper 0 with more PRG than NROM can address | Reported, not guessed. Which mapper it should be is per-game knowledge, which is what the table below is for. |

The archaic-header rule follows the NesDev recommendation: NES 2.0 if byte 7's
bits 3-2 are binary `10`; plain iNES if they are `00` *and* bytes 12-15 are clear;
otherwise archaic, and nothing in bytes 7-15 is believed. That decision is made in
exactly one place, so a header cannot be quietly repaired somewhere that does not
report it.

Everything corrected is recorded on `Header::fixes` and surfaced two ways:

```console
$ cargo run -p nes-runner -- info some-rom.nes
mapper       0 (submapper 0)
...
header       corrected:
             - ignored the high mapper bits: bytes 7-15 held junk
             - ignored the PAL flag in the same junk, and read it as NTSC
             - header carried the DiskDude! ripper's calling card
```

and in the app, into the problem log under Settings — a repaired header is usually
the reason a game looks wrong, so it is written down rather than fixed silently.

## 2. Claims that are merely wrong

The mapper number, the mirroring, whether there is a battery. Nothing in the file
says so; it takes a database. That is `TABLE` in `core/src/cart/overrides.rs`,
keyed by ROM identity and binary-searched.

**It ships empty, and that is the correct default.** An entry here overrides what
a ROM says about itself, so a row nobody has checked against a real dump would
silently mis-configure a real game — worse than leaving the header alone. Nothing
is in there that has not been verified against a ROM someone actually has.

### Filling it in

`scripts/build-header-db.py` generates the table from ROMs you have plus a board
database. It emits a row **only** where the database and the ROM's own header
disagree, and never guesses:

```sh
python3 scripts/build-header-db.py --roms ~/roms --db corrections.tsv
python3 scripts/build-header-db.py --roms ~/roms --db nescartdb.xml
python3 scripts/build-header-db.py --self-test
```

The TSV form is explicit and the one to prefer — one row per board, any field `-`
to say nothing about it:

```
sha1_of_prg	mapper	mirroring	battery	region	name
7ede138f…	0	horizontal	true	ntsc	The Liar (NROM-256)
```

`mirroring` is one of `horizontal`, `vertical`, `single-lo`, `single-hi`, `four`.
`region` is `ntsc`, `pal`, `dendy`.

The XML form is best effort against a NesCartDB-style dump. The run prints how
many boards it understood — **check that number**, because a dump whose shape
differs from what the parser expects would otherwise look like a database with
nothing wrong in it.

ROMs are matched on the SHA-1 of their PRG, which is what these databases key on.

### Two hashes, on purpose

| Field | Over | Used for |
|---|---|---|
| `Header::identity` | Everything after the 16-byte header and any trainer | The override table key |
| `Header::hash` | Exactly the PRG+CHR the final header describes | Savestate identity |

They are separate because they have to be. The override key must not depend on the
sizes the header claims, since a lying size is one of the things being corrected —
so `identity` covers the whole remainder of the file and is computed *before*
anything is repaired.

`hash` cannot change meaning at all: it is embedded in every savestate already on
the tablet, and changing it would reject them. It therefore stays exactly what it
was — the payload the final header describes. One consequence worth knowing: if a
future table row corrects a ROM's *sizes*, that ROM's `hash` moves and its old
savestates will be rejected. That is the right answer (the state belongs to a
differently configured machine) but it is a real cost, so size corrections deserve
more care than the others.

## Tests

`core/tests/header_fixup.rs` covers the repair rules against the mangled headers
that actually turn up: a DiskDude'd ROM (which claims mapper 64 *and* PAL), junk
in the tail bytes turning an unsupported mapper 20 into a working MMC3, whole
versus partial missing banks, NES 2.0 headers being left alone, and the identity
holding steady across headers that disagree.

The generator has `--self-test`, which builds a ROM and a database, runs the whole
pipeline, and checks the rendered Rust — including that the `..base` of a struct
update lands last, which a generator is exactly the wrong place to get wrong.
