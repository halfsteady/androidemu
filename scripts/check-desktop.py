#!/usr/bin/env python3
"""Exercise the real SDL frontend with a generated NROM, no external ROMs needed."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=Path("target/debug/nes-desktop"))
    parser.add_argument("--require-native-drivers", action="store_true",
                        help="also verify that SDL was built with real desktop video and audio backends")
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    env = dict(os.environ, SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy", SDL_RENDER_DRIVER="software")
    with tempfile.TemporaryDirectory(prefix="emulia-smoke-") as directory:
        root = Path(directory)
        # Battery-backed NROM with CHR RAM. Paint the backdrop, write SRAM, loop.
        rom = bytearray(16 + 16384)
        rom[:4] = b"NES\x1a"
        rom[4], rom[6] = 1, 2
        code = bytes.fromhex("78 a9 3f 8d 06 20 a9 00 8d 06 20 a9 21 8d 07 20 a9 5a 8d 00 60 4c 15 80")
        rom[16:16 + len(code)] = code
        rom[-6:] = bytes.fromhex("00 80 00 80 00 80")
        path = root / "test game.nes"
        path.write_bytes(rom)
        saves = root / "saved games"
        command = [binary, str(path), "--save-dir", str(saves), "--frames", "6"]

        def run(args, success=True):
            result = subprocess.run(args, env=env, capture_output=True, text=True, timeout=30)
            assert (result.returncode == 0) == success, result.stdout + result.stderr
            return result

        run([binary, "--help"])
        drivers = run([binary, "--list-drivers"]).stdout
        print(drivers, end="")
        if args.require_native_drivers:
            available = dict(line.split(": ", 1) for line in drivers.splitlines())
            video = set(available["Video"].split(", "))
            audio = set(available["Audio"].split(", "))
            if sys.platform == "darwin":
                assert "cocoa" in video and "coreaudio" in audio, drivers
            else:
                assert video & {"x11", "wayland"}, "SDL has no desktop display backend:\n" + drivers
                assert audio & {"alsa", "pulseaudio", "pipewire", "jack"}, "SDL has no desktop audio backend:\n" + drivers
        run(command)  # Includes SDL audio creation, queuing and playback.
        save, = saves.rglob("battery.sav")
        index = json.loads((saves / "library" / "index.json").read_text())
        assert len(index) == 1 and index[0]["title"] == "test game", index
        assert (save.parent / "game.nes").read_bytes() == rom
        data = save.read_bytes()
        assert len(data) == 8192 and data[0] == 0x5A
        # A byte the program does not touch must survive a second invocation.
        data = data[:1] + b"\xa5" + data[2:]
        save.write_bytes(data)
        run(command + ["--mute"])
        assert len(json.loads((saves / "library" / "index.json").read_text())) == 1, "a second import must dedupe"
        assert save.read_bytes() == data
        # A temporary autosave failure must leave the game running with its RAM
        # intact. Recover the directory before clean exit, then check the save.
        process = subprocess.Popen(command + ["--mute", "--frames", "480"], env=env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        blocked = save.with_suffix(f".tmp-{process.pid}")
        save_failed = threading.Event()
        errors = []

        def read_errors():
            for line in process.stderr:
                errors.append(line)
                if str(blocked) in line:
                    save_failed.set()

        reader = threading.Thread(target=read_errors, daemon=True)
        reader.start()
        try:
            blocked.write_bytes(b"temporary save conflict")
            assert save_failed.wait(timeout=15), "Expected a periodic autosave error"
            assert process.poll() is None, "Autosave failure closed the game"
            blocked.unlink()
            process.wait(timeout=30)
            reader.join(timeout=2)
            assert process.returncode == 0, process.stdout.read() + "".join(errors)
            assert save.read_bytes() == data
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            reader.join(timeout=2)
            process.stdout.close()
            process.stderr.close()
            blocked.unlink(missing_ok=True)
        # Refuse malformed saves: keep the bytes aside, write nothing over them.
        save.write_bytes(b"bad save")
        run(command, success=False)
        assert save.with_name("battery.sav.unreadable").read_bytes() == b"bad save"
        assert not save.exists(), "a session that lost progress must not write a fresh save"
        run([binary, str(path), "--frames", "0"], success=False)
        path.write_bytes(b"not a ROM")
        run(command, success=False)
    print("Desktop smoke passed: audio, muted pacing, library import, SRAM persistence, autosave recovery and invalid inputs")


if __name__ == "__main__":
    main()
