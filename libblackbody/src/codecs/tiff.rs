use imgref::ImgVec;
use rgb::RGB8;
use tiff::encoder::{TiffEncoder, colortype};
use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use tiff::decoder::DecodingResult;
use uom::si::thermodynamic_temperature::{centikelvin, kelvin};

use crate::{Error, ThermVec};
use crate::thermal::into_therm_vec;
use crate::thermogram_trait::ThermogramTrait;

/// This is the struct and `ThermogramTrait` implementation for TIFF thermograms, through the use
/// `image-rs/tiff`.
///
/// A 'TIFF thermogram' is basically any TIFF file with a channel of data, assumed to be
/// thermographic. Bigger integer types (I/U16+) are assumed to be in centikelvin.
/// Float types (F16, F32, F64) are used as-is. U/I8 is refused, as it isn't clear what
/// temperatures 0-255 can meaningfully hold.
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct TiffThermogram {
    pub file_path: PathBuf,
    thermal: ThermVec,
}

impl TiffThermogram {
    /// Read a Tiff file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the TIFF file to read.
    ///
    /// # Returns
    /// In case of success, `Some<TiffThermogram>` is returned, otherwise `None`.
    pub fn from_file(file_path: &Path) -> Option<Self> {
        let thermal = Self::read_thermal(file_path)?;
        Some(Self { thermal, file_path: file_path.to_path_buf() })
    }

    /// Decodes the first image in the TIFF. Any decode failure (corrupt file,
    /// unexpected sample count) yields `None` rather than a panic.
    fn read_thermal(file_path: &Path) -> Option<ThermVec> {
        let file = File::open(file_path).ok()?;
        let mut tiff = tiff::decoder::Decoder::new(file).ok()?;
        let (width, height) = tiff.dimensions().ok()?;
        let (width, height) = (width as usize, height as usize);

        match tiff.read_image().ok()? {
            DecodingResult::U8(_) | DecodingResult::I8(_) => None,
            DecodingResult::U16(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(f32::from), width, height))
            }
            DecodingResult::U32(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(|v| v as f32), width, height))
            }
            DecodingResult::U64(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(|v| v as f32), width, height))
            }
            DecodingResult::I16(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(f32::from), width, height))
            }
            DecodingResult::I32(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(|v| v as f32), width, height))
            }
            DecodingResult::I64(v) => {
                Some(into_therm_vec::<centikelvin>(v.into_iter().map(|v| v as f32), width, height))
            }
            DecodingResult::F16(v) => {
                Some(into_therm_vec::<kelvin>(v.into_iter().map(f32::from), width, height))
            }
            DecodingResult::F32(v) => Some(into_therm_vec::<kelvin>(v, width, height)),
            DecodingResult::F64(v) => {
                Some(into_therm_vec::<kelvin>(v.into_iter().map(|v| v as f32), width, height))
            }
        }
    }
}

pub fn encode_thermal_tiff<T: ThermogramTrait + ?Sized>(
    thermogram: &T,
) -> Result<Vec<u8>, Error> {
    let thermal = thermogram.thermal();
    let width = thermogram.thermal_shape()[1] as u32;
    let height = thermogram.thermal_shape()[0] as u32;
    let thermal = thermal.pixels().map(|t| t.get::<kelvin>()).collect::<Vec<f32>>();

    let mut cursor = Cursor::new(Vec::with_capacity((width * height) as usize));
    let mut tiff = TiffEncoder::new(&mut cursor).map_err(|e| Error::Encode(e.to_string()))?;
    let _ = tiff.write_image::<colortype::Gray32Float>(width, height, &thermal)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(cursor.into_inner())
}

impl ThermogramTrait for TiffThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        None
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::codecs::fake::Fake;

    use super::*;

    /// A file with a TIFF magic number but corrupt contents must yield None,
    /// not panic — Thermogram::from_file routes it here on magic number alone.
    #[test]
    fn corrupt_tiff_returns_none() {
        let path = std::env::temp_dir().join("blackbody_corrupt_test.tif");
        std::fs::write(&path, b"II*\0this is not a valid tiff body").unwrap();
        assert!(TiffThermogram::from_file(&path).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_none() {
        assert!(TiffThermogram::from_file(Path::new("/nonexistent/no.tif")).is_none());
    }

    /// F32 export (kelvin) and U16 import (centikelvin) both go through `into_therm_vec`;
    /// a round trip through `export_thermal` checks the F32-as-kelvin path exactly.
    #[test]
    fn export_import_round_trip_preserves_kelvin() {
        use uom::si::thermodynamic_temperature::kelvin;

        use crate::thermal::into_therm_vec;

        let temps = vec![0.0, 273.15, 300.0, 1000.5];
        let fake = Fake(into_therm_vec::<kelvin>(temps.clone(), 2, 2));

        let path = std::env::temp_dir().join("blackbody_tiff_round_trip.tif");
        fake.export_thermal(&path).expect("export");
        let tiff = TiffThermogram::from_file(&path).expect("reimport");
        let _ = std::fs::remove_file(&path);

        let read: Vec<f32> = tiff.thermal().pixels().map(|t| t.get::<kelvin>()).collect();
        assert_eq!(read, temps);
    }
}
