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
- Windows Setup includes its own Python interpreter and build packages.
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

Download the `inFamous-Setup` artifact from a successful **Setup app** workflow run and open
`inFamous Setup.exe`. Select your own US or European ISO or an extracted folder containing
`PS3_GAME`, then click **Install**. Setup installs missing build tools and builds the game on
your PC. When the build finishes, click **Play**. [Setup details](installer/README.md#the-setup-program-what-players-download)

## Updating

The launcher checks for updates in the background when it opens. Use **Update** to check
manually, or select **Update available** to open **What's New** before choosing **Install Update**.
The launcher shows the download and build progress, then restarts when the update finishes.

You can also run `Update inFamous (US).cmd` in the root folder. Both routes pull changed
files from `main`, including the installer and launcher published by the Setup app workflow,
and recompile only what changed. Release downloads are available on
[GitHub Releases](https://github.com/WoofahRayetCode/inFAMOUS-US/releases).

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
**Play** starts the game. Press **F2** in the game window to open or close the
same settings during play. With a controller, pause and choose **Settings**
or press **View/Back** when the shortcut appears. Display mode, VSync, frame
cap, button prompts, mouse speed, and Window Size apply during play. The overlay
marks changes that need a game restart.

Frame Rate Limit in the launcher and in-game settings supports Off, 30, 40, 45,
50, 60, 120, 144 and 240 FPS. These are caps; achievable FPS depends on the scene
and hardware. The existing default remains 60 FPS.

For laptop windowed play, choose **Windowed** and set **Window Size** separately
from **Game Resolution** in the launcher or in-game Settings:

| Use | Window Size | Game Resolution |
|---|---|---|
| Smaller window with room for other apps | 720p (1280 x 720) | 720p |
| Larger window with lighter rendering | 1080p (1920 x 1080) | 720p |

Windows too large for the desktop are fitted while keeping their aspect ratio.
Game Resolution changes take effect the next time the game starts. **Match Game
Resolution** sizes the window for the current game resolution. Fullscreen uses
the monitor. A Window Size change made in fullscreen applies when returning to
windowed mode.

### Settings and save manager

The launcher opens on **Play** with the detected region, installation status, and quick display
settings. **Display & Controls** has the full settings list. **Support** opens the current
and previous game logs and copies build details.

### Achievements

The game shows a brief notification when it awards a PS3 trophy. Open **Achievements**
in the launcher to see earned and locked trophies and your completion count. Hidden
trophy names and descriptions appear after you earn them. The installer reads the
trophy list from your disc dump, and local progress lives in `infamous-recomp/gamedata/trophies/`.
Run Setup or the updater to rebuild an existing installation with trophy notifications.

### Saves and backups

**Saves** shows the active save
folder, US/European save slots and last-modified times in UTC. Use **Open Save
Folder**, **Back Up Saves**, **Back Up Settings**, or **Back Up Everything**.
Settings backups include `settings.ini` and `controls.ini` when present.

Choose a snapshot and **Saves only**, **Settings only**, or **Saves and settings**
before restoring. The game must be closed for backups/restores. Restore verifies
the snapshot and creates a **before-restore** recovery backup first; unrelated
saves are kept. Pending settings changes must be saved before backing them up.
Resetting settings does not reset game progress. In the game's pause menu, **Save Game**
opens ten manual slots and **Load Game** lists existing saves, including the separate
autosave. Slot 1 is your existing manual save; slots 2-10 add independent saves.
Occupied slots show checkpoint details and a UTC timestamp. Saving over one requires
a second **Overwrite** confirmation. Cancel leaves the slot untouched. Keyboard,
mouse and controller (Up/Down, A/Cross, B/Circle) work in the selector.

The manager footer credits **Whompay / inFAMOUSreborn** for the original European ISO
port (`BCES00220`), **WoofahRayetCode / inFAMOUS-US** for this US ISO fork (`BCUS98119`),
and **sp00nznet / ps3recomp** for the recompiler foundation, with links to each repo.

Snapshots are local folders under `infamous-recomp/user-backups`, with per-file
SHA-256 verification. Keep the entire snapshot folder together when copying it.
Replaced originals are also retained in `.restore-*` transaction folders for
recovery; snapshots and transaction folders are excluded from Git and exports.
After restoring different keyboard bindings, follow the existing keyboard-prompt
refresh instructions below.

### Controls and button prompts

Xbox pads (and Steam Input's virtual pads) work through XInput; DualShock 4, DualSense, Switch Pro, Joy-Con,
Steam Controller and most other pads work through SDL3, with the community mappings in
`sdl3/gamecontrollerdb.txt` for pads SDL does not know by itself. **Button Prompts** (in the launcher and in the
Settings menu) is *Automatic* by default: the prompts follow the device you used last -- the game's own
PlayStation glyphs, Xbox, Nintendo or Steam Deck glyphs drawn in the same style, or the keyboard and mouse
keys -- and can be fixed to any one of them.

Host settings and dialog overlays accept the same controller buttons and left stick as gameplay.
Connected controllers keep their player assignments when another controller is plugged in. If player one
disconnects, an already connected controller takes over; reconnecting a pad fills an available slot.
Assignments last for the running session. Xbox's physical XInput slot does not have to be slot zero.

SDL controllers with enabled accelerometers/gyroscopes forward motion to the PS3 sensor report, including
the left/right sensor variants used by paired Joy-Con. Controllers without sensors (including standard
XInput devices) retain neutral sensor readings and do not advertise motion capability. Motion-dependent
gameplay still needs testing with a physical controller. Adaptive triggers and advanced DualSense haptics
are not implemented. The bundled SDL 3.4.18 Switch 2 driver does not support Bluetooth; a mapping entry
alone does not establish support for a connection mode.

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
and Player Model (play as Zeke, Trish, Kessler, Alden or John). For repeatable
test routes, **Testing → Mark Location** records a numbered location and
timestamp in the game log. On the US release, enable **Testing → Flight** and
hold L2 and Jump to ascend. Release either input to fall normally. Flight starts
off each time you launch the game. Set `TRAINER=0` to turn the trainer off.

## DLSS and FSR (optional)

Windows Setup includes AMD FSR and downloads the DLSS files from NVIDIA during the first build.
The installer verifies both sets of files and puts the runtime DLLs beside the game executable.
An update also prepares the upscalers for an existing installation. If a vendor download fails,
the rest of the game can still build. Run Setup or Update again to retry the download.

Choose **Upscaling** and **Frame Generation** in the launcher or under **Settings** in the pause
menu. DLSS and DLAA require a compatible NVIDIA RTX GPU and driver. The AMD runtime selects the
available FSR version for your GPU. FSR frame generation can run with FSR or DLSS on supported
hardware. Changes apply after a restart.

Set **Game Resolution** to 1080p or higher to render below the selected output resolution. With
720p output, the 720p internal floor makes the upscaler run in native anti-aliasing mode.

## How it works

| Path | Contents |
|---|---|
| `ps3recomp` | The recompiler toolkit and runtime: PPU and SPU lifters, HLE system libraries, RSX on Direct3D 12, audio and input |
| `infamous-recomp` | The inFamous-specific parts: program entry, game stubs and fixes, asset tools |
| `installer` | Setup and the command-line installer: reads the disc, regenerates the code from your game, applies `installer/patches`, builds |
| `installer/infamous.toml` | Everything about the game and the port: pipeline steps, generated files, build and launch settings |
| `installer/generated.lock` | Hashes of each generated file before and after the port's patches |

## Credits

- Original European ISO port (`BCES00220`): [Whompay / inFAMOUSreborn](https://github.com/whompay/inFAMOUSreborn).
- US ISO adaptation (`BCUS98119`): [WoofahRayetCode / inFAMOUS-US](https://github.com/WoofahRayetCode/inFAMOUS-US).
- [ps3recomp](https://github.com/sp00nznet/ps3recomp) by sp00nznet and
  contributors (see [ps3recomp/CONTRIBUTORS.md](ps3recomp/CONTRIBUTORS.md)).
- [RPCS3](https://rpcs3.net), used to decrypt the game executable.
- Sucker Punch Productions, for the game.
- [Lorenzo Aiello](https://github.com/aiellolorenzo23) and [ibrahimpuri](https://github.com/ibrahimpuri), for their support.
