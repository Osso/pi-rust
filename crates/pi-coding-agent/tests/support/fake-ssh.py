#!/usr/bin/python3
"""Sandbox fixture. Never connects to a host; records argv then runs local bash."""
import json
import os
import sys

with open(os.environ['SSH_FIXTURE_LOG'], 'a') as output:
    output.write(json.dumps({'argv': sys.argv[1:], 'stdin': sys.stdin.read()}) + '\n')
command = sys.argv[2]
if 'fail' in command or sys.argv[1] == 'bad':
    sys.stderr.write('scripted failure\n')
    sys.exit(7)
if command == 'pwd':
    print(os.environ['SSH_FIXTURE_REMOTE'])
    sys.exit(0)
os.execv('/bin/bash', ['bash', '-c', command])
