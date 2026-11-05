#!/usr/bin/env python3
"""Pipeline steps that differ between the European and US executables.

The SPU programs in the two executables are byte-identical but sit 0x500 bytes
apart, so these steps give the rest of the pipeline the same inputs for both:

  pm-carve ELF OUT   carve the WWS job manager policy module (found by hash)
  spu-names DIR      name the extracted SPU images as in the European executable

extract_spu_images.py puts the file offset in each name and that name reaches the
generated sources, so renaming is what lets one SPU patch set serve both releases.
"""
import hashlib
import sys
from pathlib import Path

PM_SHA256 = "2fedf78bc38dd708d5e950c220baec9e987db677cf776614fc4bdf5e289d1b56"
PM_SIZE = 0x2D80
PM_OFFSETS = {"BCES00220": 0x68CD80, "BCUS98119": 0x68C880}

# Image index -> (European file offset, US file offset).
SPU_OFFSETS = {
    0: (0x5F6500, 0x5F6000),
    1: (0x674380, 0x673E80),
    2: (0x68FE00, 0x68F900),
    3: (0x6A3800, 0x6A3300),
    4: (0x6BF480, 0x6BEF80),
    5: (0x6D7700, 0x6D7200),
    6: (0x6F1E00, 0x6F1900),
    7: (0x700600, 0x700100),
}


def pm_carve(elf: Path, out: Path) -> int:
    data = elf.read_bytes()
    for release, offset in PM_OFFSETS.items():
        module = data[offset : offset + PM_SIZE]
        if hashlib.sha256(module).hexdigest() == PM_SHA256:
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(module)
            print(f"policy module: {release} layout, offset {offset:#x}")
            return 0
    print(f"error: no known policy module in {elf}", file=sys.stderr)
    return 1


def spu_names(images: Path) -> int:
    renamed = 0
    for index, (eu, us) in SPU_OFFSETS.items():
        source = images / f"spu_{index:04d}_at_{us:08X}.elf"
        target = images / f"spu_{index:04d}_at_{eu:08X}.elf"
        if source.is_file():
            source.replace(target)
            renamed += 1
        elif not target.is_file():
            print(f"error: SPU image {index} is missing from {images}", file=sys.stderr)
            return 1
    print(f"SPU images: {renamed} renamed, {len(SPU_OFFSETS) - renamed} already named")
    return 0


def main(argv: list[str]) -> int:
    if len(argv) == 3 and argv[0] == "pm-carve":
        return pm_carve(Path(argv[1]), Path(argv[2]))
    if len(argv) == 2 and argv[0] == "spu-names":
        return spu_names(Path(argv[1]))
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
