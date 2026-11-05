# US release (BCUS98119)

The installer uses this folder when the dump's title ID is `BCUS98119`:

* `generated.lock` replaces `installer/generated.lock`.
* `patches/` holds the PPU patches. Any patch that isn't here comes from `installer/patches/`.

## How the US executable differs from the European one

| Part | Difference |
|---|---|
| SPU programs and job modules | None: the pipeline's SPU output is byte-identical once `region_fixups.py` has normalized the names and offsets, so the patches are shared |
| PPU code | Same program, with functions shifted by small amounts per region |
| TOC and read-only pools | Shifted by -0x48 to -0x50, with TOC-relative offsets unchanged |
| BSS globals | Same addresses at every place checked |
| `assets/ui_font.bin` | Different bytes, because the disc data differs; not patched |

The hand-written sources use European addresses. `infamous-recomp/infamous_region.h` translates them
when CMake finds the US `PARAM.SFO` in the linked dump, and it changes nothing in a European build.

## Regenerating after the European patches change

The US patches are derived from the European ones, so they go stale when `installer/patches/` changes.
`installer` checks each result against its lock hash, so a stale set fails rather than building wrongly.

The scripts in `tools/` take the checkout (`ROOT`) and a work folder (`WORK`) that holds:

* `us/EBOOT.elf`, the decrypted US executable
* `out/ppu/`, from `ps3recomp/tools/ppu_loader.py`
* `out/recompiled/`, from `ps3recomp/tools/ppu_lifter.py`, with the arguments in `infamous.toml` and
  `PYTHONUTF8=1` set as the installer does (without it the comment bytes differ and the lock won't match)

Then, in order:

```bat
python classify_hunks.py ROOT WORK    :: split the PPU patches into mechanical and semantic hunks
python rebase_ppu.py ROOT WORK        :: writes WORK/out/us_final and the address map
python audit_rebase.py ROOT WORK      :: applies the header patch and reports what changed
python make_lock.py ROOT WORK/out/recompiled WORK/out/us_final WORK/us/EBOOT.elf <US ui_font.bin>
```

`rebase_ppu.py` applies the semantic hunks at the matching place in the US lift and translates their
addresses. It then applies the mechanical `stvx` rewrite to every matching line.

Read the semantic hunks in `WORK/semantic_report.json` before trusting a new set, then compile the result.
Any new hand-written address also needs an entry in `infamous_region.h`.

## Building and running the US port (done 2026-11-05)

1. Decrypt the disc's `EBOOT.BIN` to `EBOOT.elf` with RPCS3 (Utilities > Decrypt PS3 Binaries, or let the installer
   call `rpcs3 --decrypt`). The port's `infamous.toml` lists the US executable's sha256.
2. Build the installer from this checkout (`cd installer && cargo build --release`) and put
   `infamous-installer.exe` / `infamous-launch.exe` in `installer/bin/`: the upstream binaries know nothing about
   `installer/regions/`.
3. `installerin\infamous-installer.exe install --game "<dump folder>" --build-jobs 12` from the checkout root
   (outside OneDrive). The pipeline lifts 8476 PPU functions, 62 SPU images and 21 job modules, applies this folder's
   PPU patches plus the shared SPU patches, and builds with clang-cl. `INFAMOUS_BUILD_JOBS=<n>` sets the compile
   jobs for `Update inFamous.cmd` too; the five lifted PPU units are the memory hogs.
4. Optional: `dlss-sdk/` and `fsr-sdk/` next to the checkout before the build (README) for DLSS / FSR.

Status: the US build boots, shows the autosave notice (Enter), reaches the title screen and plays at 60 fps in
1080p with the upscaler off. On an RTX 5060 Laptop (driver 621.01, DLSS SDK 310.9.16) `UPSCALER=dlaa` hung the
GPU (DXGI_ERROR_DEVICE_HUNG) on the first in-world frame; run `play-dred.cmd` (adds `D3D12_DRED=1`) to get the
device-removed breadcrumbs when investigating. `make_kbm_prompts.py` carries the US texture IDs of the HUD prompt
textures (`core.xpp` object IDs differ per release); `make_kbm_icons.py` reads the pipeline's
`assets/font_icons_l0.bin`.
