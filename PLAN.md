# androidemu — NES emulator for OnePlus Pad 3

A NES emulator built for one tablet, two people, and no compromises.

## Why this exists

Every emulator on the Play Store fails at least one of these. This one fails none:

| Problem | Rule for this project |
|---|---|
| Ads | No ads. No analytics. No network calls the user didn't ask for. |
| Locked features | Everything ships unlocked. No tiers, no IAP, no "pro". |
| Slowdown / audio crackle | Frame-paced to a real 60 Hz output mode, sub-10 ms audio. Non-negotiable. |
| Too complicated for Amelia | She lands in a shelf of box art. One tap resumes. Nothing else. |
| Bad USB NES controller support | Raw USB HID fallback + per-device profiles + a remap wizard. |

## Architecture

**Core:** written from scratch in Rust. Compiled to a `.so` per ABI (arm64-v8a only, realistically), bridged to Kotlin over JNI with a thin C ABI.

**Shell:** Kotlin + Jetpack Compose. Rendering via OpenGL ES 3.2 into a `SurfaceView`. Audio via Oboe (AAudio).

Owning the core is the whole point. Rewind, frame-exact speedrun timing, Game Genie, savestates and rollback netplay are all one-line hooks into a core you wrote, and gruesome workarounds against a core you didn't. The NES is also the best-documented console ever built — this is a solved problem, just not a small one.

```
┌─────────────────────────────────────────┐
│  Compose UI · library · kid mode · HUD  │  Kotlin
├─────────────────────────────────────────┤
│  GLES renderer  │  Oboe audio  │  input  │  Kotlin
├─────────────────────────────────────────┤
│            JNI (thin C ABI)             │
├─────────────────────────────────────────┤
│  CPU · PPU · APU · mappers · state       │  Rust (no_std-friendly)
└─────────────────────────────────────────┘
```

The core is a pure function of `(state, input) -> (state, framebuffer, samples)`. No threads, no I/O, no allocation during a frame. That property is what makes rewind, replay and netplay work.

---

## 1. Emulation core

### CPU
- Cycle-accurate 6502 (2A03), including all documented and undocumented opcodes
- Correct dummy reads/writes, page-cross penalties, interrupt hijacking
- DMC DMA cycle stalls (breaks controller reads in some games if wrong)

### PPU
- Cycle-accurate 2C02: correct sprite 0 hit, sprite overflow, OAM decay
- Mid-scanline register writes (split-screen scrolling — Battletoads, Marble Madness)
- Correct MMC3 IRQ timing off A12 rises
- Open-bus behaviour on unmapped reads

### APU
- All five channels: 2× pulse, triangle, noise, DMC
- Band-limited synthesis (blip-buf style) so highs are correct rather than aliased
- Non-linear mixing per the hardware curve

### Cartridges
- iNES + NES 2.0 header parsing
- **Header override database** keyed by ROM hash — a large fraction of ROMs in the wild have wrong headers, and this is the difference between "works" and "the graphics are garbage"
- Battery-backed SRAM persisted per game
- Region detect: NTSC / PAL / Dendy, with per-game override

### Mappers
Priority order — the first tier is roughly 90 % of the commercial library:

- **Tier 1:** 0 (NROM), 1 (MMC1), 2 (UxROM), 3 (CNROM), 4 (MMC3), 7 (AxROM)
- **Tier 2:** 9/10 (MMC2/4), 11, 66, 69 (Sunsoft FME-7), 71
- **Tier 3:** 5 (MMC5), VRC2/4/6/7, Namco 163, FDS
- **Tier 4:** the long tail, driven by whatever fails to boot

### State
- Full deterministic serialization of every core struct
- Versioned savestate format with a migration path
- Rewind: ring buffer of delta-compressed states, ~60 s at 60 fps
- Deterministic input log recording — the foundation for replay, run verification and netplay

---

## 2. Timing, video and audio

This is where emulators actually lose. The Pad 3 has vastly more CPU than a NES needs; the hard part is presenting frames and samples smoothly.

- **Refresh rate:** the NES runs at 60.0988 Hz. The Pad 3's panel runs at 144 Hz, which is *not* a clean multiple of 60 (144/60 = 2.4). Request a true 60 Hz display mode via `Surface.setFrameRate(60.0988f, FRAME_RATE_COMPATIBILITY_FIXED_SOURCE)` so frames land on real vsyncs instead of judder-stuttering every third frame.
- **Frame pacing:** driven by `Choreographer`. Emulate just-in-time before vsync rather than a frame ahead, to cut a full frame of latency.
- **Audio:** Oboe / AAudio in low-latency exclusive mode, target buffer 5–10 ms.
- **Dynamic rate control:** resample the APU output to the device rate with a feedback loop that nudges the resample ratio based on output buffer fill. This is the trick that eliminates crackle *without* buying latency with a fat buffer — it's why most Android emulators either pop or feel mushy, and it costs about forty lines.
- **Latency budget:** measurable target of ≤ 3 frames input-to-photon. There's a built-in measurement tool (§6) so this is a number, not a vibe.

---

## 3. Filters and video presentation

Live thumbnail preview in the picker, global default plus per-game override.

**Scaling**
- Pixel-perfect integer scaling
- Correct aspect: 8:7 pixel aspect ratio (true hardware) or 4:3 (as televisions showed it)
- Nearest / bilinear / sharp-bilinear
- Overscan crop, adjustable per edge (many games have garbage in the top/bottom 8 px)

**Shaders**
- CRT family: scanlines, aperture grille, shadow mask, curvature, bloom, `crt-easymode` and `crt-lottes` ports
- **NTSC composite filter** (blargg's nes-ntsc): the big one. Makes dithering blend and Zelda's waterfalls look the way they actually looked, rather than the way the raw framebuffer looks.
- Smoothing family: Scale2x, HQ2x, xBRZ — for when you want the opposite of authenticity
- LCD/handheld look, for completeness

**Palettes**
- 2C02 hardware measurement, Nestopia YUV, FBX Smooth/Composite, and custom `.pal` import
- Per-game palette override

**Presets**
- Named bundles ("Living-room CRT", "Crisp", "Amelia") so nobody has to reason about shader parameters
- Screenshots and video capture record the filtered output

---

## 4. Speedrun mode

Amelia's into this, so it gets built properly rather than as a stopwatch.

**Timing**
- Timer driven by **emulated frame count**, not wall clock. Frame-exact, reproducible, and immune to the app being backgrounded or the tablet thermal-throttling.
- RTA and loadless/in-game time side by side
- Millisecond and frame-count display

**Splits**
- Manual splits by controller button, screen tap, or hardware key
- **Auto-splitters**: rules that watch NES RAM addresses and fire on conditions (e.g. SMB's world/level bytes changing). Rule editor in-app.
- Shareable auto-splitter rule packs per game, importable as a small JSON file
- PBs, gold splits, sum-of-best, ahead/behind delta against a comparison run
- Reset detection and attempt counter

**Overlay**
- Configurable HUD: timer, splits, delta, position and scale
- **Input display** — live controller overlay showing button presses, positioned anywhere

**Practice**
- Frame advance, slow motion
- Savestate hotkeys with thumbnails
- **Segment loop**: load a state, play to a trigger, auto-reset and reload. Grinding one jump becomes one button.

**Proof and sharing**
- Record the deterministic input log for a run; replay it to reproduce the run exactly, frame for frame
- Export the replay file, or render it out
- Video capture via MediaCodec, and **GIF export of a segment**
- LiveSplit `.lss` import/export so splits move between this and desktop tooling

> Assumption worth confirming: I've read "nifski" as the speedrun-GIF tooling angle, so segment GIF export and replay-file export are both in here. Say the word if it means something else and I'll re-aim this section.

---

## 5. Game Genie and cheats

The stated goal is *easy to find and easy to use*, so the database matters more than the decoder.

**Codes**
- Game Genie 6- and 8-character decoder (8-char includes the compare byte)
- Raw address/value cheats (Pro Action Replay style)
- Manual entry with live validation and a plain-English decode readout

**Finding codes — the actual feature**
- **Bundled offline cheat database**, indexed by ROM hash. Load a game, open cheats, and the codes are *already there* with human names: "Infinite lives", "Start on world 8", "Walk through walls". No browser, no ad-riddled code site, no typing.
- Categorised and searchable, with favourites
- Curated at build time from public code sets, shipped in-app — works with no network

**Cheat finder**
- RAM search: scan for values that are equal / greater / less / changed / unchanged across snapshots, narrowing until one address remains
- Kid-friendly wizard framing: "Play until your lives go down, then tap here" — repeat twice and it hands her a working code
- Save discovered codes into the local database with a name

**Using them**
- One-tap toggles from an in-game panel that doesn't leave the frame
- Enabled cheats persist per game
- Kid mode shows a picture-and-plain-name subset only

---

## 6. Controllers and input

Explicitly a pain point today, so it gets real engineering.

**USB NES adapters**
- Primary path: standard Android `InputDevice` / `KeyEvent` handling
- **Fallback path: raw USB Host API with custom HID report parsing.** Cheap NES-to-USB adapters routinely enumerate badly, report the D-pad as an analog axis, or emit non-standard descriptors. This fallback is what makes them work when nothing else does.
- Known-adapter quirks table keyed by VID/PID, applied automatically
- Axis-as-D-pad translation with configurable thresholds
- Hotplug detect, auto-reassignment to a player slot, on-screen confirmation

**Mapping**
- Per-device profiles saved by VID/PID and restored on connect
- Interactive remap wizard: "press the button you want for A"
- Turbo/autofire per button with adjustable rate
- Simultaneous mixed input: USB pad P1, Bluetooth pad P2, touch P3

**Other input**
- Bluetooth gamepads (8BitDo et al.)
- OnePlus Pad keyboard cover as a controller
- Zapper emulation via touch, for the duck-hunt afternoon

**Touch controls**
- Layout editor: drag, resize, opacity, per-button
- Presets including a **large-target kid layout**
- Haptics on press, configurable
- Optional dead-zone-free D-pad with diagonal assist

**Diagnostics**
- Input tester screen showing raw HID reports, decoded buttons, and per-device latency
- **Latency measurement tool** producing an actual input-to-photon number

---

## 7. Multiplayer

Built in three stages, each shippable.

**Local, same device**
- Two controllers into one tablet — the common case, ships first
- Split touch controls for two players on a 13" screen
- **Four Score / NES Satellite** four-player support (Bomberman II, Micro Machines, Nightmare on Elm Street)

**Local network**
- LAN / Wi-Fi Direct netplay with **rollback (GGPO-style)**. The NES is close to ideal for this: state is tiny, the core is deterministic, and rollback is nearly free once the core serializes cleanly.
- Room codes, no accounts
- Host authority on savestates and pause

**Online**
- Same rollback transport over a relay, with a matchmaking room code
- Spectator mode
- Deferred until local netplay is solid — the hard part is the rollback, not the transport

---

## 8. Library and ROM management

- Folder scanning via the Storage Access Framework
- Hash-identify ROMs against a bundled metadata pack for real titles, publisher, year, player count
- **Offline box art** for identified games; manual art override
- Box-art grid, list view, favourites, recently played, playtime tracking
- ZIP support
- Per-game config: filter, palette, region, controls, cheats, overscan
- Import/export a single backup ZIP of saves + savestates + config + splits. No cloud, no account, no lock-in.

---

## 9. Two modes

**Kid mode** — the default landing, and where Amelia lives:
- Full-bleed box art shelf, huge tap targets, no text smaller than large
- One tap resumes exactly where she left off (auto-savestate on background/exit)
- Progress is never lost — autosave on pause, on background, on battery warning
- A big obvious **rewind** button, framed as "undo"
- Simplified cheat picker: pictures and plain names
- No file paths, no mapper names, no hex. Failures say "This game didn't work" and log the real reason elsewhere.
- No reachable setting that can break playback

**Full mode** — behind a long-press plus PIN:
- Everything above, plus speedrun tooling, cheat finder, filter parameters, input diagnostics, netplay, per-game config, core settings

Switching is one gesture, and kid mode is what the app opens into.

---

## 10. Quality of life

- Rewind (hold), fast-forward (hold, 2×/4×/8×/uncapped), slow motion, frame advance
- 10 savestate slots with screenshot thumbnails, plus rotating auto-slots
- Screenshot, GIF export, MediaCodec video capture
- External display output over USB-C — NES on the TV
- Landscape and portrait, keyboard-cover aware
- Battery and thermal sanity: hard 60 fps cap, no busy-waiting, sleep when backgrounded
- Optional RetroAchievements integration — free, no ads, and a genuinely good motivator for a kid. Off by default; it's the one feature that talks to the network.

---

## 11. Accuracy and testing

An emulator without a test harness is a rumour.

- `nestest.log` CPU trace diffing — first milestone, catches almost every CPU bug
- blargg's test ROM suites: `cpu_instrs`, `ppu_vbl_nmi`, `sprite_hit`, `apu_test`, `mmc3_test`
- Holy Mapperel for mapper coverage
- **Frame-hash regression tests**: run N frames of a fixed game set with a fixed input log, hash the framebuffer, compare against golden hashes in CI. Catches silent regressions that test ROMs miss.
- Deterministic replay tests — the same input log must produce the same frame hashes on every run and every build
- Headless core runner so all of the above runs in CI without a device

---

## 12. Phasing

| Phase | Deliverable | Rough size |
|---|---|---|
| **0 — Spike** | Rust core skeleton, headless runner, `nestest.log` passing, Donkey Kong renders | ~1 week |
| **1 — Playable** | PPU/APU accurate, mappers 0/1/2/3/4/7, Compose app, GLES render, Oboe audio, touch + USB input, library, savestates, SRAM | ~3–4 weeks |
| **2 — Amelia** | Kid mode shelf, resume-on-tap, autosave, rewind button, big-touch preset, box art | ~1 week |
| **3 — Speedrun** | Frame timer, splits, auto-splitters, input display, replay record/verify, GIF + video export | ~2 weeks |
| **4 — Cheats** | Game Genie decoder, bundled offline database, cheat search wizard | ~1 week |
| **5 — Filters** | Shader pipeline, CRT set, NTSC filter, palettes, presets | ~1–2 weeks |
| **6 — Multiplayer** | Local 2P → Four Score 4P → LAN rollback → online relay | ~2–3 weeks |
| **7 — Long tail** | MMC5, VRC6/7, Namco 163, FDS, accuracy suite expansion, perf and latency tuning | ongoing |

### Implementation progress

Phase 1 has an installable preview: tier-1 mappers plus 66 (GxROM), NTSC/PAL/Dendy
timing, five-channel audio, PPU rendering, a Compose shell, GLES video, AAudio
output, standard controller/touch input with a per-controller button-mapping
wizard, a full-screen mode, hold-to-undo rewind, ROM import, savestates and SRAM
persistence. Releases are built, signed and published by GitHub Actions; see
[the release runbook](docs/RELEASING.md). The 39-ROM regression set and nestest
pass.
**Phase 1 remains open until device acceptance passes.** See
[the implementation and acceptance record](docs/PHASE-1.md) for measured results,
known limits and the outstanding gates.

### Savestate and UI/UX acceptance

Savestates and a polished UI are required Phase 1 deliverables, not optional extras:

- Ten manual slots with screenshots and timestamps; separate automatic resume state.
- Save/load without leaving the game flow; explicit confirmation before overwriting
  an occupied slot, and no silent replacement of another game's progress.
- Atomic persistence on pause/background, validated state loads, and useful recovery
  messages when a file is damaged or belongs to a different game/version.
- A clear game shelf with a helpful empty state, large touch targets, responsive
  portrait/landscape layout, readable contrast and accessible control labels.
- Obvious pause/resume and save controls, progress feedback during file operations,
  controller disconnect handling, and no stuck inputs when focus is lost.
- Verify this flow on-device: import → play → pause → save → load → background →
  reopen → resume. Measure frame pacing, audio latency and input latency before
  calling the phase complete.

Phase 2 has rewind. It still needs the dedicated kid-mode lock, the large-target
kid layout and richer box-art handling.

Phases 1 and 2 together are the point at which Amelia stops using anything else. Everything after that is upside.

---

## Non-goals

- No ROMs bundled or downloadable in-app
- No ads, telemetry, accounts, subscriptions, or feature gates — ever
- No Play Store release (sideload / personal build), so no store-policy compromises
- No other consoles. NES only, done properly.

---

## Open questions

1. Does "nifski" point at GIF export specifically, or something else in the speedrun toolchain?
2. Which USB NES adapters do you actually have? Their VID/PIDs go straight into the quirks table, and I'd rather target the real hardware than guess.
3. RetroAchievements: worth the one network dependency, or keep the app fully offline?
