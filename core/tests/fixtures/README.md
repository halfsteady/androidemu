# Legacy snapshot fixture

`preview-v1.state` was written by the v1 snapshot encoder at commit `bdb2100`,
before the v2 changes. It contains only the synthetic cartridge and machine
state used by `core/tests/common/mod.rs`; no external game ROM is included.

To reproduce it using that revision, create `Nes::new(&common::rom(0, 0))`, call
`nes.step()` exactly 1,234 times, then write `nes.save_state()` without running
any additional frames or bus accesses. The file is 96,881 bytes; its whole-file
FNV-1a-64 hash is `cbce44fcd468ddf1`.

The compatibility test loads these actual old bytes with the current reader.
Generating the fixture with the current writer would no longer exercise v1
compatibility.
