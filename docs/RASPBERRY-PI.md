# Raspberry Pi 5 audio investigation

Measured on 2026-09-15, on Bradley's Pi 5 (4 GB), over ADB at
`192.168.1.60:5555`. Installed app: Emulia 0.3.1, release ARM64 APK.
OS: `23.2-20260520-UNOFFICIAL-KonstaKANG-rpi5_tv`, Android 16.

## Finding

The continuous audio cutouts reproduce with a callback that only writes silence:
no NES emulation, graphics, ROM loading, or save writes. Emulia requests
low-latency exclusive audio, but this ROM falls back successfully to a normal
shared mixer. Emulia then shrank the buffer using assumptions that only fit the
low-latency path.

The user reports roughly normal game speed and usable USB keyboard controls,
with sound repeatedly cutting out over HDMI. This matches the measured fault.
A separate CPU-specific APK is not needed to correct it; the common audio backend
can adapt to the mode the device actually grants.

## Device evidence

- Actual display mode: **1920×1080 at 60 Hz**. Broadcom V3D hardware renderer.
- Audio route: `persist.vendor.audio.device=hdmi0`. Android labels this route
  `AUDIO_DEVICE_OUT_SPEAKER`; it is not Bluetooth or a physical built-in speaker.
- Audio policy exposes only a primary PCM16 stereo 48 kHz output, without FAST
  or MMAP flags. `dumpsys media.audio_flinger` reports **No FastMixer**.
- AAudio logs: `EXCLUSIVE sharing mode not supported. Use SHARED`,
  `perfMode changed from 12 to 10`, `got Legacy ... perf = NO, burst = 960`.
- Mixer consumes **4096 frames** per cycle (85.3 ms at 48 kHz).
  AAudio initially grants **8196 frames**. Emulia reduced that to **1920**
  (40 ms), less than one mixer cycle.
- ALSA logs report eight 1024-frame periods, buffer 8192 frames, start threshold
  5056 frames. This platform has substantial buffering beyond the app queue.
- Historical Emulia tracks show repeated output underruns and approximately
  **275–315 ms reported track latency**. These are platform estimates, not
  microphone-measured input-to-speaker latency.
- CPU was approximately 69–72 °C during sampled reads. The undervoltage alarm
  was zero; the observed governor was `ondemand` with a 2.4 GHz maximum.
  No sustained thermal-load test was performed.
- About 2.3 GB memory was available, 105 GB storage free, and current I/O pressure
  was zero. The ROM is loaded into memory; rewind snapshots stay in RAM.
  The SD card is not needed to reproduce this particular fault.

The Controlplaine `throwaway-stremio/throwaway-stremio` transcript was also read.
Its earlier HWC-restart automation caused an Android restart and lost audio
routing. That was a separate incident. The present route works and display is
already at 60 Hz; no display properties, services, OS files or app saves were
changed in this investigation.

## Controlled comparisons

A standalone native AAudio callback filled silence as quickly as requested,
using the same mono float format and mode requests as Emulia:

| Buffer choice | Test duration | Platform underruns | Callback frames supplied |
|---|---:|---:|---:|
| Original two bursts, 1920 frames | 12 s | **139** | 268,800 |
| Platform default, 8196 frames | 12 s | **0** | 581,636 |

The second total includes initial buffer filling. Sustained output is 48,000
frames/s. The first setting delivered only roughly half that, despite trivial
callback work.

Then the actual Rust audio backend was fed silent samples at the NES frame rate:

- Original backend: source queue climbed to **151.5 ms** in 12 seconds while the
  platform repeatedly ran dry. Its source-underrun counter only showed **2**;
  that counter could not reveal the output fault.
- Keeping the default output buffer alone: platform gaps disappeared, but the
  source queue still recorded **26** starved callbacks over 12 seconds. A queue
  controller capped at two NES frames cannot cover the normal mixer's batches.
- Keeping the default buffer, deriving the queue window from it, and priming
  before consuming samples: **zero source underruns** over a 60-second trial.
  The queue settled around 122 ms. This deliberately favors continuous audio;
  it does not make this driver low latency.

In the silence-only default-buffer test, AAudio timestamps estimated roughly
**407–474 ms** of outstanding output, excluding any additional TV processing.
Keeping sound continuous on this stack still leaves a considerable delay.
Do not interpret Emulia's queue-plus-buffer estimate as end-to-end latency.

## Core throughput

A headless optimized ARM64 build using the normal release core ran 600 measured
frames per case, following 180 boot frames, a one-frame Start press and 120 more
warmup frames. No CPU-specific compilation flags were used.

| Game | Core only mean | Core + rewind mean | Core + rewind p99 | Worst with rewind |
|---|---:|---:|---:|---:|
| Super Mario Bros. | 8.864 ms | 9.318 ms | 9.529 ms | 9.608 ms |
| Super Mario Bros. 3 | 9.494 ms | 10.005 ms | 10.361 ms | 10.417 ms |

All measured frames fit the roughly 16.64 ms NTSC budget. Rewind added about
0.46 ms. These short runs do not include Android rendering, shaders, sustained
heat, or representative controller-driven gameplay. They show useful CPU
headroom, not a guarantee that every game and visual effect holds 60 fps.

## Implementation and validation

The Android backend now:

1. Checks the **granted** AAudio performance mode before reducing its buffer.
2. Uses the platform buffer as a conservative batching window on normal output;
   low-latency streams retain a window based on one NES frame.
3. Primes the source queue once at startup, with bounded headroom for adaptation.
4. Requests game audio usage and exposes platform gaps separately from source
   gaps in Settings. The displayed number is labeled **Audio buffering**, with
   additional device/TV delay explicitly excluded.

The same code serves tablets and the Pi. A conservative fallback buffer is a
compatibility fix, not a claim of low-latency Pi audio.

Run the real backend silently on the device (default: two ~30 s sessions, testing
startup and reopen):

```sh
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_HOME/ndk/28.2.13676358/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android29-clang"
cargo build --locked --release -p nes-android --example audio_probe --target aarch64-linux-android
adb -s 192.168.1.60:5555 push target/aarch64-linux-android/release/examples/audio_probe /data/local/tmp/emulia-audio-backend-probe
adb -s 192.168.1.60:5555 shell 'chmod 700 /data/local/tmp/emulia-audio-backend-probe && /data/local/tmp/emulia-audio-backend-probe 1800'
```

Both `source_gaps` and `output_gaps` should remain zero. This checks the actual
backend and platform together, but does not replace listening during gameplay.
The final backend passed two 1800-frame sessions (~30 s each) with **zero source
and output gaps throughout**, including close/reopen. AudioFlinger's track logs
independently confirmed zero platform underrun frames for both sessions.
The debug APK built successfully, all **33 Android/JNI unit tests passed**, and
Android lint passed. The first concurrent lint attempt crashed the local JVM;
rerunning lint with one Gradle worker succeeded. The APK is
`android/app/build/outputs/apk/debug/app-debug.apk`, version
`0.3.1-pi-audio-test-debug`, package `com.bsteinfeld.emulia.debug`. At Bradley's
request it was installed alongside release 0.3.1 and opened successfully. Its
app label is **Emulia (test)**, supplied by the debug manifest; it has a separate
library and saves. Both installed package versions were verified over ADB.
Bradley subsequently tried the installed comparison build and reported that it
was **"much better"**. This provides qualitative confirmation in the actual app,
in addition to the silent backend measurements. Session duration, remaining
cutouts and perceived delay were not quantified; sustained gameplay and physical
latency measurements remain outstanding. Temporary diagnostic executables were
removed from the Pi.
Raw diagnostic output and the standalone comparison/benchmark sources from this
investigation are in `/tmp/emulia-pi-investigation/` on the development machine.

## Next steps for responsive sound

Initial gameplay feedback supports the compatibility fix. Next quantify any
remaining cutouts and delay during sustained play, then investigate a
lower-latency OS audio configuration or driver, or measure the existing Linux
ARM64 desktop build on a separate Pi OS installation. Neither needs a fork of
the NES core. Changing ARM compiler tuning or buying an SD card cannot fix the
normal mixer's incompatible buffer quantum.

TV Game Mode is a separate input/display-latency check; it cannot explain or
repair the AAudio underruns reproduced without a game. The current TV settings
were not inspected.

References: [Android's low-latency audio guidance](https://developer.android.com/games/sdk/oboe/low-latency-audio)
explains granted modes, buffer sizing and `AAudioStream_getXRunCount`.
[KonstaKANG's Pi 5 Android TV page](https://konstakang.com/devices/rpi5/LineageOS23-ATV/)
identifies the installed OS family and its device-specific settings. The buffer
sizes and failures above come from this device, not those general documents.

## Controller handoff and two-player setup

After trying the audio build, Bradley reported that a newly mapped USB gamepad
worked during setup and resumed once with Start, then appeared unresponsive.
The old input adapter assigned the first device of any kind to P1 and the next
to P2. A keyboard could therefore reserve P1 before the pad was connected.
Mapping cleared held keys but retained that assignment. The pause menu accepted
Start independently of the game port, making this particularly confusing.
This input logic was unchanged from release 0.3.1; the audio fix did not introduce it.

The Pi's K830 keyboard reports KEYBOARD/DPAD/MOUSE/JOYSTICK, but its only joystick
axis is volume. The 0810:e501 USB pad reports KEYBOARD/JOYSTICK rather than GAMEPAD,
with real X/Y axes. Device capability checks now distinguish them correctly.
Keyboards share P1 by default without consuming a pad port, and releases or
neutral motion cannot reserve a port. Disconnect and reassignment clear held input.

[Controller setup](CONTROLLERS.md) now lists connected devices and both player
assignments. Selecting a device exposes a persistent P1/P2/Not assigned choice
and a wizard scoped to that device. The device name and player remain visible.
Saved mapping and port preferences have separate namespaces. Other devices cannot
supply mapping buttons, duplicates and reserved shoulders do not advance setup,
and the final Start release is swallowed before returning to the device panel.
Start cannot resume a game behind the controller or settings panels.

Validation on the Pi:

- The original audio comparison build failed 5 of the initial 8 input tests,
  including the actual K830 → mapped USB adapter sequence producing P2 input.
- The updated build passed **12 input instrumentation tests**: both controller
  ports, keyboard handoff, axes/releases/clearing, disconnect callbacks, explicit
  player assignments, disabled devices, mapping persistence, app recreation and
  repeated Start. Reconnect callbacks were simulated using the real device
  identities; these tests do not physically unplug a controller.
- **2 activity UI tests passed**: choose the actual USB pad, assign P2, map only
  that device, reject keyboard/duplicate/shoulder input, complete Start, recreate
  and cancel; and open/pause a temporary cartridge, keep Start from resuming
  behind controller setup, then resume from the main pause menu. The temporary
  cartridge and its saves were removed. User cartridges were never opened.
- UI screenshots of the list, device detail and mapping step were reviewed at
  1920×1080. All 8 existing controller mapping entries were preserved after tests.
- Debug APK, instrumentation APK, lint and all **33 host Android/JNI tests** passed.
  UI testing initially hit Espresso 3.5's removed InputManager reflection API on
  Android 16. Test-only dependencies now use Espresso 3.7 / AndroidX Test 1.7,
  whose [release notes](https://developer.android.com/jetpack/androidx/releases/test#espresso-3.7.0)
  document the fix. Runtime app dependencies are unchanged.

The installed comparison package remains `com.bsteinfeld.emulia.debug`, labeled
**Emulia (test)**, now version `0.3.1-pi-controller-test-debug`. The original release
package remains installed. Input logs, build results and screenshots are under
`/tmp/emulia-pi-investigation/`. Physical two-gamepad play, Bluetooth reconnects
and sustained play remain manual checks; these results establish routing and UI
behavior, not comprehensive controller hardware compatibility.

## Release follow-up: delay and optional rewind

On September 16, Bradley confirmed that gameplay and controller response felt
good but sound remained delayed. A read-only ADB inspection while Settings was
open showed **0.0 ms queued, 170.8 ms output buffer, 245.3 ms target, 3 source
gaps and 0 output gaps**. AudioFlinger's normal mixer still used 4096-frame batches.
The zero source queue was consistent with paused playback; the target was not
245.3 ms of audio being held at that moment. The old wording "holding" conflated
the controller's target with its current queue and has been replaced.

The standard Android build now offers a persistent **Rewind history** switch,
on by default. Turning it off releases the history and bypasses snapshot
recording; the native core's game timing, state format and synthesis are
unchanged. JNI tests compare emulation states with recording on/off and cover
load/reset/restore and re-enabling. This setting can recover the measured
~0.46 ms/frame rewind cost but cannot remove the Android HDMI output delay.

The release uses the same Android package on all supported Android devices.
The larger fallback queue reserves 96 KiB more static storage; devices granted
low-latency mode keep two output bursts. No core accuracy shortcuts were added.

The `0.3.2-rc1-debug` comparison build passed **15 Pi instrumentation tests**
(12 input, 2 controller activity, 1 performance-settings activity), **35 host
Android/JNI tests**, Android lint, **143 default-workspace Rust tests**, and the
pinned **144/144 AccuracyCoin** gate with no regressions. The performance test
confirmed the switch stops native history recording, survives activity recreation,
and resumes recording when enabled. Preferences were restored after testing.
Its first run exposed missing switch accessibility state in the existing shared
row component; the row now reports its switch role and on/off state. The final
performance/audio screen was reviewed on the Pi at 1920×1080.
