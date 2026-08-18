use image::{ExtendedColorType, ImageError, codecs::jpeg::JpegEncoder};
use imgref::ImgVec;
use rgb::RGB8;
use uom::si::f32::ThermodynamicTemperature;
use uom::si::length::meter;
use uom::si::thermodynamic_temperature::kelvin;

use crate::{Error, ThermogramTrait, palettes};

/// Encode any thermogram to the InfiRay IRG file format.
///
/// This is a partially lossy format. Thermal and visible data is compeltely
/// carried over, but not all capture parameters and camera metadata do.
pub fn encode_irg<T: ThermogramTrait + ?Sized>(thermogram: &T) -> Result<Vec<u8>, Error> {
    let (width, height) = (thermogram.thermal().width(), thermogram.thermal().height());
    let visual = thermogram.visual().unwrap_or_else(|| thermogram.render_defaults());
    let (width_v, height_v) = (visual.width() as u16, visual.height() as u16);
    let visual = encode_jpeg(visual)?;

    let width_bs = (width as u16).to_le_bytes();
    let height_bs = (height as u16).to_le_bytes();
    let thermal: Vec<_> = thermogram
        .thermal()
        .pixels()
        .map(|p| (p.get::<kelvin>() * 16.0).round() as u16)
        .flat_map(|c| c.to_le_bytes())
        .collect();

    let [min, max] = thermogram
        .embedded_render_range()
        .unwrap_or_else(|| [thermogram.min_temp(), thermogram.max_temp()]);
    let grayscale: Vec<_> =
        thermogram.render(min, max, &palettes::GRAY).pixels().map(|p| p.r).collect();

    let parameters = thermogram.capture_parameters();
    let distance = parameters.distance.map(|d| d.get::<meter>()).unwrap_or(0.0);

    let header_length = 128u16;
    let mut header = Vec::<u8>::with_capacity(128);
    header.extend(b"\xca\xac");
    header.extend(header_length.to_le_bytes());
    header.extend(((width * height) as u32).to_le_bytes());
    header.extend(&height_bs);
    header.extend(&width_bs);
    header.extend(b"\x00");
    header.extend(((width * height * 2) as u32).to_le_bytes());
    header.extend(&height_bs);
    header.extend(&width_bs);
    header.extend(b"\x00");
    header.extend((visual.len() as u32).to_le_bytes());
    header.extend(height_v.to_le_bytes());
    header.extend(width_v.to_le_bytes());
    header.extend(((parameters.emissivity.unwrap_or(1.0) * 10000f32) as u32).to_le_bytes());
    header.extend(temperature_bs(parameters.reflected_temperature));
    header.extend(temperature_bs(parameters.atmospheric_temperature));
    header.extend(((distance * 10000f32) as u32).to_le_bytes());
    header.extend(4000u32.to_le_bytes());
    header.extend(((parameters.transmissivity.unwrap_or(1.0) * 10000f32) as u32).to_le_bytes());
    header.extend(0u32.to_le_bytes());
    header.extend(10000u32.to_le_bytes());
    header.resize(126, 0);
    header.extend(b"\xac\xca");

    let out_size = header.len() + grayscale.len() + thermal.len() + visual.len();
    let mut contents = Vec::<u8>::with_capacity(out_size);

    contents.extend_from_slice(header.as_slice());
    contents.extend_from_slice(&grayscale);
    contents.extend_from_slice(&thermal);
    contents.extend_from_slice(&visual);

    Ok(contents)
}

fn temperature_bs(temperature: Option<ThermodynamicTemperature>) -> [u8; 4] {
    let temperature = temperature.map(|t| t.get::<kelvin>()).unwrap_or(0.0);
    ((temperature * 10000f32) as u32).to_le_bytes()
}

fn encode_jpeg(image: ImgVec<RGB8>) -> Result<Vec<u8>, Error> {
    let mut out: Vec<u8> = Vec::new();
    let bytes: Vec<_> = image.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();

    JpegEncoder::new_with_quality(&mut out, 95).encode(
        &bytes,
        image.width() as u32,
        image.height() as u32,
        ExtendedColorType::Rgb8,
    ).map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::decode::decode_irg;
    use super::*;
    use crate::Thermogram;
use crate::codecs::irg::format::IrgMagic;
    use rstest::*;

    fn read(name: &str) -> Thermogram {
        let path = format!("{}/thermograms/{name}", env!("CARGO_MANIFEST_DIR"));
        Thermogram::from_file(Path::new(&path)).expect("test thermogram")
    }

    fn encode_and_decode(name: &str) -> (Thermogram, Vec<u8>, crate::IrgThermogram) {
        let thermogram = read(name);
        let irg = encode_irg(&thermogram).expect("encodes as irg");
        let decoded = decode_irg(&irg).expect("decodes as irg");
        (thermogram, irg, decoded)
    }

    #[rstest]
    #[case::flir_t630sc_1("flir_t630sc_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    #[case::flir_e5_2_pip("hti_ht-04d_1.jpg")]
    fn encodes_to_and_decodes_from(#[case] name: &str) {
        let (thermogram, _, destination) = encode_and_decode(name);

        assert_eq!(destination.raw_data.magic, IrgMagic::Caac);
        assert_eq!(destination.raw_data.header_length, 128);

        let orig_dims = [thermogram.thermal().width(), thermogram.thermal().height()];
        let dest_dims = [destination.thermal().width(), destination.thermal().height()];
        assert_eq!(orig_dims, dest_dims);

        destination.visual().expect("has visual");
    }

    #[rstest]
    #[case::flir_t630sc_1("flir_t630sc_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    fn sections_are_declared_at_their_written_length(#[case] name: &str) {
        let (thermogram, bytes, destination) = encode_and_decode(name);
        let raw = &destination.raw_data;
        let (width, height) = (thermogram.thermal().width(), thermogram.thermal().height());

        assert_eq!(raw.grayscale_length as usize, width * height);
        assert_eq!(raw.thermal_length as usize, width * height * 2);
        let declared = 128
            + raw.grayscale_length as usize
            + raw.thermal_length as usize
            + raw.visual_length as usize;
        assert_eq!(declared, bytes.len(), "declared lengths must sum to the file size");

        let visual_start = 128 + raw.grayscale_length as usize + raw.thermal_length as usize;
        assert_eq!(&bytes[visual_start..visual_start + 3], b"\xff\xd8\xff", "jpeg starts here");
    }

    #[rstest]
    #[case::flir_t630sc_1("flir_t630sc_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    #[case::flir_e5_2_pip("hti_ht-04d_1.jpg")]
    #[case::flir_e5_2_pip("zenmuse_xt_ir_1.jpg")]
    fn round_trip_preserves_temperatures(#[case] name: &str) {
        let (thermogram, _, destination) = encode_and_decode(name);
        let raw = &destination.raw_data;
        assert_eq!(raw.magic.divider(raw.divider_flag), 16.0);

        for (source, dest) in thermogram.thermal().pixels().zip(destination.thermal().pixels()) {
            let (source, dest) = (source.get::<kelvin>(), dest.get::<kelvin>());
            assert!((source - dest).abs() <= 0.05, "{source}K came back as {dest}K");
        }
    }

    #[rstest]
    fn sources_without_a_visual_still_get_a_jpeg_appendix() {
        let thermogram = read("flir_sc660_1.jpg");
        assert!(thermogram.visual().is_none());

        let irg = encode_irg(&thermogram).expect("encodes as irg");
        let destination = decode_irg(&irg).expect("decodes as irg");

        let visual = destination.visual().expect("falls back to a render");
        let thermal = destination.thermal();
        assert_eq!([visual.width(), visual.height()], [thermal.width(), thermal.height()]);
    }
}
