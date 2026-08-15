use image::{ExtendedColorType, ImageError, codecs::jpeg::JpegEncoder};
use imgref::ImgVec;
use rgb::RGB8;
use uom::si::thermodynamic_temperature::degree_celsius;

use crate::{Thermogram, ThermogramTrait, palettes};

pub fn encode_hti(thermogram: &Thermogram) -> Result<Vec<u8>, ImageError> {
    let render = thermogram.render_defaults();
    let visual = thermogram.visual().unwrap_or(render.clone());
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

    let metadata_length = 104u32;
    let metadata_length_bs = metadata_length.to_le_bytes();
    let null_metadata = [0u8].repeat(metadata_length as usize - metadata_length_bs.len());

    let out_size = render.len()
        + visual.len()
        + width.len()
        + height.len()
        + thermal.len()
        + grayscale.len()
        + metadata_length_bs.len()
        + null_metadata.len();
    let mut contents = Vec::<u8>::with_capacity(out_size);

    contents.extend_from_slice(&render);
    contents.extend_from_slice(&visual);
    contents.extend_from_slice(&width);
    contents.extend_from_slice(&height);
    contents.extend_from_slice(&thermal);
    contents.extend_from_slice(&width);
    contents.extend_from_slice(&height);
    contents.extend_from_slice(&grayscale);
    contents.extend_from_slice(&metadata_length_bs);
    contents.extend_from_slice(&null_metadata);

    Ok(contents)
}

fn encode_jpeg(image: ImgVec<RGB8>) -> Result<Vec<u8>, ImageError> {
    let mut out: Vec<u8> = Vec::new();
    let bytes: Vec<_> = image.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();

    JpegEncoder::new_with_quality(&mut out, 95).encode(
        &bytes,
        image.width() as u32,
        image.height() as u32,
        ExtendedColorType::Rgb8,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::decode;
    use super::*;
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

        let destination = decode::decode_hti(&hti, None).expect("decodable {name} as hti");
        let orig_dims = [thermogram.thermal().width(), thermogram.thermal().height()];
        let dest_dims = [destination.thermal().width(), destination.thermal().height()];
        assert_eq!(orig_dims, dest_dims);

        let orig_visual = thermogram.visual().expect("has visual");
        let dest_visual = destination.visual().expect("has visual");
        let orig_dims = [orig_visual.width(), orig_visual.height()];
        let dest_dims = [dest_visual.width(), dest_visual.height()];
        assert_eq!(orig_dims, dest_dims);
    }
}
