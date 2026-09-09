# Released snapshot fixtures

`preview-v1.state` was written by the v1 snapshot encoder at commit `bdb2100`,
before the v2 changes. It contains only the synthetic cartridge and machine
state used by `core/tests/common/mod.rs`; no external game ROM is included.

To reproduce it using that revision, create `Nes::new(&common::rom(0, 0))`, call
`nes.step()` exactly 1,234 times, then write `nes.save_state()` without running
any additional frames or bus accesses. The file is 96,881 bytes; its whole-file
FNV-1a-64 hash is `cbce44fcd468ddf1`.

`preview-v2.state` was written by commit `9954d20` (released in `v0.2.5-rc1`),
using the same synthetic cartridge and 1,234 steps. It is 96,947 bytes; its
whole-file FNV-1a-64 hash is `df65d2c299a733df`.

The compatibility test loads these actual old bytes with the current reader,
checks that migration retains all serialized fields, and verifies deterministic
replay after migration. Generating fixtures with the current writer would no
longer exercise compatibility with the released encoders.

`preview-v2-rendering.state` also comes from `9954d20`. It captures scanline 21,
dot 81 before sprite 0 starts at X=128. Its size is 96,947 bytes and its whole-file
FNV-1a-64 hash is `92146647353737b2`. The test checks that migration preserves the
position and timing of the next sprite hit, including conversion from the old
static sprite coordinates/patterns to any new counters/shifters.

To reproduce with that revision and the same synthetic cartridge:

```rust
nes.bus.write(0x2006, 0);
nes.bus.write(0x2006, 0);
for _ in 0..8 { nes.bus.write(0x2007, 0xff); }
nes.bus.ppu.oam.fill(0xff);
nes.bus.ppu.oam[..4].copy_from_slice(&[20, 0, 0, 128]);
nes.bus.ppu.palette[0] = 0x0f;
nes.bus.ppu.palette[1] = 0x21;
nes.bus.ppu.palette[0x11] = 0x32;
nes.bus.ppu.v = 0;
nes.bus.ppu.t = 0;
nes.bus.ppu.mask = 0x1e;
while nes.bus.ppu.scanline != 21 || !(80..=88).contains(&nes.bus.ppu.dot) {
    nes.step();
}
// Write nes.save_state() here, with no additional clocks.
```

`core/tests/ppu_pipeline.rs` includes OAM bus traces documented in AccuracyCoin's
`$2004 Stress Test`, from the pinned `affc643aa771028510c4427451ecf5bba5e54592`
revision. Its MIT license is preserved in [ACCURACYCOIN-LICENSE](ACCURACYCOIN-LICENSE).
The external test ROM is not bundled.
