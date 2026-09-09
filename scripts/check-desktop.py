#!/usr/bin/env python3
"""Exercise the real SDL frontend with a generated NROM, no external ROMs needed."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else "target/debug/nes-desktop").resolve())
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
        run(command)  # Includes SDL audio creation, queuing and playback.
        save, = saves.glob("*.sav")
        data = save.read_bytes()
        assert len(data) == 8192 and data[0] == 0x5A
        # A byte the program does not touch must survive a second invocation.
        data = data[:1] + b"\xa5" + data[2:]
        save.write_bytes(data)
        run(command + ["--mute"])
        assert save.read_bytes() == data
        # Refuse malformed saves without overwriting them.
        save.write_bytes(b"bad save")
        run(command, success=False)
        assert save.read_bytes() == b"bad save"
        run([binary, str(path), "--frames", "0"], success=False)
        path.write_bytes(b"not a ROM")
        run(command, success=False)
    print("Desktop smoke passed: video, audio, muted pacing, SRAM persistence and invalid inputs")


if __name__ == "__main__":
    main()
