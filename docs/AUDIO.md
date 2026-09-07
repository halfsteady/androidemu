# Audio latency

PLAN.md §2 asks for a 5-10 ms audio buffer. This records what the latency actually
is, where it comes from, and why the sub-10 ms figure is **not met** — because the
reason is structural rather than a constant somebody set too high.

## Where the milliseconds are

Two buffers sit between the APU and the speaker.

**The device ring.** AAudio's own buffer, opened in low-latency exclusive mode
(shared as a fallback) and sized to two bursts. On a modern tablet a burst is
96-192 frames at 48 kHz, so this is roughly **4-8 ms**. One burst would be
smaller but leaves the callback no slack at all for a late wake-up.

**The queue.** A bounded lock-free ring between the emulator and the callback.
This is the big one, and its size is forced by *when* samples arrive rather than
by any choice made in `audio.rs`.

`Native_frame` pushes a whole frame of audio in one call, because it is called
once per displayed frame:

```rust
if advance {
    nes.run_frame();
    audio::push(nes.bus.apu.samples());   // ~800 samples, all at once
}
```

So at NTSC rates 798 samples land in a single instant every 16.7 ms, and then
nothing arrives until the next frame. The queue drains smoothly at 48 kHz between
deliveries, so its level sawtooths across a **full frame's worth of samples**. For
the trough of that sawtooth to stay above empty, the mean level cannot go below
about half a frame — **8.3 ms** — plus margin for the display's own jitter.

Queue floor plus device ring is therefore about **12-14 ms**, and that is what the
target now aims at. It was 20.8 ms before: the old controller held the queue at a
fixed 1000 samples with no reference to the frame size at all.

## Why sub-10 ms needs an architectural change, not a smaller number

The sawtooth exists because audio production is driven by the **display clock**.
Splitting the push into quarter-frames would not help: the whole frame is emulated
in one burst of CPU on the render thread, so all four pieces would still arrive at
the same instant of wall time. What matters is not how many `push` calls happen
but how they are spread across real time.

Getting under 10 ms means pacing emulation against the **audio** clock instead —
either running the core on its own thread against a timer, or letting audio
consumption drive it. Both are real changes, and both interact with frame pacing,
which is currently Choreographer-driven and is what keeps video smooth and input
latency low. That trade is not worth making blind; it wants the input-to-photon
measurement first (PLAN.md §2, still outstanding).

## What the controller does now

- **The target is measured in frames of audio**, from the region's frame rate, so
  PAL's 960 samples per frame get a proportionally larger target than NTSC's 798
  instead of sharing one hardcoded constant.
- **It starts at three quarters of a frame** and adapts. On any callback that runs
  the queue dry it rises by a quarter frame, up to two frames: the floor is a
  property of the device's scheduling, so it is found from evidence rather than
  assumed.
- **It creeps back down** by a thirty-second of a frame after a long clean run,
  never below half a frame. Probing below the sawtooth's real floor would trade a
  millisecond for a click every few seconds, which is the wrong way round.
- **Dynamic rate control** nudges the resample ratio to hold the level, clamped to
  ±0.3 %. That is inaudible as pitch, and anything fast enough to correct an error
  in under a second is not.
- **An underrun holds the last sample** rather than writing zero. A click is far
  more audible than a briefly held level.

## Reading the number off the tablet

Settings shows it live, which is what makes this a measurement rather than a
claim:

```
Audio delay                                        13.4 ms
  8.9 ms queued + 4.5 ms in the device, holding 12.5 ms · 0 underruns
```

The same five figures come out of `Native.audioStats()`: queue ms, device ms,
total, current target, underrun count.

**The underrun count is the one to watch.** It should stay at zero through a long
session, including across output-route changes (headphones in and out) and while
the tablet is thermally throttling. If it climbs, the target will climb with it,
and the reported delay is then telling you what this device actually needs.

## Still outstanding

- [ ] Read the figure on the OnePlus Pad 3 over a long session, and across route
  changes, and record it. That closes the Phase 1 gate.
- [ ] Decide whether sub-10 ms is worth decoupling audio production from the
  display clock, once input-to-photon latency has been measured.
- [ ] The nonlinear mixer and the 1024-tap FIR run per frame on the emulation
  thread; neither has been profiled on the tablet.
