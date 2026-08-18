use image::{DynamicImage, EncodableLayout};
use imgref::ImgVec;
use rgb::RGB8;
use std::path::{Path, PathBuf};
use uom::si::thermodynamic_temperature::centikelvin;

use crate::{Error, ThermVec, thermal::into_therm_vec, thermogram_trait::ThermogramTrait};

/// 16-bit grayscale PNG thermogram.
///
/// Values are interpreted as centikelvin (same convention as TIFF U16).
#[derive(Clone, Debug)]
pub struct PngThermogram {
    pub file_path: Option<PathBuf>,
    thermal: ThermVec,
}

impl PngThermogram {
    /// Whether the buffer starts with the PNG magic number. A candidate check; whether the
    /// image is a decodable 16-bit grayscale one is only known after `from_bytes`.
    pub fn matches_magic(bytes: &[u8]) -> bool {
        bytes.starts_with(b"\x89PNG")
    }

    pub fn from_file(file_path: &Path) -> Option<Self> {
        let bytes = std::fs::read(file_path).ok()?;
        let mut thermogram = Self::from_bytes(&bytes)?;
        thermogram.file_path = Some(file_path.to_path_buf());
        Some(thermogram)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let img = image::load_from_memory(bytes).ok()?;
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
        Some(Self { thermal, file_path: None })
    }
}

pub fn encode_thermal_png<T: ThermogramTrait + ?Sized>(
    thermogram: &T,
) -> Result<Vec<u8>, crate::Error> {
    let thermal = thermogram.thermal();
    let width = thermal.width() as u32;
    let height = thermal.height() as u32;

    let pixels: Vec<u16> = thermal
        .pixels()
        .map(|c| c.get::<centikelvin>().round().clamp(0.0, 65535.0) as u16)
        .collect();
    let bs: Vec<u8> = image::ImageBuffer::<image::Luma<u16>, _>::from_raw(width, height, pixels)
        .ok_or_else(|| Error::Encode("pixel buffer does not match dimensions".into()))?
        .as_bytes()
        .to_vec();
    Ok(bs)
}

impl ThermogramTrait for PngThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }
    fn visual(&self) -> Option<ImgVec<RGB8>> {
        None
    }
    fn path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use uom::si::thermodynamic_temperature::kelvin;

    use super::*;
    use crate::{codecs::fake::Fake, thermal::into_therm_vec};

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
