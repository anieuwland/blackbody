//! The HTI/ToolTop metadata block ("t_IRInfo" in the camera firmware).
//!
//! See `docs/hti-tooltop-format.md` for the byte layout this module implements.

use uom::si::f32::ThermodynamicTemperature;
use uom::si::thermodynamic_temperature::degree_celsius;

use crate::thermal::into_temp;

/// Size of the metadata block as written by the current firmware (HT-04D 2.5.1).
pub const SIZE_CURRENT: usize = 112;

/// Size as written by older firmware (HT-19 2.1.19): [`SIZE_CURRENT`] without the margins field.
pub const SIZE_LEGACY: usize = 104;

const SIZE_FIELD: usize = 4;

/// Length of each of the three null-padded text fields.
const TEXT_LEN: usize = 20;

/// Visible-image pixels per thermal pixel: a format-wide invariant, as no field records it.
pub const VISUAL_SCALE: u32 = 2;

/// One of the camera's three measurement spots, positioned in visible-image pixel space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub x: u16,
    pub y: u16,
    /// Temperature in deci-degrees Celsius, as stored.
    pub temperature: i32,
}

impl Spot {
    /// The inverse of [`Spot::thermal_xy`], taking a temperature in deci-degrees Celsius.
    pub fn from_thermal_xy(x: u32, y: u32, temperature: i32) -> Spot {
        Spot {
            x: (x * VISUAL_SCALE).try_into().unwrap_or(u16::MAX),
            y: (y * VISUAL_SCALE).try_into().unwrap_or(u16::MAX),
            temperature,
        }
    }

    /// The spot's temperature as a typed value.
    pub fn temperature(&self) -> ThermodynamicTemperature {
        into_temp::<degree_celsius>(self.temperature as f32 / 10.0)
    }

    /// The spot's position in thermal-image pixels, clamped as the camera may sit on the far edge.
    pub fn thermal_xy(&self, thermal_width: usize, thermal_height: usize) -> (u32, u32) {
        let x = u32::from(self.x) / VISUAL_SCALE;
        let y = u32::from(self.y) / VISUAL_SCALE;
        (
            x.min(thermal_width.saturating_sub(1) as u32),
            y.min(thermal_height.saturating_sub(1) as u32),
        )
    }
}

/// The parsed metadata block.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// Camera model, e.g. `"HT-04D"`.
    pub model: String,
    /// Firmware version, e.g. `"2.5.1"`.
    pub firmware: String,
    /// Capture time as stored: `YYYY/MM/DD-HH:MM:SS`.
    pub date_time: String,
    pub center: Spot,
    pub max: Spot,
    pub min: Spot,
    /// Emissivity as a fraction, e.g. `0.95`.
    pub emissivity: f32,
    /// The vendor palette's raw enum value; libblackbody does not replicate the palettes.
    pub palette: u32,
    /// The camera's display unit; stored temperatures are always deci-degrees Celsius.
    pub unit: u32,
    /// Blend between thermal and visible light in the mixed image, 0–100.
    pub mix: u32,
    /// Image margins `[top, right, bottom, left]`; absent in the 104-byte layout.
    pub margins: Option<[u16; 4]>,
}

impl Metadata {
    /// Parse a block including its leading size field; `None` on an unknown size or truncation.
    pub fn parse(bytes: &[u8]) -> Option<Metadata> {
        let (size_bs, body) = bytes.split_first_chunk::<SIZE_FIELD>()?;
        let size = u32::from_le_bytes(*size_bs) as usize;
        if size != SIZE_CURRENT && size != SIZE_LEGACY {
            return None;
        }
        // The declared size covers the size field itself.
        let body = body.get(..size - SIZE_FIELD)?;

        let text = |offset: usize| -> Option<String> {
            let field = body.get(offset..offset + TEXT_LEN)?;
            let field = field.split(|b| *b == 0).next().unwrap_or(field);
            Some(String::from_utf8_lossy(field).trim().to_string())
        };
        let u32_at = |offset: usize| -> Option<u32> {
            let field = body.get(offset..offset + 4)?;
            Some(u32::from_le_bytes(field.try_into().ok()?))
        };
        let spot_at = |offset: usize| -> Option<Spot> {
            let field = body.get(offset..offset + 8)?;
            Some(Spot {
                x: u16::from_le_bytes(field[0..2].try_into().ok()?),
                y: u16::from_le_bytes(field[2..4].try_into().ok()?),
                temperature: i32::from_le_bytes(field[4..8].try_into().ok()?),
            })
        };

        let margins = match size {
            SIZE_CURRENT => {
                let field = body.get(100..108)?;
                let mut margins = [0u16; 4];
                for (margin, bs) in margins.iter_mut().zip(field.chunks_exact(2)) {
                    *margin = u16::from_le_bytes(bs.try_into().ok()?);
                }
                Some(margins)
            }
            _ => None,
        };

        // Note the field order: centre, max, then min.
        Some(Metadata {
            model: text(0)?,
            firmware: text(20)?,
            date_time: text(40)?,
            center: spot_at(60)?,
            max: spot_at(68)?,
            min: spot_at(76)?,
            emissivity: u32_at(84)? as f32 / 100.0,
            palette: u32_at(88)?,
            unit: u32_at(92)?,
            mix: u32_at(96)?,
            margins,
        })
    }

    /// Serialize to the 112-byte layout, including the leading `u32` size field.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(SIZE_CURRENT);
        bytes.extend_from_slice(&(SIZE_CURRENT as u32).to_le_bytes());

        let mut text = |s: &str| {
            let mut field = [0u8; TEXT_LEN];
            // Truncate on a char boundary so a long value cannot split a UTF-8 sequence.
            let s = match s.char_indices().find(|(i, c)| i + c.len_utf8() > TEXT_LEN - 1) {
                Some((i, _)) => &s[..i],
                None => s,
            };
            field[..s.len()].copy_from_slice(s.as_bytes());
            bytes.extend_from_slice(&field);
        };
        text(&self.model);
        text(&self.firmware);
        text(&self.date_time);

        for spot in [&self.center, &self.max, &self.min] {
            bytes.extend_from_slice(&spot.x.to_le_bytes());
            bytes.extend_from_slice(&spot.y.to_le_bytes());
            bytes.extend_from_slice(&spot.temperature.to_le_bytes());
        }

        let emissivity = (self.emissivity * 100.0).round().clamp(0.0, u32::MAX as f32) as u32;
        for value in [emissivity, self.palette, self.unit, self.mix] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        for margin in self.margins.unwrap_or_default() {
            bytes.extend_from_slice(&margin.to_le_bytes());
        }

        debug_assert_eq!(bytes.len(), SIZE_CURRENT);
        bytes
    }

    /// The capture time as the EXIF-style `YYYY:MM:DD HH:MM:SS` other formats' metadata uses.
    pub fn exif_date_time(&self) -> Option<String> {
        let (date, time) = self.date_time.split_once('-')?;
        if date.len() != 10 || time.len() != 8 {
            return None;
        }
        Some(format!("{} {}", date.replace('/', ":"), time))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The metadata block of `hti_ht-04d_1.jpg`, verified against the camera's own readout.
    fn sample() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(SIZE_CURRENT as u32).to_le_bytes());
        let mut text = |s: &str| {
            let mut field = [0u8; TEXT_LEN];
            field[..s.len()].copy_from_slice(s.as_bytes());
            bytes.extend_from_slice(&field);
        };
        text("HT-04D");
        text("2.5.1");
        text("2024/11/21-01:06:39");
        for (x, y, t) in [(120u16, 160u16, 254i32), (118, 198, 261), (234, 236, 138)] {
            bytes.extend_from_slice(&x.to_le_bytes());
            bytes.extend_from_slice(&y.to_le_bytes());
            bytes.extend_from_slice(&t.to_le_bytes());
        }
        for value in [95u32, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0u8; 8]);
        bytes
    }

    #[test]
    fn parses_every_field() {
        let m = Metadata::parse(&sample()).expect("parses");
        assert_eq!(m.model, "HT-04D");
        assert_eq!(m.firmware, "2.5.1");
        assert_eq!(m.date_time, "2024/11/21-01:06:39");
        assert_eq!(m.center, Spot { x: 120, y: 160, temperature: 254 });
        assert_eq!(m.max, Spot { x: 118, y: 198, temperature: 261 });
        assert_eq!(m.min, Spot { x: 234, y: 236, temperature: 138 });
        assert_eq!(m.emissivity, 0.95);
        assert_eq!(m.palette, 0);
        assert_eq!(m.unit, 0);
        assert_eq!(m.mix, 0);
        assert_eq!(m.margins, Some([0, 0, 0, 0]));
    }

    #[test]
    fn parses_the_legacy_layout_without_margins() {
        let mut bytes = sample();
        bytes.truncate(SIZE_LEGACY);
        bytes[..4].copy_from_slice(&(SIZE_LEGACY as u32).to_le_bytes());

        let m = Metadata::parse(&bytes).expect("parses");
        assert_eq!(m.model, "HT-04D");
        assert_eq!(m.mix, 0);
        assert_eq!(m.margins, None);
    }

    #[test]
    fn rejects_unknown_sizes_and_truncation() {
        let mut wrong_size = sample();
        wrong_size[..4].copy_from_slice(&64u32.to_le_bytes());
        assert!(Metadata::parse(&wrong_size).is_none());

        let bytes = sample();
        assert!(Metadata::parse(&bytes[..bytes.len() - 1]).is_none());
        assert!(Metadata::parse(&[]).is_none());
    }

    #[test]
    fn spot_temperature_is_deci_celsius() {
        // Temperatures are held in kelvin internally, so a Celsius round trip drifts a few ULPs.
        let m = Metadata::parse(&sample()).expect("parses");
        assert!((m.max.temperature().get::<degree_celsius>() - 26.1).abs() < 0.01);
        assert!((m.min.temperature().get::<degree_celsius>() - 13.8).abs() < 0.01);
    }

    #[test]
    fn spot_coordinates_scale_into_the_thermal_grid() {
        let m = Metadata::parse(&sample()).expect("parses");
        assert_eq!(m.max.thermal_xy(120, 160), (59, 99));
        assert_eq!(m.min.thermal_xy(120, 160), (117, 118));
        assert_eq!(m.center.thermal_xy(120, 160), (60, 80));
    }

    /// The camera puts the centre spot on the far edge, one pixel outside the grid once halved.
    #[test]
    fn spot_coordinates_clamp_to_the_thermal_grid() {
        let spot = Spot { x: 240, y: 320, temperature: 0 };
        assert_eq!(spot.thermal_xy(120, 160), (119, 159));
    }

    /// Encoder and decoder must agree exactly, or spots drift a pixel on every round trip.
    #[test]
    fn thermal_coordinates_round_trip_through_the_visible_space() {
        for (x, y) in [(0u32, 0u32), (59, 99), (60, 80), (119, 159)] {
            let spot = Spot::from_thermal_xy(x, y, 200);
            assert_eq!(spot.thermal_xy(120, 160), (x, y), "spot ({x}, {y}) did not round trip");
        }
    }

    #[test]
    fn date_time_converts_to_exif_convention() {
        let m = Metadata::parse(&sample()).expect("parses");
        assert_eq!(m.exif_date_time().as_deref(), Some("2024:11:21 01:06:39"));
    }

    /// Byte-exactness against a real HT-04D block pins the field offsets in both directions.
    #[test]
    fn encodes_back_to_the_original_bytes() {
        let bytes = sample();
        let m = Metadata::parse(&bytes).expect("parses");
        assert_eq!(m.encode(), bytes);
    }

    #[test]
    fn encode_round_trips_every_field() {
        let m = Metadata {
            model: "HT-19".to_string(),
            firmware: "2.1.19".to_string(),
            date_time: "2025/01/02-03:04:05".to_string(),
            center: Spot { x: 1, y: 2, temperature: 300 },
            max: Spot { x: 3, y: 4, temperature: 1234 },
            min: Spot { x: 5, y: 6, temperature: -456 },
            emissivity: 0.85,
            palette: 1,
            unit: 1,
            mix: 40,
            margins: Some([1, 2, 3, 4]),
        };

        let decoded = Metadata::parse(&m.encode()).expect("parses");
        assert_eq!(decoded.model, m.model);
        assert_eq!(decoded.firmware, m.firmware);
        assert_eq!(decoded.date_time, m.date_time);
        assert_eq!(decoded.center, m.center);
        assert_eq!(decoded.max, m.max);
        assert_eq!(decoded.min, m.min);
        assert_eq!(decoded.emissivity, m.emissivity);
        assert_eq!(decoded.palette, m.palette);
        assert_eq!(decoded.unit, m.unit);
        assert_eq!(decoded.mix, m.mix);
        assert_eq!(decoded.margins, m.margins);
    }

    /// Negative temperatures are why spots are signed; a naive u32 read would wrap them.
    #[test]
    fn negative_spot_temperatures_survive_a_round_trip() {
        let mut m = Metadata::parse(&sample()).expect("parses");
        m.min = Spot { x: 0, y: 0, temperature: -204 };
        let decoded = Metadata::parse(&m.encode()).expect("parses");
        assert_eq!(decoded.min.temperature, -204);
        assert!((decoded.min.temperature().get::<degree_celsius>() + 20.4).abs() < 0.01);
    }

    #[test]
    fn legacy_blocks_re_encode_as_the_current_layout() {
        let mut bytes = sample();
        bytes.truncate(SIZE_LEGACY);
        bytes[..4].copy_from_slice(&(SIZE_LEGACY as u32).to_le_bytes());

        let encoded = Metadata::parse(&bytes).expect("parses").encode();
        assert_eq!(encoded.len(), SIZE_CURRENT);
        let decoded = Metadata::parse(&encoded).expect("parses");
        assert_eq!(decoded.model, "HT-04D");
        assert_eq!(decoded.margins, Some([0, 0, 0, 0]));
    }

    /// Truncation must not corrupt the following fields nor split a multi-byte character.
    #[test]
    fn over_long_text_is_truncated_on_a_char_boundary() {
        let mut m = Metadata::parse(&sample()).expect("parses");
        m.model = "ü".repeat(20);
        let encoded = m.encode();
        assert_eq!(encoded.len(), SIZE_CURRENT);

        let decoded = Metadata::parse(&encoded).expect("parses");
        assert!(decoded.model.chars().all(|c| c == 'ü'));
        assert!(decoded.model.len() < TEXT_LEN);
        assert_eq!(decoded.firmware, "2.5.1");
    }
}
