use ndarray::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use tiff::decoder::DecodingResult;

use crate::thermogram_trait::ThermogramTrait;

/// This is the struct and `ThermogramTrait` implementation for TIFF thermograms, through the use
/// `image-rs/tiff`.
///
/// A 'TIFF thermogram' is basically any TIFF file with a channel of data, assumed to be
/// thermographic. Bigger integer types (I/U16+) are treated as centikelvin and converted to Celsius
/// by subtracting 27315 and dividing by 100. Float types (F16, F32, F64) are used as-is. U/I8 is
/// refused, as it isn't clear what temperatures 0-255 can meaningfully hold.
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct TiffThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

impl TiffThermogram {
    /// Read a Tiff file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the FLIR file to read.
    ///
    /// # Returns
    /// In case of success, `Some<TiffThermogram>` is returned, otherwise `None`. Values are in
    /// centicelsius, as specified by the `ThermogramTrait` contract.
    pub fn from_file(file_path: &Path) -> Option<Self> {
        let thermal = Self::read_thermal(file_path)?;
        Some(Self { thermal, file_path: file_path.to_path_buf() })
    }

    /// Decodes the first image in the TIFF. Any decode failure (corrupt file,
    /// unexpected sample count) yields `None` rather than a panic.
    fn read_thermal(file_path: &Path) -> Option<Array<f32, Ix2>> {
        let file = File::open(file_path).ok()?;
        let mut tiff = tiff::decoder::Decoder::new(file).ok()?;
        let (width, height) = tiff.dimensions().ok()?;
        let dims = (height as usize, width as usize);
        let to_array = |values: Vec<f32>| Array::from_shape_vec(dims, values).ok();
        let centikelvin_to_celsius = |values: Vec<f32>| to_array(values).map(|a| (a - 27315.0) / 100.0);

        match tiff.read_image().ok()? {
            DecodingResult::U8(_) => None,
            DecodingResult::U16(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::U32(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::U64(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::I8(_) => None,
            DecodingResult::I16(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::I32(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::I64(v) => centikelvin_to_celsius(v.into_iter().map(|x| x as f32).collect()),
            DecodingResult::F16(v) => to_array(v.into_iter().map(f32::from).collect()),
            DecodingResult::F32(v) => to_array(v),
            DecodingResult::F64(v) => to_array(v.into_iter().map(|x| x as f32).collect()),
        }
    }
}

impl From<&TiffThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &TiffThermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}

impl ThermogramTrait for TiffThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
        None
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
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
}
