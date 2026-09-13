# Desktop Dolphin qualification — September 13, 2026

## Implementation contract

Emulia launches a separately installed executable using exactly `--batch --exec`
and the canonical disc path as separate arguments. The production launcher uses
Dolphin's normal profile; it does not change speed, recording, analytics, NKit
warnings or save locations. Executable selection lives in `dolphin.json` under
Emulia's data directory. Library `disc.json` records the external path, system,
disc ID, SHA-256 and detected NKit marker. The original NES/SNES persistence path
is unchanged.

The importer checks GameCube/Wii header magic and streams the file fingerprint;
it does not certify the disc's integrity. ISO/GCM are the initial accepted
extensions. Compressed containers such as RVZ/WBFS need a later importer.
Relinking requires the same content fingerprint. Forgetting a library entry
removes only Emulia's managed metadata, never the referenced disc.

## Evidence

| Check | Result |
|---|---|
| Host / installed release | macOS arm64; Dolphin 2606a universal, signed app verified |
| Executable | `~/Applications/Dolphin.app/Contents/MacOS/Dolphin` |
| User fixture | `~/Downloads/Animal Crossing (USA).iso`; GameCube ID `GAFE01` |
| Actual format | NKit v01 marker at offset 0x200 despite `.iso` extension |
| Boot/render | Nintendo logo and animated Animal Crossing title screen captured |
| Audio/speed | User reported approximately half speed with PNG dumping enabled; confirmed normal speed after disabling it |
| Automated launcher / shell | 138 desktop tests passed, 1 optional SNES commercial-ROM test ignored; strict Clippy passed |
| Updated macOS packages | App/DMG rebuilt; package NES/SNES smoke, bundle signature and DMG integrity checks passed |
| Wii on macOS | Mario Kart Wii RMCE01 imported through packaged Emulia and reached title screen at 100% speed, ~59.9 FPS/VPS; first-run save files created |
| Wii race on macOS | User completed and won a race, reporting smooth play with loading frame drops; a subsequent race-intro capture showed 100% speed and ~59.9 FPS/VPS |
| Linux | Header handling and process logic covered by macOS tests; actual Linux Dolphin gameplay not yet qualified |

The official installer used was
[2606a universal DMG](https://dl.dolphin-emu.org/releases/2606a/dolphin-2606a-universal.dmg),
SHA-256 `15df1afeac686951647d81b0b62e11d82e8715c92927b849e2858379cee6b5ca`.
Its installed `--help` confirmed the launch flags. Source-level CLI context is
available in [Dolphin's command-line parser](https://github.com/dolphin-emu/dolphin/blob/a2efdf1197be8132674b90fe9cf4761df39752ed/Source/Core/UICommon/CommandLineParse.cpp).

The rendering probe used an explicitly temporary `/tmp` Dolphin profile with
analytics disabled and the NKit warning acknowledged through a test-only config
override. PNG frame dumping caused a substantial slowdown; it was disabled for
the user's successful speed check. No production launch enables these overrides.
The NKit warning explains potential loading, compatibility and state/recording
issues. It was not an NHK message. The user image was not modified or converted.
ROMs and screenshots are not committed or distributed.

Automated tests exercise literal spaces/non-ASCII/shell metacharacters in paths,
images above the NES size limit, deduplication, valid and invalid relinks,
external-file preservation on deletion, selected macOS bundles, malformed
settings preservation, missing binary/disc failures, nonzero exit and bounded
stderr draining. A shell integration test opens NES, launches a test subprocess,
verifies the NES autosave and absence of an embedded session, rejects a second
launch, and returns to the shelf when the subprocess ends without fabricating
playtime.

## Mario Kart Wii through packaged Emulia

User fixture: `~/Downloads/Mario Kart Wii (USA) (En,Fr,Es).nkit.iso`,
2,713,321,472 bytes, disc ID `RMCE01`, Wii header magic `5d1c9ea3`, NKit v01.
Streaming importer SHA-256:
`1e84a7ab26825260eeeb7d21024d544918bf06210a7279de6ee559bcc71e9ade`.

The rebuilt macOS `Emulia.app/Contents/MacOS/Emulia` imported the actual image
using a separate `/tmp/emulia-mario-kart-wii/emulia-data` library. Its metadata
correctly records system `wii`, the canonical path and NKit marker; the entire
Emulia data folder occupied only 12 KiB, confirming no multi-gigabyte disc copy.
The native shelf rendered the game and its active Dolphin handoff banner.

A selected test executable used `execv` to replace itself with Dolphin 2606a,
preserving Emulia's tracked child through the real launch. It appended Emulia's
literal `--batch --exec <path>` arguments after explicit test-only settings:
an isolated `/tmp/emulia-mario-kart-wii/profile`, acknowledged NKit warning,
disabled analytics and frame dumping, and enabled FPS/VPS/speed overlays.
There was no speed-limit override or continuous recording. Production launcher
arguments and default profile behavior are unchanged.

Window captures showed the Wii strap screen and Mario Kart Wii title screen.
The title overlay reported 100% speed, 59.92 FPS and 59.94 VPS. One first-run
save-creation capture showed 78% speed; these are point observations, not a
sustained race benchmark. Dolphin created `rksys.dat`, `banner.bin` and related
Wii data in the temporary profile. File creation alone does not qualify saved
progress recovery. At the title's A-button prompt, manual input was requested:
macOS `CGPreflightPostEventAccess()` returned false. The generated Dolphin
GameCube keyboard profile maps A to X. The user subsequently reported winning a race with generally smooth play and
frame drops during loading. A follow-up capture showed the 100cc Mushroom Cup
second-race introduction (Moo Moo Meadows) at 100% speed, 59.95 FPS and 59.94 VPS.
This qualifies one manually completed race, not a full compatibility or audio
assessment. The cause of the loading hitches was not profiled; JIT compilation
is not established as the cause. Dolphin documents shader-compilation stutter
in its [Ubershaders explanation](https://dolphin-emu.org/blog/2017/07/30/ubershaders/),
and the installed NKit warning identifies longer loading as another concern.
Normal window-close/return behavior and saved-progress reopening remain pending;
the Dolphin child was still active after the user's race report.

## Remaining acceptance work

The observed Animal Crossing boot does not qualify sustained gameplay, physical
controllers, memory-card saves, or all NKit behavior. Real window return/focus
and native application selection still need interactive checks; automated
subprocess completion tests do not establish window-manager behavior. macOS
input automation was unavailable, while screen capture worked.

Only the child started by this Emulia instance is supervised. An unrelated
already-running Dolphin is not detected; wrappers that detach are not qualified.
Closing Emulia intentionally leaves Dolphin running. The launch currently
raises the shelf after child exit, not merely after switching focus back.

Linux Dolphin and extended Wii gameplay remain pending. Android launching remains
a later milestone; physical-tablet validation is deferred per the user. GitHub
Actions is blocked before job startup by account billing/spending limits; local
tests are not a substitute for the pending platform matrix.
