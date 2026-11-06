# inFamous PC (US Release - BCUS98119)

> **Dedicated US Recompilation Project (`BCUS98119`)**  
> Maintained separately for the North American / US release.  
> Based on upstream recompiler foundation from [sp00nznet/ps3recomp](https://github.com/sp00nznet/ps3recomp) and [whompay/inFAMOUSreborn](https://github.com/whompay/inFAMOUSreborn).

An unofficial native Windows port of **inFamous** (PlayStation 3, 2009, North American / US
release `BCUS98119`). The game's PowerPC (PPU) and SPU code is statically
recompiled to x86-64 C/C++ with [ps3recomp](https://github.com/sp00nznet/ps3recomp),
running natively on Windows Direct3D 12 rather than under an emulator.

**This repository contains no game code or data.** You build the game yourself,
on your own PC, from your own copy of the game.

## US Release Architecture & Optimizations

This dedicated US release tree integrates key structural rebases and optimizations:

* **PPU Address Translation & Rebase:** Uses custom US PPU patches in `installer/regions/BCUS98119/` and dynamic guest address translation via `infamous-recomp/infamous_region.h` to cleanly align US functions, TOC offsets, and static BSS globals.
* **US HUD Prompt & Font Glyphs:** Maps US-specific `core.xpp` texture object IDs for on-screen controller and keyboard/mouse prompts using `make_kbm_prompts.py` and `make_kbm_icons.py`.
* **Rooftop & Elevation Stutter Fix:**
  - **Asymmetric Adaptive LOD:** Fast reduction (EMA tau=6 frames / 0.4s) and gradual recovery (EMA tau=30 frames / 3.5s) with `ADAPT_LOD=1` and `ADAPT_LOD_MIN=0.85` prevents draw-call explosions (25k+ draws/frame) when overlooking Empire City.
  - **Synchronized PPU Dynamic Actor Scaling:** Multiplies the EBOOT `.data` LOD table (`0x0078AD30`) by `g_v237_lodf` in `rsx_d3d12_backend.c`, ensuring cars, pedestrians, and dynamic props scale down symmetrically with SPU geometry jobs.
* **Dual-CCD & Multi-Core Affinity Shielding:** Multi-CCD processors (such as AMD Ryzen 9 7950X / 9950X) are protected from high-latency Infinity Fabric stalls (`[TASKWAIT]`) by keeping critical SPU worker threads pinned to CCD 0 (`SPURS_PCORE_WIDS=0,1,2,6`, `V221A_PLACE=0`).

## Requirements

- Your own inFamous US disc dump: `BCUS98119`.
- Decrypted `EBOOT.elf` (SHA-256: `4f2db3fb7c76fbb4f80cbc8445cf0ea1570a664996a7787e690efcc1a39befb4`).
- Windows 10 or 11 (64-bit) and a GPU with Direct3D 12 support.
- A modern multi-core CPU. Multi-CCD AMD Ryzen and Intel hybrid CPUs are natively supported.
- [Python 3.10+](https://www.python.org/downloads/) with "Add python.exe to PATH" ticked.
- [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/)
  with the **Desktop development with C++** workload and the **C++ Clang
  Compiler for Windows** component.
- [RPCS3](https://rpcs3.net), only if your `PS3_GAME\USRDIR` has `EBOOT.BIN`
  but no decrypted `EBOOT.elf`. Setup uses it to decrypt the executable.
- About 10 GB of free disk space and 16 GB of RAM.

## Quick Launch

To play:
1. Double click **`Play inFamous.cmd`** in the root directory.
2. If diagnosing GPU device lost or investigating rendering hangs, run **`play-dred.cmd`** (writes extended D3D12 breadcrumbs to `play-dred.err.log`).

## Easy install

Download or run the installer targeting your `BCUS98119` folder (the one containing `PS3_GAME`), pick an install location, and press **Install**.

## Updating

When updates are published to this repository, run `Update inFamous.cmd` in the root folder. The updater compares files, pulls updates, and recompiles only what changed.

## Building manually

```bat
cd installer
cargo build --release
target\release\infamous-installer --game "D:\Dumps\inFamous [BCUS98119]"
```

`infamous-installer doctor` checks the requirements only. Finished steps are
skipped when you run it again, so an interrupted install continues where it
stopped. See [installer/README.md](installer/README.md) for every step and the
maintainer commands.

## Playing

Run **inFamous** (the shortcut Setup made). It opens a small options window first
(vsync, frame cap, resolution, language and more, saved to `settings.ini`);
**Play** starts the game. The same settings are in the game's pause menu under
**Settings**. Controllers work as on the console, and keyboard and mouse are supported.

### Controls and button prompts

Xbox pads (and Steam Input's virtual pads) work through XInput; DualShock 4, DualSense, Switch Pro, Joy-Con,
Steam Controller and most other pads work through SDL3, with the community mappings in
`sdl3/gamecontrollerdb.txt` for pads SDL does not know by itself. **Button Prompts** (in the launcher and in the
Settings menu) is *Automatic* by default: the prompts follow the device you used last -- the game's own
PlayStation glyphs, Xbox, Nintendo or Steam Deck glyphs drawn in the same style, or the keyboard and mouse
keys -- and can be fixed to any one of them.

Keyboard and mouse defaults (every key can be changed in `controls.ini` next to `settings.ini`, one
`action=KEY` per line; run `Update inFamous.cmd` afterwards so the drawn key prompts follow):

| Action | Key | Action | Key |
|---|---|---|---|
| Move (run) / walk | W A S D / hold Shift | Camera | Mouse |
| Jump (Cross) | Space | Melee (Square) | F |
| Triangle | E | Circle | C |
| Aim (L1) | Right mouse | Fire (R1) | Left mouse |
| L2 / R2 | Q / R | L3 / R3 | V / Middle mouse |
| Precision (d-pad up) | Ctrl | Map (Select) | Tab |
| Pause (Start) | Esc | Free the mouse pointer | hold Alt |

Actions: `forward back left right jump melee triangle circle aim fire l2 r2 l3 r3 precision select start walk
free_mouse`. Keys: a letter or digit, `SPACE`, `ENTER`, `TAB`, `ESC`, `SHIFT`, `CTRL`, `ALT`, `BACKSPACE`,
`UP`/`DOWN`/`LEFT`/`RIGHT`, `F1`-`F12`, `LMB`/`RMB`/`MMB`/`MOUSE4`/`MOUSE5`, or `VK:<hex>`. In menus Enter
selects, Backspace goes back, and W/A/S/D or the arrow keys move the cursor.

**F10** opens the in-game trainer: God Mode, Infinite Energy, Refill Health,
Max Health x2/x5/x10, Add XP, Karma +/-, Kill Enemies / Civilians, Super Speed
and Player Model (play as Zeke, Trish, Kessler, Alden or John). Set `TRAINER=0`
to turn it off.

## DLSS and FSR (optional)

The game can use NVIDIA DLSS / DLAA or AMD FSR for upscaling, and AMD FSR frame generation.
They are not included in the download because NVIDIA and AMD distribute their SDKs themselves,
but both are free and take a few minutes to add.

- **DLSS** needs an NVIDIA RTX graphics card.
- **FSR** works on any modern GPU (NVIDIA, AMD or Intel). FSR 4 needs a Radeon RX 9000 card;
  other cards get FSR 3.1 automatically.

**1. Download the SDK(s) you want.** You need the **source code**, not a Windows or Linux demo.
On the page, click the green **Code** button, then **Download ZIP**. If you go through the
Releases page instead, pick **Source code (zip)**, not the files with "demo" or "samples" in the name.

| For | Download | Rename the extracted folder to |
|---|---|---|
| DLSS / DLAA | [github.com/NVIDIA/DLSS](https://github.com/NVIDIA/DLSS) | `dlss-sdk` |
| FSR and frame generation | [github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK](https://github.com/GPUOpen-LibrariesAndSDKs/FidelityFX-SDK) | `fsr-sdk` |

**2. Put the folder(s) in your inFamous install folder**, next to `infamous-recomp` and
`ps3recomp`:

```
inFamous\
  dlss-sdk\        <- contains include\ and lib\
  fsr-sdk\         <- contains Kits\
  infamous-recomp\
  ps3recomp\
  ...
```

Make sure you don't end up with a folder inside a folder (`dlss-sdk\DLSS-main\include`).
`include` (DLSS) or `Kits` (FSR) must be directly inside.

**3. Rebuild the game once.** Open a Command Prompt in the install folder and run:

```bat
type nul > .installer\update.pending
"Update inFamous.cmd"
```

This rebuilds the game with the SDK(s), which takes a while. Any later update also picks them up
automatically, so you only do this once.

**4. Turn it on in game:** pause, then **Settings**, then **Upscaling** (DLAA, DLSS Quality /
Balanced / Performance, FSR Native AA / Quality / Balanced / Performance) and **Frame
Generation**. The same options are in the launcher's options window. Upscaling and frame
generation apply after a restart.

If the **Upscaling** row is missing, the build didn't find the SDK: check the folder names and
layout from step 2, then repeat step 3.

## How it works

| Path | Contents |
|---|---|
| `ps3recomp` | The recompiler toolkit and runtime: PPU and SPU lifters, HLE system libraries, RSX on Direct3D 12, audio and input |
| `infamous-recomp` | The inFamous-specific parts: program entry, game stubs and fixes, asset tools |
| `installer` | Setup and the command-line installer: reads the disc, regenerates the code from your game, applies `installer/patches`, builds |
| `installer/infamous.toml` | Everything about the game and the port: pipeline steps, generated files, build and launch settings |
| `installer/generated.lock` | Hashes of each generated file before and after the port's patches |

## Credits

- [ps3recomp](https://github.com/sp00nznet/ps3recomp) by sp00nznet and
  contributors (see [ps3recomp/CONTRIBUTORS.md](ps3recomp/CONTRIBUTORS.md)).
- [RPCS3](https://rpcs3.net), used to decrypt the game executable.
- Sucker Punch Productions, for the game.
- [Lorenzo Aiello](https://github.com/aiellolorenzo23) and [ibrahimpuri](https://github.com/ibrahimpuri), for their support.
