//! The fill-map executor on emulated Cortex-M55 (QEMU mps3-an547): the
//! deployment shape end to end — a hosted-solved golden image is embedded,
//! burned to a stand-in region, validated fail-closed from there, the
//! corruption and stale-provenance refusals are exercised, a whole
//! 15-decision fill is walked through the map, and every lookup is
//! fingerprint-checked against the hosted crate bit for bit.

#![no_std]
#![no_main]

use core::ptr::addr_of_mut;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use panic_semihosting as _;

use h2_fill_gov_nostd::image::{self, ImageError, IMAGE_LEN, TABLE_LEN};
use h2_fill_gov_nostd::{FLOW_TIER_G_S, PRECOOL_C};

mod golden;
use golden::{GOLDEN_IMAGE, GOLDEN_MAP_FP, GOLDEN_SERIAL, GOLDEN_TANK_HASH, GOLDEN_TANK_HASH_OTHER};

static mut BURN: [u8; IMAGE_LEN] = [0; IMAGE_LEN];

fn ok(cond: bool, err: &'static str) -> Result<(), &'static str> {
    if cond {
        Ok(())
    } else {
        Err(err)
    }
}

fn burn_and_accept() -> Result<(), &'static str> {
    let burn = unsafe { &mut *addr_of_mut!(BURN) };
    burn.copy_from_slice(&GOLDEN_IMAGE);
    let v = image::accept(burn, GOLDEN_TANK_HASH).map_err(|_| "golden refused")?;
    ok(v.serial == GOLDEN_SERIAL, "serial mismatch")?;
    ok(v.map.fingerprint() == GOLDEN_MAP_FP, "map fp mismatch")?;
    let _ = hprintln!(
        "    image {} B, map {} states, fp {:#018x}",
        IMAGE_LEN,
        TABLE_LEN,
        v.map.fingerprint()
    );
    Ok(())
}

fn refusals() -> Result<(), &'static str> {
    let burn = unsafe { &mut *addr_of_mut!(BURN) };
    // Corrupt the burned region: CRC refuses.
    burn[image::HEADER_LEN + 137] ^= 1;
    ok(
        image::validate(burn).err() == Some(ImageError::BadCrc),
        "corruption must be refused (CRC)",
    )?;
    // Forge the CRC: the map fingerprint refuses.
    let crc = image::crc32(&burn[..IMAGE_LEN - 4]);
    burn[IMAGE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
    ok(
        image::validate(burn).err() == Some(ImageError::FingerprintMismatch),
        "forged CRC must be refused (fingerprint)",
    )?;
    // Restore; a dispenser provisioned for a DIFFERENT tank refuses this map.
    burn.copy_from_slice(&GOLDEN_IMAGE);
    ok(
        image::accept(burn, GOLDEN_TANK_HASH_OTHER).err() == Some(ImageError::StaleProvenance),
        "stale provenance must be refused",
    )
}

/// Fingerprint the FULL lookup surface exactly as the hosted table
/// fingerprint does — bit parity or bust.
fn lookup_surface() -> Result<(), &'static str> {
    let burn = unsafe { &*addr_of_mut!(BURN) };
    let v = image::accept(burn, GOLDEN_TANK_HASH).map_err(|_| "accept")?;
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < TABLE_LEN {
        h = (h.rotate_left(7) ^ v.map.actions[i] as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        i += 1;
    }
    ok(h == GOLDEN_MAP_FP, "lookup-surface fp diverges from hosted")
}

/// Walk a whole fill: 16 SoC bands' worth of commands, exactly as the
/// dispenser would index them (integer permille SoC and whole °C), checking
/// every command decodes into the declared tier ladders.
fn walk_a_fill() -> Result<(), &'static str> {
    let burn = unsafe { &*addr_of_mut!(BURN) };
    let v = image::accept(burn, GOLDEN_TANK_HASH).map_err(|_| "accept")?;
    let mut gas_c: i32 = 25;
    let mut liner_c: i32 = 25;
    let mut commands = 0usize;
    let mut soc_permille: u16 = 50;
    while soc_permille < 940 {
        let c = v
            .map
            .command(soc_permille, gas_c, liner_c)
            .ok_or("map produced an illegal action byte")?;
        ok(FLOW_TIER_G_S.contains(&c.mass_flow_g_s), "flow tier off-ladder")?;
        ok(PRECOOL_C.contains(&c.precool_c), "pre-cool setpoint off-ladder")?;
        // Crude integer surrogate for the crossing's thermal effect — this
        // is a lookup-surface exercise, not a simulation; the point is that
        // every state the walk indexes yields a legal command.
        gas_c += 4;
        liner_c += 1;
        soc_permille += 60;
        commands += 1;
    }
    let _ = hprintln!("    walked {} band decisions, all in-ladder", commands);
    ok(commands >= 14, "fill walk too short")
}

struct Test {
    name: &'static str,
    run: fn() -> Result<(), &'static str>,
}

const TESTS: &[Test] = &[
    Test { name: "burn_and_accept", run: burn_and_accept },
    Test { name: "refusals_fail_closed", run: refusals },
    Test { name: "lookup_surface_parity", run: lookup_surface },
    Test { name: "walk_a_whole_fill", run: walk_a_fill },
];

#[entry]
fn main() -> ! {
    let _ = hprintln!("== H2 fast-fill map executor on Cortex-M55 (QEMU mps3-an547 / STM32N657) ==");
    let mut cp = cortex_m::Peripherals::take().unwrap();
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();
    let mut failed = 0usize;
    for t in TESTS {
        let c0 = cortex_m::peripheral::DWT::cycle_count();
        match (t.run)() {
            Ok(()) => {
                let cycles = cortex_m::peripheral::DWT::cycle_count().wrapping_sub(c0);
                let _ = hprintln!("  [PASS] {} ({} cycles)", t.name, cycles);
            }
            Err(e) => {
                failed += 1;
                let _ = hprintln!("  [FAIL] {} - {}", t.name, e);
            }
        }
    }
    let _ = hprintln!("[harness] {} passed, {} failed", TESTS.len() - failed, failed);
    if failed == 0 {
        debug::exit(debug::EXIT_SUCCESS);
    } else {
        debug::exit(debug::EXIT_FAILURE);
    }
    loop {}
}
