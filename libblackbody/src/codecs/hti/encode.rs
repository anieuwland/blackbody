use image::{ExtendedColorType, RgbImage, codecs::jpeg::JpegEncoder, imageops};
use imgref::{Img, ImgVec};
use rgb::{ComponentBytes, FromSlice, RGB8};
use uom::si::f32::ThermodynamicTemperature;
use uom::si::thermodynamic_temperature::degree_celsius;

use crate::codecs::hti::metadata::{Metadata, Spot, VISUAL_SCALE};
use crate::{Error, ThermogramTrait, palettes};

/// Encode any thermogram to the HTI file format.
///
/// Warning, lossy conversion! HTI seems to assume visible images are exactly
/// twice the size of thermal images, so this encoder scales the visible light
/// image up or down to make that true. If there is no visible light image, a
/// render is made and used instead.
///
/// Additionally, HTI doesn't store many capture parameters or camera metadata.
/// It also only stores a fixed set of measurements. Custom measurements will
/// not carry over.
pub fn encode_hti<T: ThermogramTrait + ?Sized>(thermogram: &T) -> Result<Vec<u8>, Error> {
    let thermal_dims = (thermogram.thermal().width(), thermogram.thermal().height());
    let render = thermogram.render_defaults();
    let visual = thermogram.visual().unwrap_or_else(|| render.clone());
    let visual = fit_visual(visual, thermal_dims);
    let render = encode_jpeg(render)?;
    let visual = encode_jpeg(visual)?;

    let width = (thermogram.thermal().width() as u16).to_le_bytes();
    let height = (thermogram.thermal().height() as u16).to_le_bytes();
    let thermal: Vec<_> = thermogram
        .thermal()
        .pixels()
        .map(|p| (p.get::<degree_celsius>() * 10.0).round() as i16)
        .flat_map(|c| c.to_le_bytes())
        .collect();

    let [min, max] = thermogram
        .embedded_render_range()
        .unwrap_or_else(|| [thermogram.min_temp(), thermogram.max_temp()]);
    let grayscale: Vec<_> =
        thermogram.render(min, max, &palettes::GRAY).pixels().map(|p| p.r).collect();

    let metadata = build_metadata(thermogram, min, max).encode();

    let out_size = render.len()
        + visual.len()
        + width.len()
        + height.len()
        + thermal.len()
        + width.len()
        + height.len()
        + grayscale.len()
        + metadata.len();
    let mut contents = Vec::<u8>::with_capacity(out_size);

    contents.extend_from_slice(&render);
    contents.extend_from_slice(&visual);
    contents.extend_from_slice(&width);
    contents.extend_from_slice(&height);
    contents.extend_from_slice(&thermal);
    contents.extend_from_slice(&width);
    contents.extend_from_slice(&height);
    contents.extend_from_slice(&grayscale);
    contents.extend_from_slice(&metadata);

    Ok(contents)
}

/// Centre-crop and resize the visible image to the [`VISUAL_SCALE`] ratio spots are stored in.
fn fit_visual(visual: ImgVec<RGB8>, thermal_dims: (usize, usize)) -> ImgVec<RGB8> {
    let (target_width, target_height) =
        (thermal_dims.0 * VISUAL_SCALE as usize, thermal_dims.1 * VISUAL_SCALE as usize);
    if target_width == 0 || target_height == 0 || visual.width() == 0 || visual.height() == 0 {
        return visual;
    }
    if (visual.width(), visual.height()) == (target_width, target_height) {
        return visual;
    }

    let (width, height) = (visual.width() as u32, visual.height() as u32);
    let Some(mut source) = RgbImage::from_raw(width, height, visual.buf().as_bytes().to_vec())
    else {
        return visual;
    };

    // Aspect ratios are compared as a cross product to stay in integers.
    let (crop_width, crop_height) =
        if u64::from(width) * target_height as u64 > u64::from(height) * target_width as u64 {
            // Wider than the target: trim the sides.
            let cropped = u64::from(height) * target_width as u64 / target_height as u64;
            (cropped as u32, height)
        } else {
            // Taller: trim top and bottom.
            let cropped = u64::from(width) * target_height as u64 / target_width as u64;
            (width, cropped as u32)
        };
    let (x, y) = ((width - crop_width) / 2, (height - crop_height) / 2);
    let cropped = imageops::crop(&mut source, x, y, crop_width, crop_height).to_image();

    let resized = imageops::resize(
        &cropped,
        target_width as u32,
        target_height as u32,
        imageops::FilterType::Triangle,
    );
    Img::new(resized.into_raw().as_rgb().to_vec(), target_width, target_height)
}

/// Build a metadata block, leaving fields the source format does not carry at neutral values.
fn build_metadata<T: ThermogramTrait + ?Sized>(
    thermogram: &T,
    min: ThermodynamicTemperature,
    max: ThermodynamicTemperature,
) -> Metadata {
    let thermal = thermogram.thermal();
    let (width, height) = (thermal.width(), thermal.height());
    let deci_celsius = |t: ThermodynamicTemperature| (t.get::<degree_celsius>() * 10.0).round();

    let spot_at = |x: usize, y: usize, temperature: ThermodynamicTemperature| {
        Spot::from_thermal_xy(x as u32, y as u32, deci_celsius(temperature) as i32)
    };
    let find = |wanted: ThermodynamicTemperature| {
        let index = thermal.pixels().position(|p| p == wanted).unwrap_or(0);
        (index % width.max(1), index / width.max(1))
    };

    let (min_x, min_y) = find(min);
    let (max_x, max_y) = find(max);
    let center = thermal.pixels().nth((height / 2) * width + width / 2);

    Metadata {
        model: thermogram.camera_metadata().model.unwrap_or_default(),
        firmware: String::new(),
        date_time: String::new(),
        center: spot_at(width / 2, height / 2, center.unwrap_or(min)),
        max: spot_at(max_x, max_y, max),
        min: spot_at(min_x, min_y, min),
        emissivity: thermogram.capture_parameters().emissivity.unwrap_or(1.0),
        palette: 0,
        unit: 0,
        mix: 0,
        margins: Some([0; 4]),
    }
}

fn encode_jpeg(image: ImgVec<RGB8>) -> Result<Vec<u8>, Error> {
    let mut out: Vec<u8> = Vec::new();
    let bytes: Vec<_> = image.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();

    JpegEncoder::new_with_quality(&mut out, 95)
        .encode(&bytes, image.width() as u32, image.height() as u32, ExtendedColorType::Rgb8)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::decode;
    use super::*;
    use crate::{Measurement, Thermogram};
    use rstest::*;

    fn read(name: &str) -> Thermogram {
        let path = format!("{}/thermograms/{name}", env!("CARGO_MANIFEST_DIR"));
        let path = Path::new(&path);
        Thermogram::from_file(path).expect("test thermogram")
    }

    #[rstest]
    #[case::flir_e5_2_pip("flir_e5_2-pip.jpg")]
    #[case::flir_one_g2_1("flir_one_g2_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    fn encodes_to_and_decodes_from(#[case] name: &str) {
        let thermogram = read(name);
        let hti = encode_hti(&thermogram).expect("encoded {name} as hti");
        assert!(decode::is_hti_jpeg(&hti));

        let destination = decode::decode_hti(&hti).expect("decodable {name} as hti");
        let orig_dims = [thermogram.thermal().width(), thermogram.thermal().height()];
        let dest_dims = [destination.thermal().width(), destination.thermal().height()];
        assert_eq!(orig_dims, dest_dims);

        thermogram.visual().expect("source has visual");
        let dest_visual = destination.visual().expect("has visual");
        assert_eq!(
            dest_visual.width() * destination.thermal().height(),
            dest_visual.height() * destination.thermal().width()
        );
    }

    /// The encoder writes a real metadata block rather than the zero padding it used to.
    #[rstest]
    #[case::flir_e5_2_pip("flir_e5_2-pip.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    fn round_trip_preserves_metadata(#[case] name: &str) {
        let thermogram = read(name);
        let hti = encode_hti(&thermogram).expect("encodes as hti");
        let destination = decode::decode_hti(&hti).expect("decodes as hti");

        let info = destination.info.as_ref().expect("metadata block parses");
        assert_eq!(info.margins, Some([0, 0, 0, 0]));

        let source = thermogram.capture_parameters().emissivity.expect("source records one");
        assert!(
            (info.emissivity - source).abs() <= 0.005,
            "emissivity {} does not match the source's {source}",
            info.emissivity
        );
        assert_eq!(destination.capture_parameters().emissivity, Some(info.emissivity));

        if let Some(model) = thermogram.camera_metadata().model {
            assert_eq!(info.model, model);
            assert_eq!(destination.camera_metadata().model.as_deref(), Some(info.model.as_str()));
        }

        let [min, max] = thermogram
            .embedded_render_range()
            .unwrap_or_else(|| [thermogram.min_temp(), thermogram.max_temp()]);
        let [dest_min, dest_max] =
            destination.embedded_render_range().expect("spots give a render range");
        assert!((dest_min.get::<degree_celsius>() - min.get::<degree_celsius>()).abs() < 0.1);
        assert!((dest_max.get::<degree_celsius>() - max.get::<degree_celsius>()).abs() < 0.1);

        let measurements = destination.measurements();
        assert_eq!(measurements.len(), 3);
        let thermal = destination.thermal();
        for m in &measurements {
            let Measurement::Spot { x, y, .. } = m else { panic!("expected spots, got {m:?}") };
            assert!((*x as usize) < thermal.width() && (*y as usize) < thermal.height());
        }

        // A scale mismatch between encoder and decoder lands this at a fraction of the centre.
        let expected = ((thermal.width() / 2) as u32, (thermal.height() / 2) as u32);
        assert!(
            matches!(measurements[0], Measurement::Spot { x, y, .. } if (x, y) == expected),
            "centre spot should be at {expected:?}, got {:?}",
            measurements[0]
        );
    }

    /// Sources carrying no emissivity still produce a valid block, at the neutral value.
    #[rstest]
    fn sources_without_emissivity_fall_back_to_neutral() {
        let thermogram = read("flir_e5_2-pip.jpg");
        let png = format!("{}/thermograms/hti-emissivity.png", env!("CARGO_MANIFEST_DIR"));
        let png = Path::new(&png);
        thermogram.export_thermal_png(&png.to_path_buf()).expect("exports a png");
        let stripped = Thermogram::from_file(png).expect("reads back the png");
        std::fs::remove_file(png).ok();

        assert!(!stripped.has_capture_parameters());
        let hti = encode_hti(&stripped).expect("encodes as hti");
        let destination = decode::decode_hti(&hti).expect("decodes as hti");
        assert_eq!(destination.info.as_ref().expect("metadata block").emissivity, 1.0);
    }

    /// Sources vary widely (FLIR E5 5.33x, Fluke 4x, FLIR One 2x), so this is a real resize.
    #[rstest]
    #[case::flir_e5_2_pip("flir_e5_2-pip.jpg")]
    #[case::flir_one_g2_1("flir_one_g2_1.jpg")]
    #[case::fluke_ti400_1("fluke_ti400_1.is2")]
    fn visual_is_written_at_twice_the_thermal_resolution(#[case] name: &str) {
        let thermogram = read(name);
        let hti = encode_hti(&thermogram).expect("encodes as hti");
        let destination = decode::decode_hti(&hti).expect("decodes as hti");

        let thermal = destination.thermal();
        let visual = destination.visual().expect("has visual");
        assert_eq!([visual.width(), visual.height()], [thermal.width() * 2, thermal.height() * 2]);
    }

    #[rstest]
    fn mismatched_aspect_ratios_are_cropped_not_stretched() {
        // A 100x100 source against a 40x20 thermal frame targets 80x40, so top and bottom go.
        let visual = Img::new(vec![RGB8::new(1, 2, 3); 100 * 100], 100, 100);
        let fitted = fit_visual(visual, (40, 20));
        assert_eq!([fitted.width(), fitted.height()], [80, 40]);

        // The check above passes under a stretch too, so verify the geometry on distinct bands.
        let mut pixels = vec![RGB8::new(0, 0, 0); 100 * 100];
        for (i, p) in pixels.iter_mut().enumerate() {
            // A centre crop of a 2:1 band drops this red top quarter entirely.
            if i / 100 < 25 {
                *p = RGB8::new(255, 0, 0);
            }
        }
        let fitted = fit_visual(Img::new(pixels, 100, 100), (40, 20));
        assert!(
            fitted.pixels().all(|p| p.r < 128),
            "the cropped-away red band should not appear in the result"
        );
    }
}
