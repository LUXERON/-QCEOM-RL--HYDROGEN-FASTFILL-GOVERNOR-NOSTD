# STM32N657 Physical Run — H₂ Fast-Fill Map Executor

**Status: REBUILT AND STAGED (second image), NOT YET EXECUTED ON
SILICON.** The board is a shared resource and board access is
coordinated by the lead dev, so this harness stops at the built
artifact. Everything below is ready to run verbatim.

> **The measured-result section at the bottom of this file is
> SUPERSEDED.** A 2026-08-09 estate-wide audit found that the params
> hash guarded the physics but not the **codec**: the image is a bare
> action-index byte per state, so re-basing a band grid at constant band
> count would produce a same-length image with an unchanged hash that
> misindexes every lookup. This harness already hashed its action tiers
> — the best-covered case in the estate — but not the state band
> lattice. `GAS_BASE_C`, `GAS_BAND_C`, `LIN_BASE_C`, `LIN_BAND_C` and
> the band counts are now hashed too.
>
> | Field | Previous image | Current image |
> |---|---|---|
> | Tank hash (header offset 16) | `0xB4A7CF3CCB6D74A4` | **`0x0723DA1CCDC8BB94`** |
> | Stale-hash demo constant | `0xAF1C0A6A672DB2E8` | **`0x007E72F5B2948CE3`** |
> | Image CRC32 (mailbox word [6]) | `0x0CD9D0FD` | **`0xE3E6A21E`** |
> | Map fingerprint (words [4,5]) | `0xA0954AB04324380D` | **unchanged** |
>
> The solved map did not change — only its provenance binding, and
> therefore the image header and CRC. Host parity 5/5 and QEMU
> mps3-an547 4/4 have been re-run on the new image. **The lead developer
> must re-flash `mailbox_burn.bin` and re-run.**

## What is staged

| Item | Value |
|---|---|
| Binary | `qemu-m55-harness/mailbox_burn.bin` |
| Size | **13,380 bytes** (includes the 2,436-byte golden `QCH2` image) |
| Built with | `THERMAL_N657=1` → `memory-n657.x` (RAM-only board map) |
| Load address | `0x3410_0000` (second AXISRAM megabyte — the first does not accept bulk SWD writes) |
| Initial MSP | `0x3420_0000` |
| **Reset vector** | **`0x3410_07C1`** (thumb bit set; little-endian u32 at file offset 4) |
| Mailbox | `0x3417_8000`, magic `QH2F` = `0x4632_4851` |
| Burn region | `0x3417_9000` (the golden image is written there and validated FROM there) |

## Reproduce the build

```bash
cd qemu-m55-harness
THERMAL_N657=1 CARGO_NET_GIT_FETCH_WITH_CLI=true \
  cargo build --release --target thumbv8m.main-none-eabihf --bin mailbox
"$HOME/.rustup/toolchains/nightly-x86_64-pc-windows-msvc/lib/rustlib/x86_64-pc-windows-msvc/bin/llvm-objcopy.exe" \
  -O binary target/thumbv8m.main-none-eabihf/release/mailbox mailbox_burn.bin
```

## The run sequence (ready to execute — proven recipe, do not re-derive)

Raw `.bin`, **not** ELF. Start via `-halt` → `-coreReg` → `-run`.

```bash
STM32_Programmer_CLI -c port=SWD mode=HOTPLUG -q \
  -w mailbox_burn.bin 0x34100000 \
  -halt -coreReg xPSR=0x01000000 MSP=0x34200000 PC=0x341007c1 -run

# then read the mailbox — CHECK THE MAGIC FIRST, old data persists across loads
STM32_Programmer_CLI -c port=SWD mode=HOTPLUG -q -r32 0x34178000 56
```

## Mailbox decoding

| Word | Offset | Meaning | Expected |
|---|---|---|---|
| [0] | `0x34178000` | magic `QH2F` | `0x46324851` |
| [1] | `+0x04` | status | **2** = all passed (1 = running, 3 = failures) |
| [2] | `+0x08` | tests passed | **4** |
| [3] | `+0x0C` | tests failed | **0** |
| [4] | `+0x10` | map fingerprint, low word | `0x4324380D` |
| [5] | `+0x14` | map fingerprint, high word | `0xA0954AB0` |
| [6] | `+0x18` | image CRC32 recomputed on silicon | `0xE3E6A21E` |
| [7] | `+0x1C` | `burn_and_accept` DWT cycles | — |
| [8] | `+0x20` | `refusals` DWT cycles | — |
| [9] | `+0x24` | `lookup_surface` DWT cycles | — |
| [10] | `+0x28` | `walk_a_fill` DWT cycles | — |
| [11] | `+0x2C` | progress (index of the running test) | 4 when complete |
| [12] | `+0x30` | fine-grained marker inside test 1 | 6 when complete |
| [13] | `+0x34` | band decisions walked in test 4 | **15** |

## The acceptance claim this run would establish

The map fingerprint `0xa0954ab04324380d` is already identical on **x86-64**
(the hosted emitter, `cargo run --bin emit_test_vector`) and on **QEMU
mps3-an547 / Cortex-M55** (4/4 tests). A physical N657 run reporting the
same fingerprint at word [4,5] closes the triple-target bit-determinism
claim for this harness. Until then the README claims two targets, not three.

## Known bring-up notes (inherited, not re-derived)

- The first AXISRAM megabyte (`0x3400_0000`) rejects bulk SWD downloads under
  CubeProgrammer; everything therefore lives at `0x3410_0000+`.
- One first-load run in an earlier program wedged mid-accept and did not
  reproduce after a fresh load. The fine-grained marker at word [12] exists
  to bisect that cheaply if it recurs.
- **Always check the mailbox magic before trusting any other word** — AXISRAM
  retains the previous run's data across loads.

---

## MEASURED RESULT — physical STM32N6570-DK, 2026-08-09 — **SUPERSEDED**

> **Pertains to the PREVIOUS image** (tank hash `0xB4A7CF3CCB6D74A4`,
> image CRC32 `0x0CD9D0FD`). Real, kept in full, and no longer a
> validation of what is in `mailbox_burn.bin`. The map fingerprint is
> unchanged, so words [4,5] still read `0xA0954AB04324380D`; word [6]
> and the header hash are what moved. Append a fresh measured section
> below this one after re-running.

Run by the lead developer on the board (the build agent staged the
binary but did not touch the hardware).

```
0x34178000 : 46324851 00000002 00000004 00000000
0x34178010 : 4324380D A0954AB0 0CD9D0FD 00103F1E
0x34178020 : 0018C6DB 000D465E 000B31E4 00000004
0x34178030 : 00000006 0000000F
```

| Word | Meaning | Value |
|---|---|---|
| [0] | magic `QH2F` | 0x46324851 ✓ |
| [1] | status | **2 = all passed** |
| [2] | passed | **4** |
| [3] | failed | **0** |
| [4,5] | map fingerprint | **0xA0954AB04324380D** |
| [6] | image CRC32 | 0x0CD9D0FD |
| [7..10] | per-test DWT cycles | 1,064,222 · 1,623,771 · 869,470 · 733,668 |

**Triple-target bit-determinism closed.** The map fingerprint
`0xa0954ab04324380d` is identical on x86-64, QEMU mps3-an547 and
physical STM32N657 silicon. Total executor work ≈ 4.29 M cycles
≈ **67 ms @ 64 MHz** for all four tests including the fail-closed
refusals. Unlike QEMU (whose mps3-an547 model does not tick DWT),
these are real cycle counts.
