# Native desktop packages

These commands package PR #6's Rust/SDL/egui shell, including the existing NES
core, the jgenesis SNES adapter and the external Dolphin launcher. Dolphin must
be installed separately and is not included in these packages. No Java runtime or Android SDK is needed.
Run them from a checkout using the pinned Rust toolchain, Python 3.11+, CMake
and the platform dependencies in [DESKTOP.md](DESKTOP.md).

## macOS

```sh
python3 scripts/build-desktop.py app
python3 scripts/build-desktop.py dmg --skip-build
open target/packages/Emulia.app
```

The app contains the native executable, the established Emulia icon, a bundle
identifier, game image document types, and license/source notices. The DMG has
an Applications shortcut. Builds use the host architecture (Apple Silicon or
Intel). Local packages are ad-hoc signed; Developer ID signing and notarization
remain release work. The app is not a universal binary.

### Local app with embedded Dolphin

For the opt-in embedded Dolphin integration, build an optimized, Finder-launchable
app (including its native backend and resources):

```sh
python3 scripts/dolphin-spike/build.py --release --desktop-app target/packages-embedded
ditto target/packages-embedded/Emulia.app /Applications/Emulia.app
open /Applications/Emulia.app
```

The installed app uses the regular Emulia library. The development preview's
separate data directory remains separate. No environment variables are needed;
the feature-enabled host discovers its bundled backend. This is a local build
for the current Mac with Homebrew dependencies, not a portable release package.

## Debian / Ubuntu, GNOME and KDE

```sh
sudo apt-get install python3 dpkg-dev desktop-file-utils appstream
python3 scripts/build-desktop.py deb
sudo apt install ./target/packages/emulia_0.1.0_*.deb
```

The package installs `/opt/emulia/bin/Emulia`, `/usr/bin/emulia`, an application
menu entry, a hicolor icon and AppStream metadata. X11 WM_CLASS and the Wayland
application ID match `com.bsteinfeld.emulia`, so GNOME and KDE can associate
windows with their launcher icon. File selection uses the desktop portal when
available. No desktop settings or file defaults are changed by the package.
Build on the oldest supported distribution for the desired CPU architecture;
`dpkg-shlibdeps` records the binary's required library versions. SDL's dynamically
loaded display/audio backends have additional explicit dependencies.

RPM packaging is also available on Linux with `rpmbuild` installed:

```sh
python3 scripts/build-desktop.py rpm --skip-build
```

The same desktop entry and metadata serve GNOME and KDE. An RPM built on Debian
is useful for inspecting package contents; qualify installation on the target
RPM distribution before distributing it.

## Outputs and source

Outputs default to `target/packages/`. `--output DIR` changes this, and
`--binary PATH --skip-build` packages an already built native executable.
Every invocation writes the corresponding project source archive and SHA-256
sidecars. Distribute that source archive alongside binary packages. It includes
the pinned jgenesis sources, local patch ledger, Cargo lockfile and build scripts.
Building the source archive requires network access to retrieve locked Cargo
dependencies. The combined application is distributed under GPL-3.0-only;
original component notices remain in the source.

Icons are committed. To regenerate them from the existing brand SVG, install
librsvg and Pillow, then run `python3 scripts/build-desktop-icons.py`.
