#!/usr/bin/env python3
"""Package the native Rust desktop shell on macOS or Linux (no Java runtime)."""
import argparse
import gzip
import hashlib
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
APP_ID = 'com.bsteinfeld.emulia'
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text())['workspace']['package']['version']


def run(*args, **kwargs):
    return subprocess.run([str(a) for a in args], check=True, **kwargs)


def copy(source, dest):
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, dest)


def source_archive(out):
    manifest = ROOT / 'SOURCE-MANIFEST.txt'
    if manifest.is_file():
        names = manifest.read_text().splitlines()
    else:
        names = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=ROOT).decode().split('\0')
    names = sorted(set(n for n in names if n and n != 'SOURCE-MANIFEST.txt' and (ROOT / n).is_file() and not (ROOT / n).resolve().is_relative_to(out)))
    path = out / f'emulia-{VERSION}-source.tar.gz'
    with path.open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', filename='', mtime=0) as gz, tarfile.open(fileobj=gz, mode='w') as archive:
        for name in names:
            source = ROOT / name
            info = archive.gettarinfo(str(source), arcname=f'emulia-{VERSION}/{name}')
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ''
            with source.open('rb') as data:
                archive.addfile(info, data)
        import io
        data = ('\n'.join(names) + '\n').encode()
        info = tarfile.TarInfo(f'emulia-{VERSION}/SOURCE-MANIFEST.txt')
        info.size = len(data)
        archive.addfile(info, io.BytesIO(data))
    return path


def notices(dest):
    copy(ROOT / 'LICENSES/GPL-3.0.txt', dest / 'COPYING')
    copy(ROOT / 'vendor/jgenesis/EMULIA-PATCHES.md', dest / 'JGENESIS-PATCHES.md')
    (dest / 'SOURCE.txt').write_text(
        f'Emulia {VERSION}, native Rust desktop shell.\n'
        'Combined distribution: GPL-3.0-only. Original component notices remain in the source.\n'
        'Includes jgenesis SNES; see JGENESIS-PATCHES.md for its pinned revision and modifications.\n'
        f'Corresponding project source accompanies this package as emulia-{VERSION}-source.tar.gz.\n'
        'Build instructions: docs/DESKTOP.md and docs/DESKTOP-PACKAGING.md in that archive.\n')


def mac(binary, stage):
    app = stage / 'Emulia.app'
    contents = app / 'Contents'
    copy(binary, contents / 'MacOS/Emulia')
    copy(ROOT / 'desktop/packaging/icons/Emulia.icns', contents / 'Resources/Emulia.icns')
    notices(contents / 'Resources')
    info = dict(CFBundleName='Emulia', CFBundleDisplayName='Emulia', CFBundleExecutable='Emulia',
                CFBundleIdentifier=APP_ID, CFBundlePackageType='APPL', CFBundleIconFile='Emulia.icns',
                CFBundleShortVersionString=VERSION, CFBundleVersion=VERSION, NSHighResolutionCapable=True,
                LSMinimumSystemVersion='11.0', NSHumanReadableCopyright='Emulia and contributors; GPL-3.0-only',
                CFBundleDocumentTypes=[dict(CFBundleTypeName='NES / SNES cartridge', CFBundleTypeRole='Viewer',
                                           LSHandlerRank='Alternate', CFBundleTypeExtensions=['nes', 'sfc', 'smc'])])
    (contents / 'Info.plist').write_bytes(plistlib.dumps(info))
    run('codesign', '--force', '--sign', '-', app)
    run('codesign', '--verify', '--deep', '--strict', app)
    return app


def linux(binary, stage):
    copy(binary, stage / 'opt/emulia/bin/Emulia')
    bindir = stage / 'usr/bin'
    bindir.mkdir(parents=True)
    (bindir / 'emulia').symlink_to('/opt/emulia/bin/Emulia')
    copy(ROOT / f'desktop/packaging/linux/{APP_ID}.desktop', stage / f'usr/share/applications/{APP_ID}.desktop')
    copy(ROOT / f'desktop/packaging/linux/{APP_ID}.metainfo.xml', stage / f'usr/share/metainfo/{APP_ID}.metainfo.xml')
    copy(ROOT / 'desktop/packaging/icons/Emulia.png', stage / f'usr/share/icons/hicolor/512x512/apps/{APP_ID}.png')
    notices(stage / 'usr/share/doc/emulia')


def deb(stage, out):
    arch = subprocess.check_output(['dpkg', '--print-architecture'], text=True).strip()
    control = stage / 'DEBIAN'
    control.mkdir()
    # shlibdeps discovers directly linked dependencies; SDL also loads desktop backends at runtime.
    with tempfile.TemporaryDirectory(prefix='emulia-shlibdeps-') as tmp:
        cwd = Path(tmp)
        (cwd / 'debian').mkdir()
        (cwd / 'debian/control').write_text('Source: emulia\nSection: games\nPriority: optional\nMaintainer: Emulia contributors <noreply@github.com>\n\nPackage: emulia\nArchitecture: any\nDescription: Emulia desktop emulator\n')
        found = subprocess.check_output(['dpkg-shlibdeps', '-O', '-e' + str(stage / 'opt/emulia/bin/Emulia')], cwd=cwd, text=True).strip()
    deps = found.removeprefix('shlibs:Depends=')
    (control / 'control').write_text(
        f'Package: emulia\nVersion: {VERSION}\nArchitecture: {arch}\nSection: games\nPriority: optional\n'
        'Maintainer: Emulia contributors <noreply@github.com>\n'
        f'Depends: {deps}, libgl1, libx11-6, libasound2 | libasound2t64, libpulse0\n'
        'Recommends: xdg-desktop-portal\n'
        'Homepage: https://github.com/bsteinfeld/androidemu\n'
        'Description: NES and SNES desktop emulator\n Local game library, save states, rewind and gamepad support.\n')
    path = out / f'emulia_{VERSION}_{arch}.deb'
    run('dpkg-deb', '--root-owner-group', '--build', stage, path)
    return path


def rpm(stage, out, tmp):
    top = tmp / 'rpmbuild'
    for folder in ['BUILD', 'BUILDROOT', 'RPMS', 'SOURCES', 'SPECS', 'SRPMS']:
        (top / folder).mkdir(parents=True)
    spec = top / 'SPECS/emulia.spec'
    spec.write_text(f'''Name: emulia
Version: {VERSION}
Release: 1
Summary: NES and SNES desktop emulator
License: GPL-3.0-only
URL: https://github.com/bsteinfeld/androidemu
Requires: mesa-libGL, libX11, alsa-lib, pulseaudio-libs
%description
Local game library, save states, rewind and gamepad support.
%install
mkdir -p %{{buildroot}}
cp -a "{stage}/." %{{buildroot}}/
%files
/opt/emulia
/usr/bin/emulia
/usr/share/applications/{APP_ID}.desktop
/usr/share/metainfo/{APP_ID}.metainfo.xml
/usr/share/icons/hicolor/512x512/apps/{APP_ID}.png
/usr/share/doc/emulia
''')
    run('rpmbuild', '--define', f'_topdir {top}', '-bb', spec)
    built, = (top / 'RPMS').rglob('*.rpm')
    path = out / built.name
    copy(built, path)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('format', choices=['app', 'dmg', 'deb', 'rpm', 'sources'])
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/nes-desktop')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/packages')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if args.format in ['app', 'dmg'] and platform.system() != 'Darwin':
        parser.error('macOS packages must be built on macOS')
    if args.format in ['deb', 'rpm'] and platform.system() != 'Linux':
        parser.error('Linux packages must be built on Linux')
    if args.format != 'sources' and not args.skip_build:
        run('cargo', 'build', '--release', '--locked', '-p', 'nes-desktop', cwd=ROOT)
    artifacts = [source_archive(out)]
    if args.format != 'sources':
        with tempfile.TemporaryDirectory(prefix='emulia-package-') as directory:
            tmp = Path(directory)
            stage = tmp / 'stage'
            stage.mkdir()
            if args.format in ['app', 'dmg']:
                app = mac(args.binary.resolve(), stage)
                if args.format == 'app':
                    dest = out / 'Emulia.app'
                    if dest.exists():
                        shutil.rmtree(dest)
                    shutil.copytree(app, dest)
                    print(dest)
                else:
                    (stage / 'Applications').symlink_to('/Applications')
                    dest = out / f'Emulia-{VERSION}-{platform.machine()}.dmg'
                    run('hdiutil', 'create', '-ov', '-volname', 'Emulia', '-srcfolder', stage, '-format', 'UDZO', dest)
                    artifacts.append(dest)
            else:
                linux(args.binary.resolve(), stage)
                artifacts.append(deb(stage, out) if args.format == 'deb' else rpm(stage, out, tmp))
    for path in artifacts:
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        path.with_name(path.name + '.sha256').write_text(f'{digest}  {path.name}\n')
        print(path)


if __name__ == '__main__':
    main()
