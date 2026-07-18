#!/usr/bin/env python3
"""Detect lines exceeding a character width threshold in Rust source files.

Usage:
  python3 find_long_lines.py [--threshold N] [--dir DIR]

Scans all .rs files under DIR (default: .), excluding target/, .git/, .claude/.
Reports file:line:length for any line exceeding THRESHOLD (default: 120).
"""

import os, sys


def find_long_lines(readable_dir: str, threshold: int):
    total = 0
    for root, dirs, files in os.walk(readable_dir):
        dirs[:] = [d for d in dirs if d not in ('target', '.claude', '.git')]
        for f in files:
            if not f.endswith('.rs'):
                continue
            path = os.path.join(root, f)
            with open(path) as fh:
                for lineno, line in enumerate(fh, 1):
                    length = len(line.rstrip('\n'))
                    if length > threshold:
                        print(f'{path}:{lineno}:{length}')
                        total += 1
    if total == 0:
        print(f'--- All lines ≤ {threshold} chars ---')
    else:
        print(f'\n--- {total} lines > {threshold} chars ---')


if __name__ == '__main__':
    threshold = 120
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
    find_long_lines(search_dir, threshold)
