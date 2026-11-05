#!/usr/bin/env python3
"""Write this release's generated.lock and PPU patches from a lift and its rebased copy.

  make_lock.py ROOT LIFT_DIR FINAL_DIR ELF UI_FONT

ROOT      the checkout
LIFT_DIR  ppu_lifter.py's output for the US executable
FINAL_DIR the same files after rebase_ppu.py (see README.md)
ELF       the decrypted US EBOOT.elf
UI_FONT   assets/ui_font.bin as the pipeline makes it from the US disc

SPU files are byte-identical to the European pipeline's, so their lock lines and
patches are shared. Only the PPU sources and the font have US entries.
"""
import hashlib
import re
import subprocess
import sys
from pathlib import Path

RECOMP = "infamous-recomp/recompiled/"
FONT = "infamous-recomp/assets/ui_font.bin"
NAMES = ["ppu_recomp.h"] + [f"ppu_recomp_{i:03d}.cpp" for i in range(5)]


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def patch(lift: Path, final: Path, rel: str) -> bytes:
    run = subprocess.run(
        ["git", "-c", "core.autocrlf=false", "-c", "core.safecrlf=false", "diff", "--no-index", "--no-color",
         "--no-renames", "-U3", str(lift), str(final)],
        capture_output=True,
        cwd=lift.parent,  # outside the checkout: its `* text=auto` would rewrite the line endings
    )
    if run.returncode != 1:
        raise SystemExit(f"git diff failed for {rel}: {run.stderr.decode(errors='replace')}")
    body = run.stdout[run.stdout.index(b"\n@@ ") + 1 :]
    return f"--- a/{rel}\n+++ b/{rel}\n".encode() + body


def main(root: Path, lift: Path, final: Path, elf: Path, font: Path) -> int:
    region = root / "installer/regions/BCUS98119"
    out = []
    for line in (root / "installer/generated.lock").read_text("utf8").splitlines():
        parts = line.split(None, 3)
        if line.startswith("eboot "):
            out.append(f"eboot {sha(elf)}")
        elif len(parts) == 4 and parts[3].startswith(RECOMP):
            continue
        elif len(parts) == 4 and parts[3] == FONT:
            out.append(f"file {sha(font)} {sha(font)} {FONT}")
        else:
            out.append(line)
    for name in NAMES:
        rel = RECOMP + name
        base, fin = lift / name, final / name
        out.append(f"file {sha(base)} {sha(fin)} {rel}")
        target = region / "patches" / f"{rel}.patch"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(patch(base, fin, rel))
    (region / "generated.lock").write_bytes(("\n".join(out) + "\n").encode())
    print(f"{len(out)} lock lines, {len(NAMES)} patches")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 6:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(*map(Path, sys.argv[1:])))
