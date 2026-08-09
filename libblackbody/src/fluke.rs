use ndarray::*;
use serendip::Thermogram as Serendip;
use std::path::{Path, PathBuf};

use crate::{Measurement, ThermogramTrait};

/// This is the struct and `ThermogramTrait` implementation for Fluke thermograms, using
/// [serendip](https://crates.io/crates/serendip).
#[derive(Clone, Debug)]
pub struct FlukeThermogram {
    pub thermogram: Serendip,
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
        let thermogram = Thermogram::new_from_path(file_path).ok()?;

        let w = thermogram.width().into();
        let h = thermogram.height().into();
        let data: Vec<f32> = thermogram.kelvin()?.pixels().map(|k| k - 273.15).collect();
        let thermal_buffer = Array::from(data).into_shape_with_order(((h, w), Order::C)).ok()?;

        Some(FlukeThermogram { thermogram, file_path: file_path.to_path_buf(), thermal_buffer })
    }
}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<Array<u8, Ix3>> {
        let visual = self.thermogram.visual()?;
        let (w, h) = (visual.width(), visual.height());
        let data: Vec<u8> = visual.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();
        Array::from(data).into_shape_with_order(((h, w, 3), Order::C)).ok()
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    /// Palette in RGB, normalized to 0.0–1.0. Alpha is discarded.
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        self.thermogram.palette().map(|p| {
            p.iter()
                .map(|c| [c.r, c.g, c.b].map(|channel| f32::from(channel) / 255.0))
                .collect()
        })
    }

    fn embedded_render_range(&self) -> Option<[f32; 2]> {
        let scale = self.thermogram.embedded_render_range()?;
        Some([scale.min, scale.max])
    }

    fn measurements(&self) -> Vec<Measurement> {
        self.thermogram.markers().iter().map(Into::into).collect()
    }
}

impl From<&FlukeThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &FlukeThermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}
