use std::{io::Cursor, path::Path};

use binrw::{Error, prelude::*};
use uom::si::thermodynamic_temperature::kelvin;

use crate::{
    codecs::irg::format::{IrgThermogram, RawIrgData},
    thermal::into_therm_vec,
};

pub fn decode_irg(bytes: &[u8], file_path: &Path) -> Result<IrgThermogram, Error> {
    let mut cursor = Cursor::new(&bytes);
    let raw_data = cursor.read_le::<RawIrgData>()?;

    let divider = raw_data.magic.divider(raw_data.divider_flag);

    let thermal: Vec<_> = raw_data.thermal.iter().map(|i| f32::from(*i) / divider).collect();
    let thermal_width = raw_data.thermal_width as usize;
    let thermal_height = raw_data.thermal_height as usize;
    let thermal = into_therm_vec::<kelvin>(thermal, thermal_width, thermal_height);

    Ok(IrgThermogram { file_path: file_path.to_path_buf(), thermal, raw_data })
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
    #[case::hti_ht_04d_1("hti_ht-04d_1.irg")]
    #[case::hti_ht_04d_1("infiray_c201_1.irg")]
    #[case::hti_ht_04d_1("vevor_sc240m_1.irg")]
    fn decodes_irg(#[case] name: &str) {
        use std::path::PathBuf;

        let bytes = read(name);
        let irg = decode_irg(bytes.as_slice(), &PathBuf::from(name));
        assert!(irg.is_ok());
    }
}
