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

## What is in the console, as of 2026-09-07

The listing copy above is **entered and saved** as a draft ("Change saved. Send
for review in Publishing overview"). What is still missing is graphics:

| field | state | file |
|---|---|---|
| App name, short + full description | **done** | the copy above |
| App icon (512×512) | not uploaded | `brand/png/c-one-in-four/play-512.png` |
| Feature graphic (1024×500) | not uploaded | `brand/png/c-one-in-four/play-feature-1024x500.png` |
| Phone screenshots (2–8) | not captured | — |
| Privacy policy URL | **blocked** | text below, needs hosting |
| Data safety form | not answered | answers in the checklist below |

Both graphics are built and on the `/stuff` shelf. They cannot be uploaded from
a script: the console has no `<input type=file>` in its DOM and opens a native
picker instead, so those three rows are a human at a keyboard.

Screenshots need a real device — the emulator on this machine will not start
(its KVM check reads group membership rather than the ACL that grants access,
and forced past that it cannot initialise a Qt platform plugin headless).

**The copy predates two shipped features.** It does not mention the ten picture
modes and the `.pal` palette import, or the shelf's list/card layouts. Left
alone deliberately rather than edited on the fly — a store listing is a public
claim, and this is the version that was written and approved.

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

The text is [docs/PRIVACY.md](PRIVACY.md). **It still needs somewhere public to
live.** This repository is private, so its GitHub address returns 404 to anyone
who is not signed in as its owner — including Google's reviewer — and a policy
URL that only works for one person is not a policy URL.

Two ways to fix that, neither of which makes this repository public:

1. **A small public repo with Pages** — the tidier answer, and a real web page:

   ```sh
   gh repo create emulia-privacy --public --description "Privacy policy for Emulia"
   # copy docs/PRIVACY.md in as README.md, push, then:
   gh api -X POST repos/bsteinfeld/emulia-privacy/pages \
     -f 'source[branch]=main' -f 'source[path]=/'
   ```

   Final URL: `https://bsteinfeld.github.io/emulia-privacy/`

2. **A public gist** — thirty seconds, slightly informal, works today:

   ```sh
   gh gist create docs/PRIVACY.md --public --desc "Emulia privacy policy"
   ```

Whichever is chosen, paste the URL into Play Console at **Policy ▸ App content ▸
Privacy policy**, and keep `docs/PRIVACY.md` as the source that gets copied
there when it changes.

It contains a public contact address (`bradley@steinfeld.ca`). Play requires a
contact email on the listing regardless, so it becomes public either way — but
if a dedicated alias is preferred, change it before publishing rather than after.
