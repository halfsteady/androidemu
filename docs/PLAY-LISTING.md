# Play Store listing

Draft copy and the compliance checklist for publishing Emulia free on Google
Play. Not legal advice — the judgement calls below are explained so they can be
overruled deliberately.

## Why the app is called Emulia

"NES" is a Nintendo registered trademark. Google's
[Intellectual Property policy](https://support.google.com/googleplay/android-developer/answer/9888072)
states that an app using another party's trademark "in a way that is likely to
cause confusion" may be suspended, and enforcement escalates to terminating the
developer account — which would take every other listing on the account with it.

Plenty of emulators do carry "NES" in their Play titles, so this is tolerated
rather than permitted, and it is complaint-driven. Nintendo files complaints.
A name is a cheap thing to give up for that risk.

Referring to the hardware factually in the description is nominative use and is
much more defensible than using the mark as the product name. The copy below
mentions it exactly once, to say what file the app opens.

## Store fields

**App name** (30 char limit)

```
Emulia
```

**Short description** (80 char limit)

```
A fast, private 8-bit game emulator. No ads, no accounts, nothing tracked.
```

**Full description** (4000 char limit)

```
Emulia plays 8-bit cartridge files (.nes) that you already own, on your phone
or tablet.

It does not come with any games, and it cannot download any. You add your own
files from your device, and they stay on your device.

WHAT IT DOES

• Accurate emulation, written from scratch — every CPU instruction, cycle-exact
  video timing, and all five sound channels
• NTSC, PAL and Dendy timing, picked up from the file
• Smooth 60 fps with low-latency audio, paced to your screen's refresh rate
• Ten save slots with thumbnails, plus an automatic save so one tap picks up
  exactly where you stopped
• Rewind — hold Undo and the game runs backwards, frame by frame
• Full screen, with on-screen controls you can hide
• USB and Bluetooth controllers for two players, with a guided setup wizard for
  controllers whose buttons come through wrong
• Battery saves, kept per game, exactly as the cartridge would

WHAT IT DOES NOT DO

• No ads, ever
• No accounts, no sign-in
• No analytics, no tracking, no data collection of any kind
• No paid tiers, no in-app purchases — every feature is here
• No internet permission at all. The app cannot talk to the network, which you
  can verify: it declares no INTERNET permission

Emulia was built for one nine-year-old and her dad, which is why it opens on a
shelf of games and resumes with a single tap.
```

Keeping "no INTERNET permission" in the copy is worth it: it is unusual, it is
checkable, and it answers the privacy question before anyone asks it.

## Console checklist

| Field | Answer | Why |
|---|---|---|
| Category | Arcade, or Tools | Either is defensible; Arcade reaches the right people |
| Target audience | **13+** | Built for a child, but declaring it child-directed triggers the [Families policy](https://support.google.com/googleplay/android-developer/answer/9893335) and a stricter review it gains nothing from |
| Contains ads | No | |
| In-app purchases | No | |
| Data safety | No data collected, no data shared | Verifiable: the manifest declares no `INTERNET` permission |
| Content rating | Complete the IARC questionnaire | No violence, no user content, no ads, no data sharing |
| Privacy policy | **Required** — a URL is mandatory even collecting nothing | Draft below |
| Government app | No | |

A new **personal** developer account created after 13 Nov 2023 must first run a
[closed test with 12 testers for 14 continuous days](https://support.google.com/googleplay/android-developer/answer/14151465)
before production access. An account older than that, or an organization
account, is exempt.

## Privacy policy

Play needs this at a public URL. The whole of it:

> **Emulia privacy policy**
>
> Emulia does not collect, store, transmit or share any personal data.
>
> The app has no internet permission and cannot make network connections. Game
> files you add, your saved games and your settings are stored only on your own
> device, in the app's private storage, and are removed when you uninstall the
> app.
>
> There is no analytics, no advertising, no crash reporting and no account.
>
> Contact: <address>

## Things that would change this answer

- **Box art or a bundled game-metadata pack.** Shipping publisher artwork is
  straightforward copyright infringement and is the single most likely reason a
  later version gets pulled. Let people supply their own art instead.
- **"Game Genie"** is a trademark. The cheats feature should be called cheat
  codes.
- **RetroAchievements**, or anything else that adds networking, ends the "no
  INTERNET permission" claim and changes the Data safety answers. Remove the
  claim from the listing in the same release that adds the permission.
