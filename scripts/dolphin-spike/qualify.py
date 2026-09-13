#!/usr/bin/env python3
"""Exercise the debug Dolphin host with a local disc, in a NEW disposable directory."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--app', required=True, type=Path)
    parser.add_argument('--game', required=True, type=Path)
    parser.add_argument('--work-dir', required=True, type=Path)
    parser.add_argument('--wii', action='store_true')
    args = parser.parse_args()
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=False)
    command = work / 'command.json'
    status_path = command.with_suffix('.status')
    report = []
    env = dict(os.environ, EMULIA_DOLPHIN_PROBE_COMMAND=str(command))
    log = (work / 'host.log').open('w')
    process = subprocess.Popen(['python3', str(Path(__file__).with_name('run.py')),
        '--app', str(args.app.resolve()), '--data-dir', str(work / 'data'),
        str(args.game.resolve())], env=env, stdout=log, stderr=subprocess.STDOUT)

    def status():
        try:
            return json.loads(status_path.read_text())
        except (FileNotFoundError, json.JSONDecodeError):
            return {}

    def wait(predicate, label, seconds=60):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            if process.poll() is not None:
                raise RuntimeError(f'Host exited during {label}; see host.log')
            data = status()
            if data and predicate(data):
                return data
            time.sleep(.1)
        raise RuntimeError(f'Timed out: {label}; last status: {status()}')

    def send(action, **fields):
        wait(lambda s: not s['busy'], 'idle before ' + action)
        temp = command.with_suffix('.tmp')
        temp.write_text(json.dumps(dict(action=action, **fields)))
        temp.replace(command)
        wait(lambda _: not command.exists(), action + ' consumed')
        time.sleep(.25)
        return wait(lambda s: not s['busy'], action + ' completed')

    try:
        wait(lambda s: not s['busy'], 'boot')
        time.sleep(5)
        send('pause')
        data = send('save', slot=0)
        assert not data['message'], data
        game_dir = next((work / 'data/library').glob('dolphin-*'))
        saved = game_dir / 'slot-0.state'
        assert saved.stat().st_size > 1024
        assert (game_dir / 'slot-0.png').stat().st_size > 100
        send('resume')
        time.sleep(1)
        send('pause')
        assert not send('load', slot=0)['message']
        report.append('save/load round trip and PNG thumbnail')
        (game_dir / 'slot-1.state').write_bytes(b'invalid qualification state')
        assert send('load', slot=1)['message']
        send('dismiss')
        (game_dir / 'slot-1.state').unlink()
        digest = hashlib.sha256(saved.read_bytes()).hexdigest()
        staging = game_dir / 'slot-0.dolphin-staging'
        staging.mkdir()
        assert send('save', slot=0)['message']
        assert hashlib.sha256(saved.read_bytes()).hexdigest() == digest
        staging.rmdir()
        send('dismiss')
        report.append('invalid load rejected; failed replacement preserves old slot')
        if args.wii:
            for style in (1, 2, 3):
                send('style', slot=style)
                send('reset')
                wait(lambda s: s['native_style'] == style and not s['busy'], 'Wii style boot')
                send('resume')
                data = send('key', name='X', value=1)
                assert data['evaluated']['Wii/Buttons/A'] == 1
                if style == 3:
                    assert data['evaluated']['Classic/Buttons/A'] == 1
                send('key', name='X', value=0)
                if style == 2:
                    data = send('key', name='Left Shift', value=1)
                    assert data['evaluated']['Nunchuk/Buttons/C'] == 1
                    send('key', name='Left Shift', value=0)
                data = send('axis', name='rightx', value=20000)
                assert 0 < data['evaluated']['Wii/IR/Right'] < 1
                send('axis', name='rightx', value=0)
                send('pause')
                report.append(f'Wii style {style}: native configuration and controller expressions')
        else:
            send('resume')
            assert send('key', name='X', value=1)['native_pad'] & 0x100
            send('key', name='X', value=0)
            assert ((send('axis', name='leftx', value=20000)['native_pad'] >> 16) & 255) > 128
            send('axis', name='leftx', value=0)
            trigger = (send('axis', name='lefttrigger', value=20000)['native_pad'] >> 32) & 255
            assert 0 < trigger < 255
            send('axis', name='lefttrigger', value=0)
            send('pause')
            send('bind', name='GC/Buttons/A')
            send('reset')
            time.sleep(3)
            send('resume')
            assert send('key', name='Q', value=1)['native_pad'] & 0x100
            send('key', name='Q', value=0)
            send('pause')
            report.append('GameCube button, analog stick/trigger and persistent remap across restart')
        # Quit goes through capture/autosave, then native teardown.
        command.write_text('{"action":"quit"}')
        assert process.wait(timeout=60) == 0
        assert (game_dir / 'auto.state').stat().st_size > 1024
        report.append('autosave and clean shutdown')
        (work / 'report.json').write_text(json.dumps(report, indent=2))
        print('\n'.join(report))
    finally:
        if process.poll() is None:
            print(f'Qualification did not finish; inspect the open app and {work}/host.log')
        log.close()


if __name__ == '__main__':
    main()
