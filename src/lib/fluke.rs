use serendip::SerendipThermogram;
use ndarray::*;
use std::path::{Path, PathBuf};

use crate::ThermogramTrait;

/// This is the struct and `ThermogramTrait` implementation for FLIR thermograms, using
/// [flyr](https://crates.io/crates/flyr).
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct FlukeThermogram {
    pub thermogram: SerendipThermogram,
    file_path: PathBuf,
    thermal_buffer: Array<f32, Ix2>,
}

impl FlukeThermogram {
    /// Read a Fluke file (is2) referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlukeThermogram>` is returned, otherwise `None`. Values are in
    /// centigrades, as specified by the `ThermogramTrait` contract.
    pub fn from_file(file_path: &Path) -> Option<FlukeThermogram> {
        FlukeThermogram::read_thermal(file_path)
    }

    fn read_thermal(file_path: &Path) -> Option<FlukeThermogram> {
        let thermogram = SerendipThermogram::new_from_path(file_path).ok()?;

        let w = thermogram.width().into();
        let h = thermogram.height().into();
        let data: Vec<f32> = thermogram.kelvin()?.iter().map(|k| k - 273.15).collect();
        let thermal_buffer = Array::from(data).into_shape_with_order(((h, w), Order::C)).ok()?;

        Some(FlukeThermogram {
            thermogram,
            file_path: file_path.to_path_buf(),
            thermal_buffer,
        })
    }
}

impl FlukeThermogram {

}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
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

impl From<&FlukeThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &FlukeThermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}

fn ycc_to_rgb(y: u8, cb: u8, cr: u8) -> [f32; 3] {
    let r = y as f32 + 1.4075 * (cr as f32 - 128f32);
    let g = y as f32 - 0.3455 * (cb as f32 - 128f32) - (0.7169 * (cr as f32 - 128f32));
    let b = y as f32 + 1.7790 * (cb as f32 - 128f32);

    let r = r.clamp(0f32, 255f32) / 255f32;
    let g = g.clamp(0f32, 255f32) / 255f32;
    let b = b.clamp(0f32, 255f32) / 255f32;

    [r, g, b]
}
