//! N657 mailbox variant: no semihosting — results + DWT cycles to AXISRAM
//! @ 0x3417_8000, loaded as a raw .bin via CubeProgrammer and started with
//! `-halt` → `-coreReg` → `-run` (the proven recipe). The golden image is
//! burned to 0x3417_9000 and validated FROM there; a valid image is left in
//! place for post-run inspection.
//!
//! Mailbox layout (u32 words at 0x3417_8000):
//!
//! | word | meaning |
//! |---|---|
//! | 0 | magic `"QH2F"` = 0x4632_4851 — **check it, old data persists across loads** |
//! | 1 | status: 1 running, 2 all passed, 3 failures |
//! | 2 | tests passed |
//! | 3 | tests failed |
//! | 4,5 | map fingerprint (lo, hi) |
//! | 6 | image CRC32 as recomputed on-silicon |
//! | 7..11 | DWT cycles for tests 1..4 |
//! | 11 | progress: index of the test currently running |
//! | 12 | fine-grained marker inside test 1 (kept from the bring-up bisect recipe) |
//! | 13 | band decisions walked in test 4 |

#![no_std]
#![no_main]

use core::ptr::write_volatile;
use cortex_m_rt::entry;
use panic_semihosting as _;

use h2_fill_gov_nostd::image::{self, ImageError, HEADER_LEN, IMAGE_LEN, TABLE_LEN};
use h2_fill_gov_nostd::{FLOW_TIER_G_S, PRECOOL_C};

#[path = "../golden.rs"]
mod golden;
use golden::{GOLDEN_IMAGE, GOLDEN_MAP_FP, GOLDEN_SERIAL, GOLDEN_TANK_HASH, GOLDEN_TANK_HASH_OTHER};

const MAILBOX: *mut u32 = 0x3417_8000 as *mut u32;
const BURN_REGION: *mut u8 = 0x3417_9000 as *mut u8;
const MAGIC_QH2F: u32 = 0x4632_4851; // "QH2F"

fn mb(idx: usize, val: u32) {
    unsafe { write_volatile(MAILBOX.add(idx), val) }
}

fn burned() -> &'static mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(BURN_REGION, IMAGE_LEN) }
}

fn burn_and_accept() -> bool {
    let b = burned();
    for (dst, &src) in b.iter_mut().zip(GOLDEN_IMAGE.iter()) {
        unsafe { write_volatile(dst as *mut u8, src) }
    }
    mb(12, 1); // burn done
    mb(6, image::crc32(&b[..IMAGE_LEN - 4]));
    mb(12, 2); // crc done
    let r = image::validate(b);
    mb(12, 3); // validate returned
    let v = match r {
        Ok(v) => v,
        Err(_) => return false,
    };
    mb(12, 4);
    if v.tank_hash != GOLDEN_TANK_HASH {
        return false;
    }
    mb(12, 5);
    let fp = v.map.fingerprint();
    mb(4, fp as u32);
    mb(5, (fp >> 32) as u32);
    mb(12, 6);
    v.serial == GOLDEN_SERIAL && fp == GOLDEN_MAP_FP
}

fn refusals() -> bool {
    let b = burned();
    b[HEADER_LEN + 137] ^= 1;
    let crc_reject = image::validate(b).err() == Some(ImageError::BadCrc);
    let crc = image::crc32(&b[..IMAGE_LEN - 4]);
    b[IMAGE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
    let fp_reject = image::validate(b).err() == Some(ImageError::FingerprintMismatch);
    b.copy_from_slice(&GOLDEN_IMAGE);
    let stale_reject =
        image::accept(b, GOLDEN_TANK_HASH_OTHER).err() == Some(ImageError::StaleProvenance);
    crc_reject && fp_reject && stale_reject
}

fn lookup_surface() -> bool {
    let b = burned();
    let v = match image::accept(b, GOLDEN_TANK_HASH) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < TABLE_LEN {
        h = (h.rotate_left(7) ^ v.map.actions[i] as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        i += 1;
    }
    h == GOLDEN_MAP_FP
}

fn walk_a_fill() -> bool {
    let b = burned();
    let v = match image::accept(b, GOLDEN_TANK_HASH) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let mut gas_c: i32 = 25;
    let mut liner_c: i32 = 25;
    let mut soc_permille: u16 = 50;
    let mut commands = 0u32;
    while soc_permille < 940 {
        let c = match v.map.command(soc_permille, gas_c, liner_c) {
            Some(c) => c,
            None => return false,
        };
        if !FLOW_TIER_G_S.contains(&c.mass_flow_g_s) || !PRECOOL_C.contains(&c.precool_c) {
            return false;
        }
        gas_c += 4;
        liner_c += 1;
        soc_permille += 60;
        commands += 1;
    }
    mb(13, commands);
    commands >= 14
}

#[entry]
fn main() -> ! {
    mb(0, MAGIC_QH2F);
    mb(1, 1);
    for i in 2..14 {
        mb(i, 0);
    }
    let mut cp = cortex_m::Peripherals::take().unwrap();
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    let mut passed = 0u32;
    let mut failed = 0u32;
    let tests: [(fn() -> bool, usize); 4] = [
        (burn_and_accept, 7),
        (refusals, 8),
        (lookup_surface, 9),
        (walk_a_fill, 10),
    ];
    for (i, (test, cyc_idx)) in tests.iter().enumerate() {
        mb(11, i as u32 + 1);
        let c0 = cortex_m::peripheral::DWT::cycle_count();
        let okk = test();
        let cycles = cortex_m::peripheral::DWT::cycle_count().wrapping_sub(c0);
        mb(*cyc_idx, cycles);
        if okk {
            passed += 1;
        } else {
            failed += 1;
        }
        mb(2, passed);
        mb(3, failed);
    }
    mb(1, if failed == 0 { 2 } else { 3 });
    loop {
        cortex_m::asm::nop();
    }
}
