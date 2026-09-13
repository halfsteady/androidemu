# Isolated SNES probe

`main.rs` is an Emulia-authored development harness for jgenesis revision
`b1419eface3147568b2247d33b6bdb6695adfe06`. It is not a workspace member and adds no
upstream dependency to the app. It exercises only an original generated LoROM
program, not commercial games or enhancement chips. The evaluation executable
links GPL-licensed upstream code; shipping-core selection remains undecided.

The results and reproduction instructions belong in `docs/SNES-CORE-COMPARISON.md`.
