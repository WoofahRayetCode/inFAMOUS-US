/*
 * Guest addresses that differ between the European (BCES00220) and US (BCUS98119) executables.
 *
 * CMakeLists.txt defines INFAMOUS_REGION_US when the linked dump is the US release. The port sources
 * are written with European addresses, and everything here is the identity for a European build.
 *
 * The two builds are nearly the same program. PPU code is shifted by a few bytes per region, the TOC
 * and read-only pools by -0x48 to -0x50, and BSS globals not at all (0x7BC650, 0x907680, 0x914E00,
 * 0x915430, 0x917144... were each confirmed from the US code that uses them). SPU programs are identical.
 * Function addresses were derived from the 1,999 call sites in the port patches, and each one was
 * checked to land on a function start in the US lift.
 */
#pragma once
#include <stdint.h>

#ifdef INFAMOUS_REGION_US
#define INF_TITLE_TOC    0x0078BF78u
#define INF_CLOCK_SLOT   0x00784070u   /* [TOC-0x7F08] */
#define INF_CLOCK_POOL   0x00769554u
#define INF_CLOCK_WORD   0x0076155Cu   /* [pool-0x7FF8] = clock base */
#define INF_ANIM_KICK_LR 0x00028534u
#define INF_CLASS_COLE   0x007279E0u
#define INF_CLASS_PED    0x0072A990u
#define INF_CLASS_ENEMY  0x0072A728u
#define INF_CAR_SPAWN_MIN2 0x0076B51Cu
#define INF_CAR_SPAWN_VISR 0x0076B538u
#define INF_LOD_TAB      0x0078AD30u
#define INF_PM_ANCHOR_OPD 0x0077C128u
#define INF_RELEASE_NAME "BCUS98119"
#else
#define INF_TITLE_TOC    0x0078BFC0u
#define INF_CLOCK_SLOT   0x007840B8u
#define INF_CLOCK_POOL   0x0076959Cu
#define INF_CLOCK_WORD   0x007615A4u
#define INF_ANIM_KICK_LR 0x00028554u
#define INF_CLASS_COLE   0x00727A28u
#define INF_CLASS_PED    0x0072A9D8u
#define INF_CLASS_ENEMY  0x0072A770u
#define INF_CAR_SPAWN_MIN2 0x0076B564u
#define INF_CAR_SPAWN_VISR 0x0076B580u
#define INF_LOD_TAB      0x0078AD80u
#define INF_PM_ANCHOR_OPD 0x0077C170u
#define INF_RELEASE_NAME "BCES00220"
#endif

#ifdef __cplusplus
#ifdef INFAMOUS_REGION_US
/* European name -> the same function in the US lift. */
#define func_0004E658 func_0004E638
#define func_00051BE0 func_00051BC0
#define func_00200568 func_00200700
#define func_00203008 func_002031A0
#define func_00216548 func_002166D0
#define func_00218060 func_002181E8
#define func_00218F30 func_002190B8
#define func_00274630 func_002747B8
#define func_0027A020 func_0027A1A8
#define func_0027A2C0 func_0027A448
#define func_0027A720 func_0027A8A8
#define func_0027A840 func_0027A9C8
#define func_0027A950 func_0027AAD8
#define func_0027FC18 func_0027FDA0
#define func_002AF108 func_002AF0C8
#define func_003562C0 func_00356230
#define func_004BA678 func_004BA770
#endif

/* A European function address as a guest address of this build. */
constexpr uint32_t INF_RA(uint32_t eu)
{
#ifdef INFAMOUS_REGION_US
    switch (eu) {
    case 0x0004E658u: return 0x0004E638u;
    case 0x00051BE0u: return 0x00051BC0u;
    case 0x00200568u: return 0x00200700u;
    case 0x00203008u: return 0x002031A0u;
    case 0x00216548u: return 0x002166D0u;
    case 0x00218060u: return 0x002181E8u;
    case 0x00218F30u: return 0x002190B8u;
    case 0x00274630u: return 0x002747B8u;
    case 0x0027A020u: return 0x0027A1A8u;
    case 0x0027A2C0u: return 0x0027A448u;
    case 0x0027A720u: return 0x0027A8A8u;
    case 0x0027A840u: return 0x0027A9C8u;
    case 0x0027A950u: return 0x0027AAD8u;
    case 0x0027FC18u: return 0x0027FDA0u;
    case 0x002AF108u: return 0x002AF0C8u;
    case 0x003562C0u: return 0x00356230u;
    case 0x004BA678u: return 0x004BA770u;
    default: return eu;
    }
#else
    return eu;
#endif
}
#endif
