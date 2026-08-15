use std::{
    ops::Div,
    path::{Path, PathBuf},
};

use log::warn;
use uom::si::thermodynamic_temperature::degree_celsius;

use flyr::camera_metadata::CameraMetadata;

use crate::codecs::hti::metadata::Metadata;
use crate::{ThermVec, thermal::into_therm_vec};

/// The decoded contents of an HTI/ToolTop JPEG.
#[derive(Clone, Debug)]
pub struct HtiThermogram {
    pub file_path: PathBuf,
    /// The raw metadata block, including its leading `u32` size field.
    pub metadata: Vec<u8>,
    /// The metadata block's parsed fields, or `None` if it could not be interpreted.
    pub info: Option<Metadata>,
    /// Camera make/model and capture time, derived from [`Self::info`].
    pub camera_metadata: Option<CameraMetadata>,
    /// Temperatures as stored in the file: deci-degrees Celsius, one per pixel.
    pub thermal: Vec<i16>,
    /// The same temperatures converted to a [`ThermVec`].
    pub thermal_buffer: ThermVec,
    /// The embedded visible-light image, as JPEG bytes.
    pub visual: Vec<u8>,
}

impl HtiThermogram {
    /// Read an HTI/ToolTop file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    /// In case of success, `Some<HtiThermogram>` is returned, otherwise `None`.
    pub fn from_file(file_path: &Path) -> Option<HtiThermogram> {
        let bytes = std::fs::read(file_path).ok()?;
        decode_hti(&bytes, Some(file_path))
    }
}

/// Decode an HTI/ToolTop JPEG, returning `None` if any section is missing or malformed.
pub fn decode_hti(bytes: &[u8], file_path: Option<&Path>) -> Option<HtiThermogram> {
    // Structure:
    // 1. JPEG #1. Skip until end of file.
    // 2. JPEG #2: Visible light image.
    // 3. Thermal header: 4 bytes, 2 u16s meaning width * height
    // 4. Thermal data: width * height * 2 bytes; interpret as i16 decicelsius
    // 5. Thermal header again.
    // 6. Grayscale render: width * height u8 grayscale intensity. Skip.
    // 7. Metadata

    // TODO Find start of #2 in better way
    let bytes = seek_to_visual_image(bytes)?;
    let (visual, bytes) = extract_visual_data(bytes)?;
    let (thermal, width, height, bytes) = decode_thermal_data(bytes)?;
    let bytes = seek_to_metadata(bytes)?;
    let metadata = decode_metadata(bytes)?;
    let info = Metadata::parse(metadata);
    if info.is_none() {
        warn!("Failed parsing the HTI metadata block; continuing without camera information");
    }
    let thermal_buffer = into_therm_vec::<degree_celsius>(
        thermal.iter().map(|t| f32::from(*t).div(10.0)),
        width as usize,
        height as usize,
    );

    Some(HtiThermogram {
        file_path: file_path.map(Path::to_path_buf).unwrap_or_default(),
        metadata: metadata.to_vec(),
        camera_metadata: info.as_ref().map(camera_metadata),
        info,
        thermal,
        thermal_buffer,
        visual: visual.to_vec(),
    })
}

/// Report whether `bytes` looks like an HTI/ToolTop JPEG.
///
/// HTI files carry no magic number, so this walks the whole section layout and only accepts a
/// file whose blocks line up exactly with its length.
pub fn is_hti_jpeg(bytes: &[u8]) -> bool {
    // FIXME Implement more orbust 0xFF0xD8 and 0xFF0xD9 checking

    // Check if 2 JPEGs present
    let Some((start, bytes)) = bytes.split_at_checked(2) else { return false };
    if start != [0xFF, 0xD8] {
        return false;
    }
    let Some(end_position) = bytes.windows(4).position(|w| w == [0xFF, 0xD9, 0xFF, 0xD8]) else {
        return false;
    };
    let Some(bytes) = bytes.get(end_position + 2..) else { return false };
    let Some((start, bytes)) = bytes.split_at_checked(2) else { return false };
    if start != [0xFF, 0xD8] {
        return false;
    }
    let Some(end_position) = bytes.windows(2).position(|w| w == [0xFF, 0xD9]) else {
        return false;
    };
    let Some(bytes) = bytes.get(end_position + 2..) else { return false };

    // Check if thermal data and render are present
    let Some((width, height, bytes)) = decode_thermal_header(bytes) else { return false };
    let length = width as usize * height as usize * 2;
    let Some(bytes) = bytes.get(length..) else { return false };
    let Some((width, height, bytes)) = decode_thermal_header(bytes) else { return false };
    let length = width as usize * height as usize;
    let Some(bytes) = bytes.get(length..) else { return false };

    // There should still be left over bytes for the metadata
    let Some((length_bs, bytes)) = bytes.split_first_chunk::<4>() else { return false };
    let length = u32::from_le_bytes(*length_bs);
    if length != 112 && length != 104 {
        return false;
    };
    if bytes.len() + 4 != length as usize {
        return false;
    };

    // All section found: file passes
    true
}

/// Skip past the first JPEG, returning the bytes from the visible-light image onwards.
fn seek_to_visual_image(bytes: &[u8]) -> Option<&[u8]> {
    // TODO Implement more reliably
    let eof = bytes.windows(4).position(|bs| bs == [0xFF, 0xD9, 0xFF, 0xD8])?;
    let (_, bytes) = bytes.split_at_checked(eof + 2)?;
    Some(bytes)
}

/// Split off the visible-light JPEG, returning it and the bytes that follow it.
fn extract_visual_data(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    // TODO Implement more reliably
    let eof = bytes.windows(2).position(|bs| bs == [0xFF, 0xD9])?;
    let result = bytes.split_at_checked(eof + 2)?;
    Some(result)
}

/// Read a block header of two little-endian `u16`s, returning width, height and the remainder.
///
/// Zero dimensions are rejected, as both blocks that use this header describe a real image.
fn decode_thermal_header(bytes: &[u8]) -> Option<(u16, u16, &[u8])> {
    let (width_bs, bytes) = bytes.split_first_chunk::<2>()?;
    let (height_bs, bytes) = bytes.split_first_chunk::<2>()?;
    let (width, height) = (u16::from_le_bytes(*width_bs), u16::from_le_bytes(*height_bs));
    if width == 0 || height == 0 {
        return None;
    };
    Some((width, height, bytes))
}

/// Read the temperature block, returning its values in deci-degrees Celsius with its dimensions.
fn decode_thermal_data(bytes: &[u8]) -> Option<(Vec<i16>, u16, u16, &[u8])> {
    let (width, height, bytes) = decode_thermal_header(bytes)?;
    let length = width as usize * height as usize * 2;
    let (thermal_bs, bytes) = bytes.split_at_checked(length)?;
    let (thermal_chunks, remainder) = thermal_bs.as_chunks::<2>();

    let remainder_length = remainder.len();
    if remainder_length != 0 {
        warn!(
            "Expected to have 0 bytes left over while decoding HTI thermal data, still found {remainder_length}"
        )
    }
    let thermal = thermal_chunks.iter().map(|bs| i16::from_le_bytes(*bs)).collect();

    Some((thermal, width, height, bytes))
}

/// Skip past the grayscale block, returning the bytes from the metadata block onwards.
fn seek_to_metadata(bytes: &[u8]) -> Option<&[u8]> {
    let (width, height, bytes) = decode_thermal_header(bytes)?;
    let length = width as usize * height as usize;
    let (_, bytes) = bytes.split_at_checked(length)?;
    Some(bytes)
}

/// Return the raw metadata block; see [`Metadata::parse`] for its fields.
fn decode_metadata(bytes: &[u8]) -> Option<&[u8]> {
    Some(bytes)
}

/// HTI stores no make, lens or GPS information, so only model and capture time are filled in.
fn camera_metadata(info: &Metadata) -> CameraMetadata {
    CameraMetadata {
        make: Some("HTI".to_string()),
        model: Some(info.model.clone()),
        focal_length: None,
        date_time: info.exif_date_time(),
        gps_latitude: None,
        gps_longitude: None,
        gps_altitude: None,
        gps_img_direction: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;

    fn read(name: &str) -> Vec<u8> {
        let path = format!("{}/thermograms/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).expect("test thermogram")
    }

    /// The HT-04D sample stores a 120x160 portrait thermal frame; its trailer is laid out as
    /// temperature block, grayscale block and a metadata block.
    #[rstest]
    #[case::hti_ht_04d_1("hti_ht-04d_1.jpg")]
    fn detects_hti_files(#[case] name: &str) {
        assert!(is_hti_jpeg(&read(name)));
    }

    /// Other formats are JPEGs too (Fluke is2 files embed one), so detection must reject them
    /// structurally rather than on a magic number. FLIR One files also append data after EOI,
    /// which is exactly the trait a naive "has trailing bytes" check would trip over.
    #[rstest]
    #[case::flir_e5_2_pip("flir_e5_2-pip.jpg")]
    #[case::flir_one_g2_1("flir_one_g2_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    fn rejects_other_formats(#[case] name: &str) {
        assert!(!is_hti_jpeg(&read(name)));
    }

    #[rstest]
    fn rejects_empty_input() {
        assert!(!is_hti_jpeg(&[]));
    }

    /// Lopping bytes off the tail must break the exact block arithmetic the detector relies on.
    #[rstest]
    #[case::one_byte(1)]
    #[case::metadata_block(112)]
    #[case::grayscale_block(120 * 160)]
    fn rejects_truncated_input(#[case] missing: usize) {
        let bytes = read("hti_ht-04d_1.jpg");
        assert!(!is_hti_jpeg(&bytes[..bytes.len() - missing]));
    }

    #[rstest]
    fn decodes_the_hti_sample() {
        let hti = decode_hti(&read("hti_ht-04d_1.jpg"), None).expect("decodes");

        assert_eq!(hti.thermal.len(), 120 * 160);
        assert_eq!((hti.thermal_buffer.width(), hti.thermal_buffer.height()), (120, 160));

        // Stored as deci-degrees Celsius: 13.3 C to 27.9 C.
        assert_eq!(hti.thermal.iter().copied().min(), Some(133));
        assert_eq!(hti.thermal.iter().copied().max(), Some(279));
        assert_eq!(&hti.thermal[..3], &[162, 162, 163]);

        // Buffer block parses temperature correctly
        assert_eq!(hti.thermal_buffer[(0usize, 0usize)].get::<degree_celsius>().round(), 16.0);

        // The visible-light image is a complete JPEG.
        assert_eq!(&hti.visual[..2], &[0xFF, 0xD8]);
        assert_eq!(&hti.visual[hti.visual.len() - 2..], &[0xFF, 0xD9]);

        // Metadata block: a u32 size of 112 followed by the fields themselves.
        assert_eq!(hti.metadata.len(), 112);
        assert_eq!(u32::from_le_bytes(hti.metadata[..4].try_into().unwrap()), 112);
        assert!(hti.metadata[4..24].starts_with(b"HT-04D\0"));

        let info = hti.info.as_ref().expect("metadata parses");
        assert_eq!(info.model, "HT-04D");
        assert_eq!(info.firmware, "2.5.1");
        assert_eq!(info.date_time, "2024/11/21-01:06:39");
        assert_eq!(info.emissivity, 0.95);
        assert_eq!((info.center.x, info.center.y), (120, 160));
        assert_eq!(info.max.temperature, 261);
        assert_eq!(info.min.temperature, 138);
    }

    /// The trait methods the metadata feeds, checked against a real sample not a synthetic block.
    #[rstest]
    fn exposes_metadata_through_the_trait() {
        use crate::{Measurement, ThermogramTrait};

        let hti = decode_hti(&read("hti_ht-04d_1.jpg"), None).expect("decodes");

        let camera = hti.camera_metadata().expect("camera metadata");
        assert_eq!(camera.model.as_deref(), Some("HT-04D"));
        assert_eq!(camera.date_time.as_deref(), Some("2024:11:21 01:06:39"));

        // Narrower than the raw extremes (13.3 to 27.9 C): the camera samples a 3x3 neighbourhood.
        let [min, max] = hti.embedded_render_range().expect("render range");
        assert!((min.get::<degree_celsius>() - 13.8).abs() < 0.01);
        assert!((max.get::<degree_celsius>() - 26.1).abs() < 0.01);

        // The visible image is 240x320, so spots halve into the 120x160 thermal grid.
        let measurements = hti.measurements();
        assert_eq!(measurements.len(), 3);
        let labels: Vec<_> = measurements
            .iter()
            .map(|m| match m {
                Measurement::Spot { label, .. } => label.as_str(),
                other => panic!("expected spots, got {other:?}"),
            })
            .collect();
        assert_eq!(labels, ["Center", "Max", "Min"]);
        assert!(matches!(measurements[0], Measurement::Spot { x: 60, y: 80, .. }));
        assert!(matches!(measurements[1], Measurement::Spot { x: 59, y: 99, .. }));
        assert!(matches!(measurements[2], Measurement::Spot { x: 117, y: 118, .. }));
    }
}
