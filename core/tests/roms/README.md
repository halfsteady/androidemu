# Test ROMs

Test ROMs are **not** committed to this repository. Drop them in this directory and
the corresponding tests switch themselves on; without them those tests skip with a
printed note rather than failing.

| File | What it verifies | Where to get it |
|---|---|---|
| `nestest.nes` | Every documented and undocumented CPU opcode, and exact cycle counts | Kevin Horton's `nestest`, distributed with its reference log |
| `nestest.log` | The reference trace the above is compared against | Ships alongside `nestest.nes` |
| `cpu_instrs/*.nes` | Instruction behaviour, in detail | blargg's `cpu_instrs` suite |
| `ppu_vbl_nmi/*.nes` | Vblank flag and NMI timing | blargg's `ppu_vbl_nmi` suite |
| `mmc3_test/*.nes` | MMC3 scanline IRQ timing | blargg's `mmc3_test` suite |

All of the above are freely redistributable homebrew test programs, but they are
left out of the tree so this repository contains no binaries.

## Running nestest

`nestest.nes` has an automation mode: start the CPU at `$C000` with the cycle count
at 7 and it runs every opcode without needing any input, writing results to
`$0002`/`$0003`.

```sh
cargo test -p nes-core --test nestest
cargo run -p nes-runner -- trace core/tests/roms/nestest.nes 8992 --pc=C000
```
