# androidemu

A NES emulator for the OnePlus Pad 3, written from scratch. No ads, nothing locked,
no crackle, and USB controllers that actually work.

See **[PLAN.md](PLAN.md)** for the full feature and build plan.

## Layout

```
core/     nes-core  - the emulation core. No I/O, no threads, no per-frame allocation.
runner/   nes-runner - headless CLI: header info, execution traces, frame hashing.
```

The core is a pure function of `(state, input) -> (state, framebuffer, samples)`.
That property is what makes rewind, deterministic replay and rollback netplay cheap
to add later, and it is why the core is written rather than wrapped.

## Status

**Phase 0 — spike.** Complete and tested.

| Component | State |
|---|---|
| 6502 CPU | Complete: all documented and undocumented opcodes, cycle-accurate |
| Cartridge / iNES / NES 2.0 | Complete, with hash identification |
| Mapper 0 (NROM) | Complete |
| System bus, OAM DMA, controllers | Complete |
| PPU | Registers, frame timing, vblank/NMI. **Rendering is Phase 1.** |
| APU | Registers and frame-counter IRQ. **Sound is Phase 1.** |

### Timing model

Every cycle of a real 6502 is a bus cycle. This core mirrors that: `Bus::read` and
`Bus::write` each advance the whole system by one CPU cycle, and every cycle of
every instruction goes through one of them — dummy accesses included.

Cycle counts are therefore **emergent, not tabulated**. There is no instruction
timing table to get wrong, and PPU/APU accesses land on the same cycles they would
on hardware, which is what mid-scanline register writes and MMC3 IRQ timing depend
on. `cycle_counts_match_hardware` checks the emergent counts against the canonical
6502 timings for every addressing mode.

## Building

```sh
cargo test --workspace           # 33 tests, no ROMs required
cargo run -p nes-runner -- info   <rom>
cargo run -p nes-runner -- trace  <rom> [n] [--pc=C000]
cargo run -p nes-runner -- frames <rom> <n>
```

Cross-compiling for the tablet:

```sh
rustup target add aarch64-linux-android
cargo build -p nes-core --release --target aarch64-linux-android
```

### Toolchain

| Tool | Version | Location |
|---|---|---|
| Rust | 1.98.1 | `~/.cargo` |
| Android NDK | 28.2.13676358 | `~/Android/Sdk/ndk` |
| Android SDK | platform 36, build-tools 36.0.0 | `~/Android/Sdk` |
| JDK | Temurin 21 | `~/.local/jdk` |

The NDK linker paths are wired up in `.cargo/config.toml`, pinned to Android API 29.

## Testing

Tests run with no external files. Test ROMs (nestest, blargg's suites, Holy
Mapperel) are not committed; drop them into `core/tests/roms/` and the
corresponding tests switch themselves on. See
[`core/tests/roms/README.md`](core/tests/roms/README.md).

The nestest harness compares **parsed register state**, not log text — matching text
would test the formatter, and would report every failure as "line differs" rather
than naming the register that went wrong.
