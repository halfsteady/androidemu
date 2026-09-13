# Vendored jgenesis provenance

Upstream: https://github.com/jsgroth/jgenesis
Revision: `b1419eface3147568b2247d33b6bdb6695adfe06` (0.14.1 development)
License: GPLv3, with any individual-file exceptions retained in their headers.
The upstream LICENSE and README are included verbatim.

Only the nine crates needed for the SNES backend are included. The root workspace
member/default-member list is narrowed; upstream dependency and lint declarations
are retained. `emulia.patch` records every change from that revision.

Small host API additions expose current battery SRAM, standard-cartridge detection and exact-length imports
for standard LoROM/HiROM. This avoids relying on upstream's periodic save callback
when pausing, rewinding or immediately importing a battery file. No CPU, PPU, DSP
or cartridge timing behavior was changed. Enhancement-chip cartridges remain
rejected by the Emulia adapter until their persistence and time controls are
qualified. Rebase these additions explicitly when updating the upstream pin.
