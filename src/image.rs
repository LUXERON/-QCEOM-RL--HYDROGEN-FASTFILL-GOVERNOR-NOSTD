//! `QCH2` image validation — byte-compatible with the hosted `image.rs`,
//! rebuilt on pure `core`.
//!
//! Layout (2436 bytes, LE): magic `"QCH2"` u32 · format version u32 ·
//! dispenser serial u64 · tank+rulebook hash u64 · map fingerprint u64 ·
//! 2400-byte map · CRC32 over bytes 0..2432.

use crate::FillMap;

pub const MAGIC: u32 = 0x3248_4351; // "QCH2"
pub const VERSION: u32 = 1;
pub const TABLE_LEN: usize = 2400;
pub const HEADER_LEN: usize = 32;
pub const IMAGE_LEN: usize = HEADER_LEN + TABLE_LEN + 4; // 2436

pub fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    let mut i = 0;
    while i < data.len() {
        crc ^= data[i] as u32;
        let mut k = 0;
        while k < 8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            k += 1;
        }
        i += 1;
    }
    !crc
}

pub fn fingerprint(table: &[u8; TABLE_LEN]) -> u64 {
    let mut h: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut i = 0;
    while i < TABLE_LEN {
        h = (h.rotate_left(7) ^ table[i] as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        i += 1;
    }
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    BadMagic,
    BadVersion,
    BadCrc,
    FingerprintMismatch,
    /// The map's provenance hash is not what this dispenser was provisioned
    /// to expect — wrong tank model, or a stale rulebook.
    StaleProvenance,
}

#[derive(Debug)]
pub struct ValidImage {
    pub serial: u64,
    pub tank_hash: u64,
    pub map: FillMap,
}

fn u32_at(img: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([img[i], img[i + 1], img[i + 2], img[i + 3]])
}

fn u64_at(img: &[u8], i: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&img[i..i + 8]);
    u64::from_le_bytes(b)
}

/// Structural validation: magic → version → CRC → fingerprint.
pub fn validate(img: &[u8]) -> Result<ValidImage, ImageError> {
    if img.len() < IMAGE_LEN || u32_at(img, 0) != MAGIC {
        return Err(ImageError::BadMagic);
    }
    if u32_at(img, 4) != VERSION {
        return Err(ImageError::BadVersion);
    }
    if crc32(&img[..IMAGE_LEN - 4]) != u32_at(img, IMAGE_LEN - 4) {
        return Err(ImageError::BadCrc);
    }
    let mut actions = [0u8; TABLE_LEN];
    actions.copy_from_slice(&img[HEADER_LEN..HEADER_LEN + TABLE_LEN]);
    if fingerprint(&actions) != u64_at(img, 24) {
        return Err(ImageError::FingerprintMismatch);
    }
    Ok(ValidImage {
        serial: u64_at(img, 8),
        tank_hash: u64_at(img, 16),
        map: FillMap { actions },
    })
}

/// Full dispenser-side acceptance: structural validation PLUS the
/// provisioned provenance expectation. This is the call a station controller
/// makes before commanding a single gram of hydrogen.
pub fn accept(img: &[u8], expected_tank_hash: u64) -> Result<ValidImage, ImageError> {
    let v = validate(img)?;
    if v.tank_hash != expected_tank_hash {
        return Err(ImageError::StaleProvenance);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ACTIONS, GAS_BANDS, LIN_BANDS, SOC_BANDS};

    // The golden vector emitted by the hosted crate (nominal 350 L @ 25 °C).
    include!("../qemu-m55-harness/src/golden.rs");

    #[test]
    fn crc32_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn layout_constants_agree_with_the_hosted_crate() {
        assert_eq!(TABLE_LEN, SOC_BANDS * GAS_BANDS * LIN_BANDS);
        assert_eq!(IMAGE_LEN, 2436);
        assert_eq!(&MAGIC.to_le_bytes(), b"QCH2");
    }

    #[test]
    fn golden_image_parity_with_hosted() {
        let v = accept(&GOLDEN_IMAGE, GOLDEN_TANK_HASH).expect("golden accepts");
        assert_eq!(v.serial, GOLDEN_SERIAL);
        assert_eq!(v.map.fingerprint(), GOLDEN_MAP_FP);
        // Every action byte decodes to a legal command.
        for a in v.map.actions.iter() {
            assert!((*a as usize) < ACTIONS, "illegal action byte {a}");
        }
        // A representative command: mid-fill, warm gas, cool wall.
        let c = v.map.command(400, 55, 30).expect("command");
        assert!(crate::FLOW_TIER_G_S.contains(&c.mass_flow_g_s));
        assert!(crate::PRECOOL_C.contains(&c.precool_c));
    }

    #[test]
    fn corruption_and_staleness_are_refused() {
        let mut bad = GOLDEN_IMAGE;
        bad[HEADER_LEN + 137] ^= 1;
        assert_eq!(validate(&bad).unwrap_err(), ImageError::BadCrc);
        let crc = crc32(&bad[..IMAGE_LEN - 4]);
        bad[IMAGE_LEN - 4..].copy_from_slice(&crc.to_le_bytes());
        assert_eq!(validate(&bad).unwrap_err(), ImageError::FingerprintMismatch);
        // A map solved for a DIFFERENT tank model is refused by a dispenser
        // provisioned for this one.
        assert_eq!(
            accept(&GOLDEN_IMAGE, GOLDEN_TANK_HASH_OTHER).unwrap_err(),
            ImageError::StaleProvenance
        );
        let mut m = GOLDEN_IMAGE;
        m[0] ^= 0xFF;
        assert_eq!(validate(&m).unwrap_err(), ImageError::BadMagic);
        let mut vsn = GOLDEN_IMAGE;
        vsn[4] = 9;
        assert_eq!(validate(&vsn).unwrap_err(), ImageError::BadVersion);
        assert_eq!(validate(&GOLDEN_IMAGE[..IMAGE_LEN - 1]).unwrap_err(), ImageError::BadMagic);
    }
}
