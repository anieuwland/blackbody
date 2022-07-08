use ndarray::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use tiff::decoder::DecodingResult;

use crate::thermogram_trait::ThermogramTrait;

/// This is the struct and `ThermogramTrait` implementation for TIFF thermograms, through the use
/// `image-rs/tiff`.
///
/// A 'TIFF thermogram' is basically any TIFF file with a channel of data, assumed to be
/// thermographic. Currently the only supported data types are u16 and u32, which are converted to
/// floats. U16 data is assumed to be centikelvin and converted to centigrades by subtracting 27315.
/// U32 is assumed to actually be a f32 and transmuted to that data type.
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
    /// Read a FLIR file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the FLIR file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlirThermogram>` is returned, otherwise `None`. Values are in
    /// centigrades, as specified by the `ThermogramTrait` contract.
    pub fn from_file(file_path: &Path) -> Option<Self> {
        match Self::_read_thermal(file_path) {
            Some(thermal) => {
                return Some(Self { thermal: thermal, file_path: (*file_path).to_path_buf() });
            }
            _ => return None,
        }
    }

    fn _read_thermal(file_path: &Path) -> Option<Array<f32, Ix2>> {
        return Self::_read_thermal_libtiff(file_path);
    }

    fn _read_thermal_libtiff(file_path: &Path) -> Option<Array<f32, Ix2>> {
        let file = File::open(file_path).unwrap();
        let mut tiff = tiff::decoder::Decoder::new(file).unwrap();
        let tiff_dims = tiff.dimensions().unwrap();
        let arr_dims = Dim((tiff_dims.1 as usize, tiff_dims.0 as usize));
        let vec_to_ndarray = |values| {
            let thermal = ndarray::ArrayBase::from(values);
            let thermal = thermal.into_shape(arr_dims).unwrap();
            thermal
        };

        match tiff.read_image() {
            Ok(image) => match image {
                DecodingResult::U8(values) => {
                    let f32_values: Vec<f32> =
                        values.into_iter().map(|integer| integer as f32).collect();

                    let thermal = vec_to_ndarray(f32_values);
                    Some(thermal)
                }
                DecodingResult::U16(values) => {
                    let f32_values: Vec<f32> =
                        values.into_iter().map(|integer| integer as f32).collect();

                    let thermal = vec_to_ndarray(f32_values);
                    let thermal = thermal - 27315.0;
                    Some(thermal / 100.0)
                }
                DecodingResult::U32(values) => {
                    let f32_values: Vec<f32> =
                        values.into_iter().map(|integer| integer as f32).collect();

                    let thermal = vec_to_ndarray(f32_values);
                    let thermal = thermal - 27315.0;
                    Some(thermal / 100.0)
                }
                DecodingResult::U64(values) => {
                    let f32_values: Vec<f32> =
                        values.into_iter().map(|integer| integer as f32).collect();

                    let thermal = vec_to_ndarray(f32_values);
                    let thermal = thermal - 27315.0;
                    Some(thermal / 100.0)
                }
                DecodingResult::F32(values) => Some(vec_to_ndarray(values)),
                DecodingResult::F64(values) => {
                    Some(vec_to_ndarray(values.iter().map(|v| *v as f32).collect()))
                }
            },
            _ => None,
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
        // FIXME unwraps
        let file_name = self.file_path.file_name();
        file_name.unwrap().to_str().unwrap()
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}
