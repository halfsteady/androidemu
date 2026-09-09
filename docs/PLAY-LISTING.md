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
| Phone screenshots (2–8) | not captured | tablet ones exist: `site/assets/*.webp` |
| Privacy policy URL | **done** | `https://emulia.website/privacy.html` |
| Data safety form | not answered | answers in the checklist below |

Both graphics are built and on the `/stuff` shelf. They cannot be uploaded from
a script: the console has no `<input type=file>` in its DOM and opens a native
picker instead, so those three rows are a human at a keyboard.

Screenshots need a real device — the emulator on this machine will not start
(its KVM check reads group membership rather than the ACL that grants access,
and forced past that it cannot initialise a Qt platform plugin headless). Four
tablet screenshots were captured by hand and are in `site/assets/`; Play's phone
screenshots are a separate size and still outstanding.

### The screenshots show other people's games

Every screenshot of an emulator shows a game, and the four in `site/assets/` show
Super Mario Bros./Duck Hunt, Final Fantasy, Basewars and Bubble Bobble — two of
them Nintendo titles, named in text on the shelf.

This is worth a deliberate decision rather than a default, because it cuts
against the reasoning at the top of this file. The app gave up the "NES" name to
avoid a complaint from a company that files them; a marketing page showing Mario
running, with "SuperMarioBros-DuckHunt" spelled out beside it, is a louder
target than the name was, and it is the first thing a reviewer or a rights holder
sees. Play's own policy is separate from copyright law here: emulators are
allowed, but a listing that looks like it distributes games attracts the
enforcement that emulators otherwise avoid.

Three ways out, in order of cost:

1. **Recapture the shelf with homebrew or public-domain ROMs.** There are good
   free ones, and it removes the issue completely — the app looks the same.
2. **Keep the play and settings shots, drop the two shelf shots.** The titles in
   text are the sharpest edge; the gameplay alone is far less identifiable.
3. **Ship as is.** Common practice among emulators, and mostly untroubled — but
   it is a bet, and the thing being bet is the developer account that every
   other listing sits on.

The pages are built so this is a file swap: replace the `.webp` files in
`site/assets/` and nothing else changes.

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
| Privacy policy | **Required** — a URL is mandatory even collecting nothing | https://emulia.website/privacy.html |
| Government app | No | |

A new **personal** developer account created after 13 Nov 2023 must first run a
[closed test with 12 testers for 14 continuous days](https://support.google.com/googleplay/android-developer/answer/14151465)
before production access. An account older than that, or an organization
account, is exempt.

## Privacy policy

**https://emulia.website/privacy.html** — paste that into Play Console at
**Policy ▸ App content ▸ Privacy policy**.

The text is [docs/PRIVACY.md](PRIVACY.md), which stays the source. The published
page is `site/privacy.html`, written out as HTML rather than generated, so the
two have to be changed together when the policy changes — the alternative was a
Markdown build step for one page.

This repository is private, so a GitHub address for the policy would return 404
to anyone not signed in as its owner, including Google's reviewer. The site gets
around that without making this repository public: `site/` is mirrored by
[`publish-site.yml`](../.github/workflows/publish-site.yml) into the public
`bsteinfeld/emulia-site`, which is what GitHub Pages serves. See
[the site's README](../site/README.md).

Until the `emulia.website` DNS records are in place the policy is not reachable,
and Play will reject a policy URL it cannot fetch — so the DNS has to land before
the listing is sent for review.

It contains a public contact address (`bradley@steinfeld.ca`). Play requires a
contact email on the listing regardless, so it becomes public either way — but
if a dedicated alias is preferred, change it before publishing rather than after.
