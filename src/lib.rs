//! [QCEOM RL] Hydrogen fast-fill governor — NOSTD twin.
//!
//! The dispenser side of the fuelling pipeline. The fill map is solved
//! OFF-station (hosted repo `LUXERON/-QCEOM-RL--HYDROGEN-FASTFILL-GOVERNOR`);
//! the station controller consumes a 2436-byte `QCH2` provenance image and
//! serves commands. This crate is therefore pure `core`, zero heap, and —
//! deliberately — **zero floating point on the device path**: validation is
//! integer hashing, band indexing is integer division on °C and permille
//! SoC, and the map is u8 action codes. Cross-target bit-identity is
//! therefore *structural* rather than something to re-verify per libm.
//!
//! Fail-closed contract (byte-compatible with the hosted `image.rs`):
//! magic → format version → CRC32 → map fingerprint, then the provisioned
//! tank-hash comparison (tank geometry + conductances + FULL rulebook: the
//! ceilings, the tier ladders, and the declared objective weights). A map
//! solved for a different tank, or under a revised rulebook, is refused
//! before a single kilogram is dispensed.
//!
//! **Patent posture, enforced by construction.** The device does not
//! estimate, identify, or adapt. It indexes a static table at SoC-band entry
//! and holds the command for the crossing. Adaptation is a new image.

#![cfg_attr(not(test), no_std)]

pub mod image;

// --- The band grid, mirrored from the hosted `fill_env` (integer form) ---

pub const SOC_BANDS: usize = 16;
pub const GAS_BANDS: usize = 15;
pub const LIN_BANDS: usize = 10;

/// Gas-temperature bands: 5 K each from 10 °C, top edge 85 °C (the ceiling).
pub const GAS_BASE_C: i32 = 10;
pub const GAS_BAND_C: i32 = 5;
/// Liner bands: 6 K each from 15 °C, top edge 75 °C (the ceiling).
pub const LIN_BASE_C: i32 = 15;
pub const LIN_BAND_C: i32 = 6;

/// Commanded mass-flow tiers, **g/s** (integer — the hosted kg/s values
/// 0.015…0.120 scaled by 1000, exact in integers).
pub const FLOW_TIER_G_S: [u16; 6] = [15, 30, 50, 70, 95, 120];
/// Pre-cool setpoints, °C (SAE station categories T40/T30/T20/T10).
pub const PRECOOL_C: [i8; 4] = [-40, -30, -20, -10];
pub const ACTIONS: usize = 24;

/// The command a dispenser executes for one SoC-band crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillCommand {
    pub mass_flow_g_s: u16,
    pub precool_c: i8,
}

fn clamp_band(raw: i32, n: usize) -> usize {
    if raw < 0 {
        0
    } else if raw as usize >= n {
        n - 1
    } else {
        raw as usize
    }
}

/// SoC band from state of charge in **permille** (0…1000).
#[inline]
pub fn soc_band_permille(soc_permille: u16) -> usize {
    clamp_band(soc_permille as i32 * SOC_BANDS as i32 / 1000, SOC_BANDS)
}

/// Gas-temperature band from whole °C. Integer floor division that also
/// floors correctly below the base (Rust `/` truncates toward zero).
#[inline]
pub fn gas_band_c(t_c: i32) -> usize {
    clamp_band(div_floor(t_c - GAS_BASE_C, GAS_BAND_C), GAS_BANDS)
}

/// Liner-temperature band from whole °C.
#[inline]
pub fn liner_band_c(t_c: i32) -> usize {
    clamp_band(div_floor(t_c - LIN_BASE_C, LIN_BAND_C), LIN_BANDS)
}

fn div_floor(a: i32, b: i32) -> i32 {
    let q = a / b;
    if a % b != 0 && (a < 0) != (b < 0) {
        q - 1
    } else {
        q
    }
}

#[inline]
pub fn state_id(sb: usize, gb: usize, lb: usize) -> usize {
    (sb * GAS_BANDS + gb) * LIN_BANDS + lb
}

/// The executable fill map: one action byte per state.
#[derive(Debug, Clone, Copy)]
pub struct FillMap {
    pub actions: [u8; image::TABLE_LEN],
}

impl FillMap {
    /// The whole runtime of the product: three integer band indices, one
    /// array lookup, one action decode. No floats, no allocation, no state.
    ///
    /// Returns `None` if the action byte is out of range — a corrupt map
    /// that somehow passed validation still cannot produce a command.
    #[inline]
    pub fn command(&self, soc_permille: u16, t_gas_c: i32, t_liner_c: i32) -> Option<FillCommand> {
        let s = state_id(
            soc_band_permille(soc_permille),
            gas_band_c(t_gas_c),
            liner_band_c(t_liner_c),
        );
        let a = self.actions[s] as usize;
        if a >= ACTIONS {
            return None;
        }
        Some(FillCommand {
            mass_flow_g_s: FLOW_TIER_G_S[a / PRECOOL_C.len()],
            precool_c: PRECOOL_C[a % PRECOOL_C.len()],
        })
    }

    pub fn fingerprint(&self) -> u64 {
        image::fingerprint(&self.actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_indexing_is_integer_and_clamped() {
        assert_eq!(soc_band_permille(0), 0);
        assert_eq!(soc_band_permille(50), 0);
        assert_eq!(soc_band_permille(1000), SOC_BANDS - 1);
        assert_eq!(soc_band_permille(500), 8);
        assert_eq!(gas_band_c(10), 0);
        assert_eq!(gas_band_c(14), 0);
        assert_eq!(gas_band_c(15), 1);
        assert_eq!(gas_band_c(84), 14);
        assert_eq!(gas_band_c(200), GAS_BANDS - 1);
        assert_eq!(gas_band_c(-40), 0); // below base clamps, never wraps
        assert_eq!(liner_band_c(15), 0);
        assert_eq!(liner_band_c(20), 0);
        assert_eq!(liner_band_c(21), 1);
        assert_eq!(liner_band_c(74), 9);
        assert_eq!(liner_band_c(-5), 0);
        assert_eq!(state_id(SOC_BANDS - 1, GAS_BANDS - 1, LIN_BANDS - 1), image::TABLE_LEN - 1);
    }
}
