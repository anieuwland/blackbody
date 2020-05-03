// http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
// https://en.wikipedia.org/wiki/JPEG#Syntax_and_structure
// http://gvsoft.no-ip.org/exif/exif-explanation.html
// https://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files
// https://rdrr.io/cran/Thermimage/man/readflirJPG.html
// https://exiftool.org/TagNames/FLIR.html
// https://github.com/kamadak/exif-rs https://docs.rs/kamadak-exif/0.5.1/exif/
// https://crates.io/crates/implex
// https://github.com/vadixidav/exifsd https://docs.rs/exifsd/0.1.0/exifsd/


use std::fs::File;
use std::path::{Path, PathBuf};

use ndarray::*;

use super::thermogram::Thermogram;

#[derive(Debug, Clone)]
pub struct FlirThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

impl FlirThermogram {
    pub fn new_from_path(file_path: &Path) -> Option<FlirThermogram> {
        let thermal = XenicsThermogram::read_thermal(file_path).unwrap();

        Some(XenicsThermogram {
            thermal: thermal,
            file_path: (*file_path).to_path_buf(),
        })
    }

    fn read_thermal(file_path: &Path) -> Option<Array<f32, Ix2>> {
        return XenicsThermogram::_read_thermal_libtiff(file_path);
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

        match tiff.read_image().unwrap() {
            tiff::decoder::DecodingResult::U8(_) => None,
            tiff::decoder::DecodingResult::U16(values) => {
                let f32_values: Vec<f32> =
                    values.into_iter().map(|integer| integer as f32).collect();

                let thermal = vec_to_ndarray(f32_values);
                let thermal = thermal - 27315.0;
                Some(thermal / 100.0)
            }
            tiff::decoder::DecodingResult::U32(values) => {
                let f32_values = values
                    .into_iter()
                    .map(|integer| unsafe { std::mem::transmute::<u32, f32>(integer) })
                    .collect();

                Some(vec_to_ndarray(f32_values))
            }
            tiff::decoder::DecodingResult::U64(_) => {
                // Untested
                //let f32_values: Vec<f32> = values.into_iter()
                //     .map(|integer| unsafe {
                //         std::mem::transmute::<u64, f64>(integer) as f32
                //     })
                //     .collect();

                //Some(vec_to_ndarray(f32_values))
                None
            }
        }
    }
}

impl Thermogram for FlirThermogram {
    fn identifier(&self) -> String {
        // FIXME unwraps
        //self.file_path.file_name().unwrap().to_str().unwrap().to_string();
        let file_name = self.file_path.file_name();
        file_name.unwrap().to_os_string().into_string().unwrap()
    }

    fn thermal(&self) -> Array<f32, Ix2> {
        self.thermal.clone()
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
        None
    }
}
