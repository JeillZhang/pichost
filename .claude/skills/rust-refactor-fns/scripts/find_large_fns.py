#!/usr/bin/env python3
"""Detect Rust functions exceeding a line count threshold.

Usage:
  python3 find_large_fns.py [--threshold N] [--dir DIR]

Scans all .rs files under DIR (default: .), excluding target/, .git/, .claude/.
Prints functions exceeding THRESHOLD (default: 50) sorted by line count descending.
"""

import os, re, sys


def find_fns(readable_dir: str, threshold: int):
    total = 0
    for root, dirs, files in os.walk(readable_dir):
        dirs[:] = [d for d in dirs if d not in ('target', '.claude', '.git')]
        for f in files:
            if not f.endswith('.rs'):
                continue
            path = os.path.join(root, f)
            with open(path) as fh:
                lines = fh.readlines()

            STATE_IDLE, STATE_IN_SIG, STATE_IN_BODY = 0, 1, 2
            state, fn_start, fn_name, brace = STATE_IDLE, 0, '', 0

            for i, line in enumerate(lines):
                s = line.strip()
                if state == STATE_IDLE:
                    m = re.match(
                        r'^(pub(\s*\(crate\))?\s+)?(async\s+)?fn\s+(\w+)', s
                    )
                    if m:
                        fn_name = m.group(4)
                        fn_start = i
                        if '{' in s:
                            brace = s.count('{') - s.count('}')
                            state = STATE_IN_BODY
                        else:
                            state = STATE_IN_SIG
                            brace = 0
                elif state == STATE_IN_SIG:
                    if '{' in s:
                        brace = s.count('{') - s.count('}')
                        state = STATE_IN_BODY
                    elif ';' in s:
                        state = STATE_IDLE  # trait fn, skip
                elif state == STATE_IN_BODY:
                    brace += s.count('{') - s.count('}')
                    if brace == 0:
                        lc = i - fn_start + 1
                        if lc > threshold:
                            print(f'{lc:4}L  {path}:{fn_start+1}  {fn_name}')
                            total += 1
                        state = STATE_IDLE
    if total == 0:
        print(f'--- All functions ≤ {threshold} lines ---')
    else:
        print(f'\n--- {total} functions > {threshold} lines ---')


if __name__ == '__main__':
    threshold = 50
    search_dir = '.'
    args = sys.argv[1:]
    i = 0
    while i < len(args):
        if args[i] == '--threshold' and i + 1 < len(args):
            threshold = int(args[i + 1])
            i += 2
        elif args[i] == '--dir' and i + 1 < len(args):
            search_dir = args[i + 1]
            i += 2
        elif args[i] in ('-h', '--help'):
            print(__doc__)
            sys.exit(0)
        else:
            i += 1
    find_fns(search_dir, threshold)
