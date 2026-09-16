# Audio buffering and latency

The small-buffer target assumes the device actually grants low-latency output.
The Raspberry Pi 5 investigation found that a successful AAudio open can instead
return a normal shared mixer, with much larger batches and delay. See the
[device measurements and reproduction](RASPBERRY-PI.md).

## What the app measures

Two buffers are visible to Emulia:

- **Source queue:** samples waiting between emulation and the audio callback.
- **AAudio output buffer:** the usable buffer size granted by AAudio, divided by
  its sample rate. On a legacy stream this is an AudioTrack buffer, not the
  entire hardware pipeline.

Their sum is a buffering estimate. Additional mixing, driver queues and TV or
headphone processing are not included. The earlier **Audio delay** label and
claim of 12–14 ms end-to-end were too strong. Settings now says **Audio buffering**.
On suitable low-latency hardware, roughly 12–14 ms remains an estimate for these
two buffers only, not a measured guarantee.

`Native.audioStats()` returns six floats: source queue ms, AAudio buffer ms,
their sum, source target ms, source underruns and platform underruns.

The gap counts describe different failures:

- **Source gaps:** callbacks exhausted the samples produced by emulation.
- **Output gaps:** AAudio's own `getXRunCount()` reports underruns downstream.
  A source queue can be full while an undersized output buffer repeatedly fails.

Both should remain zero during steady playback. Output counts are retained on
close so pausing to inspect Settings does not discard them; both counters reset
when a new stream starts. Initial queue priming outputs silence before consuming
game samples and is not counted as an underrun.

## Low-latency output

The backend requests low-latency exclusive output, uses the device's natural
sample rate, marks its usage as a game, and fills samples through a lock-free
callback. It checks the **granted performance mode** before shrinking the output
buffer to two bursts.

Emulation produces about 798 samples all at once every 16.64 ms for NTSC, or 960
every 20 ms for PAL. The source queue drains between deliveries, so it needs room
for this sawtooth. Its target starts at three quarters of a frame, rises by a
quarter frame on starvation up to two frames, and slowly falls toward half a
frame after long clean runs. Dynamic rate correction is bounded to ±0.3%.

A truly sub-10 ms pipeline would require emulation paced against the audio clock,
or another way to spread sample production over wall time. Splitting a frame
into multiple push calls without spreading the work out does not accomplish it.

## Normal-mixer fallback

If AAudio declines low-latency mode, the callback burst is not a safe measure of
the downstream mixer's consumption quantum. On Pi 5 LineageOS, bursts are 960
frames but the mixer consumes 4096 frames at a time. Reducing the output buffer
to 1920 frames caused 139 platform underruns in a 12-second silence-only test.
Keeping its 8196-frame default produced zero.

The backend therefore retains the platform's default buffer on this path and
uses its duration as a conservative window for the source queue's adaptive
controller. It primes the source queue before consuming game samples. The ring
has 32768 samples, with the window capped at a quarter of that capacity, leaving
room for the two-window adaptive ceiling and incoming NES frames.

The larger buffers prevent gaps but cannot remove latency in the platform.
Reducing that latency requires further device/OS work; this code does not change
system audio policy, restart services, or claim a low-latency route exists.

## Still to measure

- Sustained gameplay and remaining cutouts/delay on the Pi. Bradley's initial
  comparison of the installed test build was "much better"; duration and residual
  issues were not quantified.
- Long sessions and output-route changes on the OnePlus Pad 3.
- Physical input-to-sound and input-to-photon latency; queue sizes alone do not
  measure either.

The [Android audio guidance](https://developer.android.com/games/sdk/oboe/low-latency-audio)
covers mode requests, game usage, callbacks, buffer sizing and platform underruns.

## Scope and performance controls

This is the shared Android backend, packaged in the regular release APK/AAB for
supported phones, tablets and Android-based Pi installations. There is no Pi
flavor or model-name switch. AAudio's granted performance mode chooses the buffer
policy each time output opens. Low-latency output still requests two bursts;
normal output keeps Android's default buffer. The SDL desktop backend is separate.

Compared with 0.3.1, the static audio queue reserves an additional **96 KiB**.
The CPU/PPU/APU core and per-sample synthesis are unchanged. Larger fallback
buffers improve continuity at the cost of sound arriving later. They do not
speed up emulation or establish an end-to-end latency measurement. We have not
measured every Android device's scheduling or audio path.

Settings now labels the queue **target** explicitly. When playback is paused,
it says **Paused** and shows the last buffer settings rather than presenting
the output-buffer size as live sound latency. Source/output gap counts are
cumulative since playback last started, not gaps per second.

The **Performance → Rewind history** switch defaults on. Turning it off frees
the in-memory rewind chain and skips per-frame snapshot recording. It preserves
emulation timing, audio synthesis, manual states, autosave and battery saves;
turning it back on starts fresh history at the current frame. The Pi's earlier
600-frame runs measured about 0.46 ms/frame for rewind recording (roughly 5% of
those core+rewind runs). This is a modest CPU saving, not a remedy for HDMI delay.
**Look → Off** is the existing option to avoid extra picture-filter work.

AccuracyCoin's 144 tests run during development/CI, not inside gameplay. There
is no runtime test workload to disable. This release keeps the accurate core
behavior and offers the independent rewind control instead of a less accurate
emulation mode.
