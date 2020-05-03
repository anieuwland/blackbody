use std::fs::File;
use std::path::{Path, PathBuf};

use ndarray::*;
//use image::GenericImageView;
//use opencv::imgcodecs::imread;
//use opencv::prelude::*;

use super::thermogram::Thermogram;

#[derive(Debug, Clone)]
pub struct XenicsThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

impl XenicsThermogram {
    pub fn new_from_path(file_path: &Path) -> Option<XenicsThermogram> {
        let thermal = XenicsThermogram::read_thermal(file_path).unwrap();

        Some(XenicsThermogram {
            thermal: thermal,
            file_path: (*file_path).to_path_buf(),
        })
    }

    fn read_thermal(file_path: &Path) -> Option<Array<f32, Ix2>> {
        return XenicsThermogram::_read_thermal_libtiff(file_path);
    }

//    fn _read_thermal_libimage(file_path: &Path) -> Option<Array<f32, Ix2>> {
//        let img = image::open(file_path).unwrap(); // FIXME
//        let dims = img.dimensions();

//        let values: Vec<f32> = img
//            .as_flat_samples_u16()?
//            .to_vec()
//            .samples
//            .into_iter()
//            .map(|integer| integer as f32)
//            .collect();

//        let thermal = ndarray::ArrayBase::from(values);
//        let thermal = thermal
//            .into_shape((dims.1 as usize, dims.0 as usize))
//            .unwrap();

//        let thermal = thermal - 27315.0;
//        let thermal = thermal / 100.0;

//        Some(thermal)
//    }

//    fn _read_thermal_libcv(file_path: &Path) -> Option<Array<f32, Ix2>> {
//        let path_str = file_path.to_str().unwrap();
//        let data = imread(path_str, 2).unwrap();
//        println!(
//            "Reading Xenics file: {:?} dims, {:?}×{:?}",
//            data.dims(),
//            data.cols(),
//            data.rows()
//        );

//        let dim = Dim([data.rows() as usize, data.cols() as usize]);
//        let mut thermal: Array<f32, Ix2> = Array::ones(dim);

//        for y in 0..data.rows() {
//            for x in 0..data.cols() {
//                let val = data.at_2d::<u16>(y, x).unwrap();
//                thermal[[y as usize, x as usize]] = (*val) as f32;
//            }
//        }

//        let thermal = thermal - 27315.0;
//        let thermal = thermal / 100.0;

//        Some(thermal)
//    }

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

impl Thermogram for XenicsThermogram {
    fn identifier(&self) -> String {
        // FIXME unwraps
        let file_name = self.file_path.file_name();
        file_name.unwrap().to_os_string().into_string().unwrap()
    }

    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal
    }

    fn optical(&self) -> Option<&Array<u8, Ix3>> {
        None
    }
}
