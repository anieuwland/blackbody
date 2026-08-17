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
    use std::path::PathBuf;

    use super::*;
    use rstest::*;

    fn read(name: &str) -> Vec<u8> {
        let path = format!("{}/thermograms/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).expect("test thermogram")
    }

    fn decode(name: &str) -> IrgThermogram {
        decode_irg(read(name).as_slice(), &PathBuf::from(name)).expect("decodes")
    }

    #[rstest]
    #[case::hti_ht_04d_1("hti_ht-04d_1.irg")]
    #[case::hti_ht_04d_1("infiray_c201_1.irg")]
    #[case::hti_ht_04d_1("vevor_sc240m_1.irg")]
    #[case::topdon_tc004_1("topdon_tc004_1.irg")]
    #[case::topdon_tc004_2("topdon_tc004_2.irg")]
    #[case::topdon_tc004_3("topdon_tc004_3.irg")]
    #[case::topdon_tc004_4("topdon_tc004_4.irg")]
    #[case::topdon_tc004_5("topdon_tc004_5.irg")]
    fn decodes_irg(#[case] name: &str) {
        let bytes = read(name);
        let irg = decode_irg(bytes.as_slice(), &PathBuf::from(name));
        assert!(irg.is_ok());
    }

    /// Pin the actual temperatures, not just that decoding succeeded.
    #[rstest]
    #[case::hti_ht_04d_1("hti_ht-04d_1.irg", 286.5, 301.1)]
    #[case::infiray_c201_1("infiray_c201_1.irg", 299.0, 322.375)]
    #[case::vevor_sc240m_1("vevor_sc240m_1.irg", 292.5, 415.1)]
    #[case::topdon_tc004_1("topdon_tc004_1.irg", 281.5, 296.6)]
    #[case::topdon_tc004_5("topdon_tc004_5.irg", 255.2, 307.4)]
    fn decodes_thermal_extremes(#[case] name: &str, #[case] min: f32, #[case] max: f32) {
        let thermal = decode(name).thermal;
        let (decoded_min, decoded_max) = thermal
            .pixels()
            .map(|p| p.get::<kelvin>())
            .fold((f32::MAX, f32::MIN), |(lo, hi), t| (lo.min(t), hi.max(t)));

        assert!((decoded_min - min).abs() < 0.01, "min was {decoded_min}, expected {min}");
        assert!((decoded_max - max).abs() < 0.01, "max was {decoded_max}, expected {max}");
    }
}
