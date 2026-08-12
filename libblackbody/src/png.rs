use image::DynamicImage;
use imgref::ImgVec;
use rgb::RGB8;
use std::path::{Path, PathBuf};
use uom::si::thermodynamic_temperature::centikelvin;

use crate::{ThermVec, thermal::into_therm_vec, thermogram_trait::ThermogramTrait};

/// 16-bit grayscale PNG thermogram.
///
/// Values are interpreted as centikelvin (same convention as TIFF U16).
#[derive(Clone, Debug)]
pub struct PngThermogram {
    pub file_path: PathBuf,
    thermal: ThermVec,
}

impl PngThermogram {
    pub fn from_file(file_path: &Path) -> Option<Self> {
        let img = image::open(file_path).ok()?;
        let buf = match img {
            DynamicImage::ImageLuma16(b) => b,
            _ => return None,
        };
        let (width, height) = buf.dimensions();
        let thermal = into_therm_vec::<centikelvin>(
            buf.iter().map(|v| f32::from(*v)),
            width as usize,
            height as usize,
        );
        Some(Self { thermal, file_path: file_path.to_path_buf() })
    }
}

impl ThermogramTrait for PngThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }
    fn visual(&self) -> Option<ImgVec<RGB8>> {
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
    use uom::si::thermodynamic_temperature::kelvin;

    use super::*;
    use crate::{fake::Fake, thermal::into_therm_vec};

    /// Exporting to PNG (centikelvin u16) and reading it back must preserve temperatures
    /// exactly when they fall on centikelvin steps.
    #[test]
    fn export_import_round_trip_preserves_kelvin() {
        let temps = vec![273.15, 300.0, 0.0, 655.35];
        let fake = Fake(into_therm_vec::<kelvin>(temps.clone(), 2, 2));

        let path = std::env::temp_dir().join("blackbody_png_round_trip.png");
        fake.export_thermal_png(&path).expect("export");
        let png = PngThermogram::from_file(&path).expect("reimport");
        let _ = std::fs::remove_file(&path);

        assert_eq!((png.thermal().width(), png.thermal().height()), (2, 2));
        for (orig, read) in temps.iter().zip(png.thermal().pixels()) {
            assert_eq!(*orig, read.get::<kelvin>(), "{orig} K badly round-tripped");
        }
    }
}
