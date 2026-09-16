# Releasing Emulia

Linux jobs use the halfsteady organization runner. See [CI-RUNNERS.md](CI-RUNNERS.md)
for the runner profile, manual desktop platform checks, and repository transfer.

One workflow produces everything: `.github/workflows/android-release.yml` builds
a signed **AAB** (what Play Console wants) and a signed **APK** (what you
sideload onto the tablet) from the same commit, so the two can never disagree.

- **App name:** Emulia
- **Package (`applicationId`):** `com.halfsteady.emulia`
- **Dev builds** install alongside it as `com.halfsteady.emulia.debug`.
- Renamed from `com.bsteinfeld.emulia` in 0.3.2. Android identifies an app by
  this string, so the new build does not update the old one — it installs beside
  it. Anything sideloaded before 0.3.2 should be uninstalled by hand, and its
  saves do not carry over.

The name carries no third-party trademark, which is the point: "NES" is
Nintendo's, and using it as a store name is the most common way an emulator
listing gets pulled. [docs/PLAY-LISTING.md](PLAY-LISTING.md) has the listing
copy and the rest of the store checklist.

The Kotlin package stays `dev.androidemu`. It is the JNI symbol prefix — every
`Java_dev_androidemu_Native_*` export in `native/` depends on it — and it is
independent of the package name users see.

## Cutting a release

```bash
git tag v0.2.0
git push origin v0.2.0
```

The tag push builds, tests, signs, and publishes a GitHub Release carrying
`emulia-0.2.0.aab`, `emulia-0.2.0.apk` and `SHA256SUMS.txt`. To get
artifacts without cutting a release, run the workflow manually from the Actions
tab; they attach to the run instead.

A **stable** tag also puts the desktop player on the same release:
`emulia-<version>-Linux-X64.tar.gz`, `-Linux-ARM64`, `-macOS-ARM64`,
`-macOS-X64` and `SHA256SUMS-desktop.txt`. Those come from
[`desktop.yml`](../.github/workflows/desktop.yml)'s `release-desktop` job,
which is why a stable tag runs the hosted matrix: macOS cannot be built on the
local Linux runner, and the local image is Ubuntu 24.04, so its Linux binary
needs a newer glibc than the ubuntu-22.04 build handed to everyone else. That
costs four hosted jobs per stable release, which is the trade for shipping
binaries people can actually run.

Release candidates deliberately do not get them: an `-rc` tag exists to put an
Android build in front of testers, and the desktop tarballs would be a bill
nobody asked for. Whichever of the two workflows reaches the release first
creates it and the other uploads into it, so their order does not matter.

This was done by hand for v0.2.5 and by nobody at all for v0.2.6, v0.2.7-rc1
and v0.3.0, which shipped without any desktop binaries. v0.3.1 is the first
release where it is automatic.

Before signing, the workflow runs the workspace tests, the pinned AccuracyCoin
regression gate, and the Android unit tests against the real host JNI library.
It rejects any lost recorded pass, unfinished test, incorrect ROM hash or timeout.
Reports are attached as `accuracycoin-android` and `android-unit-tests`, including
when their checks fail.

`versionName` comes from the tag with the `v` stripped. `versionCode` is
chronological — days since the project epoch, shifted four places, plus UTC
`HHMM` — computed the same way in the workflow and in
`android/app/build.gradle.kts`. Builds therefore order by when they were made
whichever produced them, so a release can never hand the tablet a code below the
one already installed. Android reads that as a downgrade and refuses to install;
Play rejects a reused code outright. Two builds in the same minute collide, which
is loud rather than silent.

## Publishing to Play from CI

A tag and the production setting decide the track. Nothing else changes about
how a release is cut. While production is disabled, stable releases still reach
the internal testers who installed the candidates.

| tag | goes to | as |
|---|---|---|
| `v0.2.3-rc1` | internal testing | live to testers |
| `v0.2.3`, production disabled (default) | internal testing | live to testers |
| `v0.2.3`, `PLAY_PRODUCTION_ENABLED=true` | production | a **draft**, waiting in the console |
| manual run | nowhere | artifacts on the run only |

**All of that is gated on one master switch, `PLAY_UPLOAD_ENABLED`, and it is
currently off.** The app was renamed from `com.bsteinfeld.emulia` to
`com.halfsteady.emulia`, and as far as Play is concerned that is not a rename
but a *new app*: a new listing, a new signing-key enrolment, a fresh version
code space, and — the part that blocks CI — a first bundle that Google will only
accept through the console by hand. The Developer API cannot seed a package it
has never seen. Until that upload happens, a tag builds, tests, signs, verifies
its signer and publishes its GitHub Release, then stops, with a notice in the
log saying why. Turn it on with `gh variable set PLAY_UPLOAD_ENABLED --body
true` once the first AAB is in the console and the service account has access to
the new app; after that every tag reaches Play on its own.

The upload is a **separate job** from the build, so a Play API failure — a
permission still propagating, a rejected version code, an expired key — costs
the upload and nothing else. The GitHub Release is already published by then,
and re-running the one job is cheap. It downloads the same `.aab` the build job
signed and verified, so Play and the GitHub Release get byte-identical files.

**Production is off.** Emulia is still a Draft app: production is Inactive and
the setup checklist is 8/11, with the Data safety form outstanding and the app
icon, feature graphic and phone screenshots not yet uploaded. The job exists and
is wired, but it is gated on a repository *variable*. Switch it on with `gh
variable set PLAY_PRODUCTION_ENABLED --body true`, and grant the service account
*Release to production* in the same sitting. `environment: play-production` is
where an approval gate goes — add required reviewers to that environment and a
production upload waits for a click.

**This was tried on 2026-09-12 and Play refused it**, which is worth recording
because the build side was blameless. With the variable set to `true`, `v0.2.6`
built, signed and verified, and the AAB reached Play — `Creating a new Edit`,
`Validating tracks: 'production'`, `Successfully uploaded 1 artifacts` — and then
committing the edit failed with **`Precondition check failed.`** That error is
the app being ineligible for the track, not a credential or a version code: the
same AAB, at the same version code, was accepted on internal testing minutes
later, so the abandoned edit burned nothing. Finishing the console checklist is
the only thing that changes this outcome, and no amount of workflow will
substitute for it.

Note the shape of the failure: with production enabled, a stable tag selects
*only* the production job, so the rejection left `v0.2.6` on no Play track at all
— the same stranding that [PR #5](https://github.com/bsteinfeld/androidemu/pull/5)
fixed for the disabled case, reappearing on the enabled path. It was recovered
with `play-internal.yml`. Until the checklist is done the variable stays `false`;
turning it on before then converts every stable release into a manual recovery.

**Without `PLAY_SERVICE_ACCOUNT_JSON` the workflow behaves exactly as it did
before**, printing a notice and skipping the upload. Setting it is what turns
this on.

### Recovering a missed internal upload

If a GitHub release exists but its AAB was never uploaded to Play, run
**Actions → Publish existing release to Play internal testing → Run workflow**
on `main`, supplying its tag (for example `v0.2.5`). Optional notes override the
first line of the GitHub release description. Supply plain-text notes when the
description contains Markdown formatting, which Play does not render.

```sh
gh workflow run play-internal.yml --ref main -f tag=v0.2.5
```

This downloads the existing AAB, verifies its published checksum, upload-key
fingerprint and signature, and creates a completed internal-testing release.
It does not rebuild the app, replace GitHub assets or change its version code.
Use it for a version that has not already been uploaded: Play rejects reused
version codes. Automatic uploads and recovery share a concurrency group so
they do not create competing Play edits.

The original `v0.2.5` build exposed the need for this recovery path: stable tags
previously selected only the disabled production job, so GitHub publication
succeeded while both Play jobs were skipped. Desktop release assets are
independent of Play publishing.

### One-time setup

1. **Google Cloud** — in the project linked to the Play account, enable the
   **Google Play Android Developer API**, create a service account (no roles
   needed on the Cloud side), then Keys ▸ Add key ▸ **JSON** and download it.
2. **Play Console** ▸ Users and permissions ▸ Invite new user, using the service
   account's email. Give it app access to Emulia only, with **Release to testing
   tracks**. Add *Release to production* only when production is being turned on.
   First-time permission propagation can take hours.
3. **The secret** — from wherever the key landed:

   ```sh
   gh secret set PLAY_SERVICE_ACCOUNT_JSON < ~/Downloads/<key>.json
   rm ~/Downloads/<key>.json     # it is a private key; do not leave it there
   ```

4. **Prove it** with a throwaway candidate: `git tag v0.2.4-rc1 && git push
   origin v0.2.4-rc1`, then watch the `publish-internal` job and check the
   release appears on the internal track.

The first upload for a package has to be made by hand. That was satisfied for
`com.bsteinfeld.emulia`, which carried internal releases up to 0.3.2-rc2, but
**it is not satisfied for `com.halfsteady.emulia`** — the app exists in the
console with no releases. Download the AAB from the GitHub Release and upload it
there once; then set `PLAY_UPLOAD_ENABLED`. Step 2's app access also has to be
granted again, on the new app: the old grant does not follow the rename.

### Version codes and candidates

Play rejects any version code it has ever seen, even from a deleted release, so
five RCs in a cycle consume five codes. The chronological scheme handles this
without help: `days-since-epoch × 10000 + UTC HHMM` increases with every minute
that passes, so a candidate can never collide with the release that follows it.
The only collision is two builds inside the same UTC minute.

## The signing key

Release builds are signed with an **upload key**, not the debug key. Play
Console rejects debug-signed uploads, and Android refuses to update an app whose
signature changed.

`android/upload-keystore.jks` and `android/keystore.properties` are gitignored.
**Back both up** — a password manager plus an off-machine copy. Losing the
upload key means requesting a reset from Google; leaking it lets someone else
sign as this app.

Local signed builds read `android/keystore.properties`:

```
storeFile=upload-keystore.jks
storePassword=<generated>
keyAlias=amelianes-upload   # predates the rename; an alias is internal
keyPassword=<generated>
```

CI has no such file, so it reads `ANDROIDEMU_UPLOAD_*` environment variables
instead, fed from repository secrets. If the keystore is absent entirely, the
build falls back to the debug key and produces something sideloadable but not
publishable.

One-time secret setup:

```bash
base64 -w0 android/upload-keystore.jks | gh secret set ANDROIDEMU_UPLOAD_KEYSTORE_BASE64
gh secret set ANDROIDEMU_UPLOAD_STORE_PASSWORD   # value from keystore.properties
gh secret set ANDROIDEMU_UPLOAD_KEY_PASSWORD     # same value
gh secret set ANDROIDEMU_UPLOAD_KEY_ALIAS        # amelianes-upload
```

Upload key SHA-256:
`75:FD:CC:B5:30:27:05:AB:CC:12:CF:62:8D:E6:34:CF:B6:1C:D5:7F:73:5D:33:39:39:84:97:8D:C3:1A:0F:D5`

## Building locally

```bash
cd android
./gradlew :app:bundleRelease :app:assembleRelease
```

`scripts/publish-apks.py` then copies both onto the `/stuff` shelf with their
hashes, refusing anything carrying the debug certificate.

## If you do go to Play

Play App Signing means Google holds the key that signs what users install, and
strips your upload key on the way through. A sideloaded APK from a GitHub
Release and the same version from Play therefore carry **different signatures**,
so the first Play install has to replace the sideload rather than update it.
Worth doing before there is a save file anyone minds losing.
