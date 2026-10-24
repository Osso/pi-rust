#!/usr/bin/python3
import json
import os
import pathlib
import sys
import time

root = pathlib.Path(os.environ['PI_CLIPBOARD_TEXT_STATE'])
config = json.loads((root / 'config.json').read_text())
name = pathlib.Path(sys.argv[0]).name
with (root / 'trace.jsonl').open('a') as trace:
    trace.write(json.dumps({'program': name, 'args': sys.argv[1:]}, separators=(',', ':')) + '\n')
if name != 'which':
    time.sleep(config.get('delayRead', {}).get(name, 0))
    if name not in config.get('earlyExit', []):
        (root / f'{name}.input').write_bytes(sys.stdin.buffer.read())
(root / f'{name}.finished').write_text('finished')
sys.exit(config['tools'][name])
