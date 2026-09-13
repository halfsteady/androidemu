#!/usr/bin/env python3
"""Build a pinned, opt-in Dolphin host inside Emulia's existing macOS shell."""
import argparse
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
from patch_state import apply as patch_state

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
REV = 'c77bbaa0f372c3f72281602a8b087206706542cb'  # Dolphin 2606a
SUBMODULES = '''mGBA/mgba libusb/libusb spirv_cross/SPIRV-Cross SDL/SDL zlib-ng/zlib-ng
libspng/libspng VulkanMemoryAllocator cubeb/cubeb implot/implot rcheevos/rcheevos
curl/curl fmt/fmt lz4/lz4 xxhash/xxHash enet/enet hidapi/hidapi-src tinygltf/tinygltf
minizip-ng/minizip-ng Vulkan-Headers watcher/watcher SFML/SFML zstd/zstd
miniupnpc/miniupnp glslang/glslang pugixml/pugixml cpp-ipc/cpp-ipc
cpp-optparse/cpp-optparse bzip2/bzip2 imgui/imgui'''.split()
STANZA = '''
# Emulia opt-in probe
if(EMULIA_DOLPHIN_SPIKE_SOURCE)
  add_subdirectory("${EMULIA_DOLPHIN_SPIKE_SOURCE}" "${CMAKE_BINARY_DIR}/emulia-probe")
endif()
'''

def run(*args, cwd=ROOT):
    subprocess.run([str(a) for a in args], cwd=cwd, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--work-dir', type=Path, default=ROOT / 'target/dolphin-spike')
    parser.add_argument('--jobs', type=int, default=6)
    parser.add_argument('--release', action='store_true', help='Build an optimized host without the debug qualification harness')
    parser.add_argument('--desktop-app', type=Path, help='Also package a Finder-launchable Emulia.app in this output directory')
    args = parser.parse_args()
    if sys.platform != 'darwin':
        parser.error('This rendering prototype currently supports macOS only')
    if args.jobs < 1:
        parser.error('--jobs must be positive')
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)
    executable = work / 'EmuliaDolphinProbe.app/Contents/MacOS/Emulia'
    processes = subprocess.check_output(['ps', '-axo', 'command='], text=True).splitlines()
    executables = [executable]
    if args.desktop_app:
        executables.append(args.desktop_app.resolve() / 'Emulia.app/Contents/MacOS/Emulia')
    if any(line.startswith(str(path) + ' ') or line == str(path)
           for path in executables for line in processes):
        raise RuntimeError('Close the target app before rebuilding it')
    geometry_test = work / 'pointer-geometry-test'
    run('c++', '-std=c++20', HERE / 'pointer_geometry_test.cpp', '-o', geometry_test)
    run(geometry_test)
    source, build = work / 'source', work / 'build'
    if not source.exists():
        run('git', 'clone', '--depth', '1', '--branch', '2606a',
            'https://github.com/dolphin-emu/dolphin.git', source)
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
    if actual != REV:
        raise RuntimeError(f'Expected Dolphin {REV}, found {actual}')
    run('git', 'submodule', 'update', '--init', '--depth', '1', '--jobs', args.jobs,
        '--', *['Externals/' + p for p in SUBMODULES], cwd=source)
    run('git', 'submodule', 'update', '--init', '--recursive', '--depth', '1',
        '--', 'Externals/cubeb/cubeb', cwd=source)
    cmake_file = source / 'Source/Core/CMakeLists.txt'
    original = subprocess.check_output(['git', 'show', f'{REV}:Source/Core/CMakeLists.txt'],
                                       cwd=source, text=True)
    if cmake_file.read_text() not in (original, original + STANZA):
        raise RuntimeError('Dolphin CMakeLists has other edits; use a fresh work directory')
    cmake_file.write_text(original + STANZA)
    patch_state(source, REV)
    disabled = ('QT', 'NOGUI', 'CLI_TOOL', 'TESTS', 'AUTOUPDATE', 'ANALYTICS',
                'VULKAN', 'LLVM', 'SDL')
    run('cmake', '-S', source, '-B', build, '-G', 'Unix Makefiles',
        '-DCMAKE_BUILD_TYPE=Release', '-DCMAKE_POSITION_INDEPENDENT_CODE=ON',
        f'-DEMULIA_DOLPHIN_SPIKE_SOURCE={HERE}',
        *[f'-DENABLE_{name}=OFF' for name in disabled],
        '-DUSE_DISCORD_PRESENCE=OFF', '-DUSE_MGBA=OFF', '-DUSE_RETRO_ACHIEVEMENTS=OFF',
        '-DENCODE_FRAMEDUMPS=OFF')
    run('cmake', '--build', build, '--target', 'emulia_dolphin_probe', '-j', args.jobs)
    # Separate output prevents a prototype build replacing the normal desktop binary.
    rust_target = work / 'rust-target'
    run('cargo', 'build', '--locked', '--config', 'profile.dev.package.sha2.opt-level=3', '--config', 'profile.dev.package.nes-desktop.opt-level=1', '-p', 'nes-desktop', '--features',
        'dolphin-embed-probe', '--target-dir', rust_target, *(['--release'] if args.release else []))
    app = work / 'EmuliaDolphinProbe.app'
    contents = app / 'Contents'
    (contents / 'MacOS').mkdir(parents=True, exist_ok=True)
    (contents / 'Resources').mkdir(exist_ok=True)
    (contents / 'Frameworks').mkdir(exist_ok=True)
    shutil.copy2(rust_target / ('release' if args.release else 'debug') / 'nes-desktop', contents / 'MacOS/Emulia')
    shutil.copy2(build / 'emulia-probe/libemulia_dolphin_probe.dylib',
                 contents / 'Frameworks/libemulia_dolphin_probe.dylib')
    sys_path = contents / 'Resources/Sys'
    if sys_path.is_symlink():
        sys_path.unlink()
    shutil.copytree(source / 'Data/Sys', sys_path, dirs_exist_ok=True)
    shutil.copy2(source / 'COPYING', contents / 'Resources/DOLPHIN-COPYING')
    (contents / 'Resources/PROBE-SOURCE.txt').write_text(
        f'Local development probe, Dolphin revision {REV}.\nSource: {source}\n'
        f'Host bridge: {HERE}\nNot a self-contained distribution.\n')
    shutil.copy2(ROOT / 'desktop/packaging/icons/Emulia.icns', contents / 'Resources/Emulia.icns')
    (contents / 'Info.plist').write_bytes(plistlib.dumps(dict(
        CFBundleExecutable='Emulia', CFBundleIdentifier='com.bsteinfeld.emulia.dolphin-probe',
        CFBundleName='Emulia Dolphin Probe', CFBundlePackageType='APPL',
        CFBundleIconFile='Emulia.icns', NSHighResolutionCapable=True)))
    run('codesign', '--force', '--sign', '-', contents / 'Frameworks/libemulia_dolphin_probe.dylib')
    run('codesign', '--force', '--sign', '-', app)
    run('codesign', '--verify', '--deep', '--strict', app)
    if args.desktop_app:
        output = args.desktop_app.resolve()
        run(sys.executable, ROOT / 'scripts/build-desktop.py', 'app', '--skip-build',
            '--binary', contents / 'MacOS/Emulia', '--output', output)
        desktop_app = output / 'Emulia.app'
        desktop_contents = desktop_app / 'Contents'
        shutil.copytree(contents / 'Frameworks', desktop_contents / 'Frameworks', dirs_exist_ok=True)
        shutil.copytree(contents / 'Resources', desktop_contents / 'Resources', dirs_exist_ok=True)
        # This local native build has not been qualified on older macOS versions.
        info_path = desktop_contents / 'Info.plist'
        info = plistlib.loads(info_path.read_bytes())
        info['LSMinimumSystemVersion'] = subprocess.check_output(
            ['sw_vers', '-productVersion'], text=True).strip()
        info_path.write_bytes(plistlib.dumps(info))
        run('codesign', '--force', '--sign', '-', desktop_app)
        run('codesign', '--verify', '--deep', '--strict', desktop_app)
        print(f'Desktop app: {desktop_app}')
    print(f'Probe app: {app}')
    print('Run using scripts/dolphin-spike/run.py --app APP --data-dir DIRECTORY GAME')

if __name__ == '__main__':
    main()
