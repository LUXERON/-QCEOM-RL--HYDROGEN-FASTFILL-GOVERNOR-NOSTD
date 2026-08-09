# [QCEOM RL] HYDROGEN-FASTFILL-GOVERNOR-NOSTD

**The dispenser side of the hydrogen fast-fill pipeline**: a fail-closed
`QCH2` fill-map image validator and station-controller executor for 700 bar
H₂ refuelling, verified on emulated Cortex-M55 (QEMU mps3-an547) and staged
for a physical STM32N657. Twin of
[-QCEOM-RL--HYDROGEN-FASTFILL-GOVERNOR](https://github.com/LUXERON/-QCEOM-RL--HYDROGEN-FASTFILL-GOVERNOR)
(the hosted solver).

## The deployment shape

The fill map is solved OFF-station (exact DP, < 4 s per tank model, hosted).
The station controller consumes a **2436-byte** provenance image and serves
commands. This crate is therefore pure `core`, zero heap, and **zero
floating point on the device path** — validation is integer hashing, band
indexing is integer division on permille SoC and whole °C, and the map is
u8 action codes. Cross-target bit-identity is *structural*, not something to
re-verify per libm.

The entire runtime of the product:

```rust
let cmd = map.command(soc_permille, t_gas_c, t_liner_c)?;
// -> FillCommand { mass_flow_g_s, precool_c }
```

Three integer band indices, one array lookup, one action decode. The
controller does not estimate, identify, or adapt — it indexes a static table
at SoC-band entry and holds the command for the crossing. **Adaptation is a
new image.** That is the patent posture (see the hosted repo's
`PATENT-LANDSCAPE.md`) enforced by construction.

Acceptance chain before a single gram is dispensed (fail-closed, ordered):
magic → format version → CRC32 → map fingerprint → **provisioned tank-hash
comparison**. The tank hash covers geometry, conductances, wall heat
capacity, ambient, *and the full rulebook* — the ceilings, both tier
ladders, and the declared objective weights. A map solved for a different
tank, or under a revised rulebook, is refused mechanically
(`ImageError::StaleProvenance`).

## Measured verification ladder

| Rung | Result |
|---|---|
| Host (x86-64) | **5/5** — golden-image parity with the hosted crate, integer band indexing, four refusal paths |
| QEMU mps3-an547 (Cortex-M55) | **4/4** — burn/accept, three refusals, full 2400-state lookup-surface fingerprint parity, and a 15-decision fill walked through the map |
| Physical STM32N657 | **not run** — binary built and staged; see [`docs/N657-RUN.md`](docs/N657-RUN.md) |

The map fingerprint `0xa0954ab04324380d` is **identical on x86-64 and on
Cortex-M55**. The golden vector (`qemu-m55-harness/src/golden.rs`) is emitted
by the hosted crate (`cargo run --release --bin emit_test_vector`, nominal
350 L tank at 25 °C) — the same 2436 bytes validate identically on both
targets, and a map for a different tank is refused on both.

Honest rung notes:

- **QEMU reports 0 DWT cycles.** The mps3-an547 model does not tick the
  cycle counter; the per-test cycle numbers are meaningful only on physical
  silicon, which is why the N657 mailbox records them. No timing claim is
  made from QEMU.
- **No physical-silicon claim.** The board is a shared resource and access is
  coordinated by the lead dev. `mailbox_burn.bin` (13,380 B, reset vector
  `0x3410_07C1`) is built and staged with the exact command sequence ready
  to execute; until it runs, this harness claims **two** targets, not three.

## N657 recipe (proven, inherited from the thermal / charge / hearing programs)

Raw `.bin` (not ELF) to `0x3410_0000`; `THERMAL_N657=1` selects the board
memory map; start via CubeProgrammer `-halt` →
`-coreReg xPSR=0x01000000 MSP=0x34200000 PC=0x341007c1` → `-run`; mailbox at
`0x3417_8000` (magic `QH2F` — **check it, old data persists across loads**);
the golden image is burned to `0x3417_9000` and validated from there. Full
sequence and mailbox decoding table in [`docs/N657-RUN.md`](docs/N657-RUN.md).

## Reproduce

```bash
cargo test --release                       # host parity, 5 tests

cd qemu-m55-harness
CARGO_NET_GIT_FETCH_WITH_CLI=true \
  cargo build --release --target thumbv8m.main-none-eabihf
python run_qemu_test.py \
  target/thumbv8m.main-none-eabihf/release/h2-fill-gov-m55-harness
```

Requires `qemu-system-arm >= 8.2` (native, or inside WSL on Windows — the
runner shells into WSL automatically and translates the ELF path).

## Layout

```
src/lib.rs                      integer band grid + the FillMap executor
src/image.rs                    QCH2 fail-closed validator (core, no float)
qemu-m55-harness/src/main.rs    the M55 executor test harness (semihosting)
qemu-m55-harness/src/bin/       the N657 mailbox variant (no semihosting)
qemu-m55-harness/src/golden.rs  the hosted-emitted golden vector
qemu-m55-harness/memory-*.x     QEMU vs physical-board memory maps
docs/N657-RUN.md                the staged physical-run sequence
```


## Physical silicon — triple-target closed

Measured on an STM32N6570-DK on 2026-08-09: **4 passed, 0 failed**
(mailbox `QH2F`, status 2). The map fingerprint
`0xa0954ab04324380d` is identical on x86-64, QEMU mps3-an547 and
physical STM32N657. Total ≈ 4.29 M cycles ≈ **67 ms @ 64 MHz**,
including every fail-closed refusal. Full mailbox decode:
[docs/N657-RUN.md](docs/N657-RUN.md) in the NOSTD repo.
