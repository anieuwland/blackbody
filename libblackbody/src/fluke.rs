use imgref::{Img, ImgVec};
use rgb::RGB8;
use serendip::{Thermogram as Serendip};
use std::path::{Path, PathBuf};

use crate::{Measurement, ThermogramTrait};

/// This is the struct and `ThermogramTrait` implementation for Fluke thermograms, using
/// [serendip](https://crates.io/crates/serendip).
#[derive(Clone, Debug)]
pub struct FlukeThermogram {
    pub thermogram: Serendip,
    pub file_path: PathBuf,
    thermal: ImgVec<f32>,
}

impl FlukeThermogram {
    /// Read a Fluke file (is2) referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlukeThermogram>` is returned, otherwise `None`. Values are in
    /// celsius, as specified by the `ThermogramTrait` contract.
    pub fn from_file(file_path: &Path) -> Option<FlukeThermogram> {
        FlukeThermogram::read_thermal(file_path)
    }

    fn read_thermal(file_path: &Path) -> Option<FlukeThermogram> {
        let thermogram = Serendip::new_from_path(file_path).ok()?;
        let kelvin = thermogram.kelvin()?;

        // FIXME Standardize on kelvin in libblackbody (Ugly transform to celsius)
        let (buf, w, h) = kelvin.into_contiguous_buf();
        let thermal = Img::new(buf.into_iter().map(|k| k - 273.15).collect(), w, h);

        Some(FlukeThermogram { thermogram, file_path: file_path.to_path_buf(), thermal })
    }
}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &ImgVec<f32> {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        self.thermogram.visual()
    }

    fn has_optical(&self) -> bool {
        match &self.thermogram {
            Serendip::Zip(t) => !t.visuals.is_empty(),
            Serendip::Blob(t) => t.visual_data.is_some(),
        }
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
            p.iter().map(|c| [c.r, c.g, c.b].map(|channel| f32::from(channel) / 255.0)).collect()
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
