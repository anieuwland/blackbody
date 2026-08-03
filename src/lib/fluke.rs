use log::{debug, error};
use ndarray::*;
use serendip::SerendipThermogram;
use std::path::{Path, PathBuf};

use crate::{Measurement, ThermogramTrait};

/// This is the struct and `ThermogramTrait` implementation for Fluke thermograms, using
/// [serendip](https://crates.io/crates/serendip).
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

        Some(FlukeThermogram { thermogram, file_path: file_path.to_path_buf(), thermal_buffer })
    }
}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<Array<u8, Ix3>> {
        let bytes = self.thermogram.visual()?;
        let (width, height, visual) = decode_jpeg(bytes)?;
        Array::from(visual)
            .into_shape_with_order(((height, width, 3), Order::C))
            .inspect_err(|e| error!("{e}"))
            .ok()
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

    fn measurements(&self) -> Vec<Measurement> {
        let markers = match &self.thermogram {
            SerendipThermogram::Zip(t) => &t.markers,
        };
        markers.iter().map(Into::into).collect()
    }
}

impl From<&FlukeThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &FlukeThermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}

fn decode_jpeg(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    debug!("Decoding jpeg");
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .inspect_err(|e| debug!("JPEG decode failed: {e}"))
        .ok()?
        .into_rgb8();
    let (width, height) = (img.width() as usize, img.height() as usize);
    debug!("Decoded image dimensions: {width}×{height}");
    Some((width, height, img.into_raw()))
}
