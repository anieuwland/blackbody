use flyr::camera_metadata::CameraMetadata;
use flyr::thermogram::Thermogram as Flyr;
use imgref::{Img, ImgVec};
use log::warn;
use rgb::{FromSlice, RGB8};
use std::path::{Path, PathBuf};
use uom::si::{f32::ThermodynamicTemperature, thermodynamic_temperature::kelvin};

use crate::{Measurement, ThermVec, ThermogramTrait, thermal::into_therm_vec};

/// This is the struct and `ThermogramTrait` implementation for FLIR thermograms, using
/// [flyr](https://crates.io/crates/flyr).
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct FlirThermogram {
    pub thermogram: Flyr,
    pub file_path: PathBuf,
    thermal: ThermVec,
}

impl FlirThermogram {
    /// Read a FLIR file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the FLIR file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlirThermogram>` is returned, otherwise `None`.
    pub fn from_file(file_path: &Path) -> Option<FlirThermogram> {
        FlirThermogram::read_thermal(file_path)
    }

    fn read_thermal(file_path: &Path) -> Option<FlirThermogram> {
        let thermogram = Flyr::new_from_path(file_path).ok()?;
        let thermal = thermogram.kelvin();
        let (width, height) = (thermogram.width(), thermogram.height());

        let (expected, length) = (width * height, thermal.len());
        if expected != length {
            warn!(
                "Thermal data did not contain expected amount of pixels: expected {expected}, got {length}"
            );
            return None;
        }
        let thermal = into_therm_vec::<kelvin>(thermal, width, height);

        Some(FlirThermogram { thermogram, file_path: file_path.to_path_buf(), thermal })
    }
}

impl ThermogramTrait for FlirThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        let bytes = self.thermogram.optical().ok()?;
        let width = *self.thermogram.optical_width()? as usize;
        let height = *self.thermogram.optical_height()? as usize;

        let (expected, length) = (width * height * 3, bytes.len());
        if expected != length {
            warn!(
                "Visual light image did not contain expected amount of bytes: expected {expected}, got {length}"
            );
            return None;
        }

        // as_rgb reinterprets the bytes in place; to_vec is then a single memcpy.
        Some(Img::new(bytes.as_rgb().to_vec(), width, height))
    }

    fn has_visual(&self) -> bool {
        self.thermogram.embedded_image.is_some()
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        self.thermogram
            .palette_info
            .as_ref()
            .map(|info| info.palette.iter().map(|[y, cb, cr]| ycc_to_rgb(*y, *cr, *cb)).collect())
    }

    fn camera_metadata(&self) -> Option<&CameraMetadata> {
        self.thermogram.camera_metadata.as_ref()
    }

    /// Measurements (spots, areas, lines, …) embedded in the file.
    /// Coordinates are in thermal-image pixels.
    fn measurements(&self) -> Vec<Measurement> {
        self.thermogram.measurements.iter().map(Into::into).collect()
    }

    fn has_pip(&self) -> bool {
        self.thermogram.pip_info.is_some() && self.thermogram.embedded_image.is_some()
    }

    /// Composite the thermal render onto the visual light image using the embedded PiP geometry.
    /// Palette colors in 0.0–1.0 RGB.
    fn picture_in_picture(
        &self,
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> Option<ImgVec<RGB8>> {
        let to_u8 = |f: f32| (f * 255.0) as u8;
        let colors = palette.iter().map(|c| [to_u8(c[0]), to_u8(c[1]), to_u8(c[2])]).collect();
        let normalization = flyr::units::Normalization::Explicit {
            min: min_temp.get::<kelvin>(),
            max: max_temp.get::<kelvin>(),
        };
        let rgba = self
            .thermogram
            .picture_in_picture(&flyr::units::Palette::Custom(colors), &normalization)
            .ok()?;

        // The composite has the orientation-corrected visual light dimensions.
        let ei = self.thermogram.embedded_image.as_ref()?;
        let orientation = self.thermogram.orientation.unwrap_or(1);
        let (w, h) = if (5..=8).contains(&orientation) {
            (ei.height as usize, ei.width as usize)
        } else {
            (ei.width as usize, ei.height as usize)
        };
        let (expected, length) = (w * h * 4, rgba.len());
        if expected != length {
            warn!(
                "PiP composite did not contain expected amount of bytes: expected {expected}, got {length}"
            );
            return None;
        }

        // as_rgba reinterprets the bytes in place; dropping alpha is then a single pass.
        let pixels: Vec<RGB8> = rgba.as_rgba().iter().map(|p| p.rgb()).collect();
        Some(Img::new(pixels, w, h))
    }

    fn embedded_render_range(&self) -> Option<[ThermodynamicTemperature; 2]> {
        let range = self.thermogram.embedded_range(flyr::units::Temperature::Kelvin);
        Some([
            ThermodynamicTemperature::new::<kelvin>(range[0]),
            ThermodynamicTemperature::new::<kelvin>(range[1]),
        ])
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

#[cfg(test)]
mod tests {
    use uom::si::{f32::TemperatureInterval, temperature_interval};

    use super::*;

    #[test]
    fn pip_composite_has_visual_shape() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        assert!(t.has_pip());
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        // Visual is 640x480 vs 120x90 thermal; RGB channels last.
        assert_eq!([img.width(), img.height()], [640, 480]);
    }

    #[test]
    fn pip_normalization_range_is_kelvin() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        // If min/max were passed to flyr in the wrong unit (regression: celsius instead of
        // kelvin), the range lands entirely below the data and every thermal pixel clamps to
        // the same palette color — making a correct range indistinguishable from one shifted
        // down by 273.15. A correct implementation renders these two ranges differently.
        let shift = TemperatureInterval::new::<temperature_interval::kelvin>(273.15);
        let good = t.picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO);
        let shifted = t.picture_in_picture(
            t.min_temp() - shift,
            t.max_temp() - shift,
            &crate::palettes::TURBO,
        );
        assert_ne!(good.unwrap().buf(), shifted.unwrap().buf());
    }
}
