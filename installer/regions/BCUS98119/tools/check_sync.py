#!/usr/bin/env python3
"""Report whether the US PPU patches still match the European ones they were rebased from.

  check_sync.py ROOT

The US patches in installer/regions/BCUS98119/patches/ are rebased copies of
installer/patches/infamous-recomp/recompiled/. make_lock.py records the European hashes in
eu-source.lock; this compares them with the checkout. A European change that has not been rebased
leaves the US build on the old code, and the US patches still apply, so nothing else notices.

  exit 0  in sync
  exit 1  drifted: rerun the steps in README.md
  exit 2  eu-source.lock is missing or the arguments are wrong
"""

import hashlib
import sys
from pathlib import Path

REGION = "installer/regions/BCUS98119"
EU = "installer/patches/infamous-recomp/recompiled"


def main(root: Path) -> int:
    lock = root / REGION / "eu-source.lock"
    if not lock.exists():
        print(f"{lock} is missing: write it with make_lock.py (README.md)")
        return 2
    recorded = {}
    for line in lock.read_text("utf8").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        digest, name = line.split()
        recorded[name] = digest

    drifted = 0
    for name in sorted(recorded):
        patch = root / EU / name
        if not patch.exists():
            print(f"  removed: {name}")
            drifted += 1
            continue
        if hashlib.sha256(patch.read_bytes()).hexdigest() != recorded[name]:
            print(f"  changed: {name}")
            drifted += 1
    for patch in sorted((root / EU).glob("*.patch")):
        if patch.name not in recorded:
            print(f"  added:   {patch.name}")
            drifted += 1

    if drifted:
        print(f"\n{drifted} European PPU patch file(s) differ from the set the US patches came from.")
        print("The US build keeps the old patches until they are regenerated: rerun README.md's steps.")
        return 1
    print(f"US PPU patches in sync ({len(recorded)} European patch files unchanged).")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(Path(sys.argv[1]).resolve()))
