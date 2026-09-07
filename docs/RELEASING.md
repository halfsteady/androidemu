# Releasing Amelia's NES

One workflow produces everything: `.github/workflows/android-release.yml` builds
a signed **AAB** (what Play Console wants) and a signed **APK** (what you
sideload onto the tablet) from the same commit, so the two can never disagree.

- **App name:** Amelia's NES
- **Package (`applicationId`):** `com.bsteinfeld.amelianes`
- **Dev builds** install alongside it as `com.bsteinfeld.amelianes.debug`.

The Kotlin package stays `dev.androidemu`. It is the JNI symbol prefix — every
`Java_dev_androidemu_Native_*` export in `native/` depends on it — and it is
independent of the package name users see.

## Cutting a release

```bash
git tag v0.1.0
git push origin v0.1.0
```

The tag push builds, tests, signs, and publishes a GitHub Release carrying
`amelias-nes-0.1.0.aab`, `amelias-nes-0.1.0.apk` and `SHA256SUMS.txt`. To get
artifacts without cutting a release, run the workflow manually from the Actions
tab; they attach to the run instead.

`versionName` comes from the tag with the `v` stripped. `versionCode` is
chronological — days since the project epoch, shifted four places, plus UTC
`HHMM` — computed the same way in the workflow and in
`android/app/build.gradle.kts`. Builds therefore order by when they were made
whichever produced them, so a release can never hand the tablet a code below the
one already installed. Android reads that as a downgrade and refuses to install;
Play rejects a reused code outright. Two builds in the same minute collide, which
is loud rather than silent.

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
keyAlias=amelianes-upload
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
