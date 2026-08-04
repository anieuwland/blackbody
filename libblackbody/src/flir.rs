use flyr::camera_metadata::CameraMetadata;
use flyr::thermogram::FlyrThermogram;
use ndarray::*;
use std::path::{Path, PathBuf};

use crate::{Measurement, ThermogramTrait};

/// This is the struct and `ThermogramTrait` implementation for FLIR thermograms, using
/// [flyr](https://crates.io/crates/flyr).
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct FlirThermogram {
    pub thermogram: FlyrThermogram,
    file_path: PathBuf,
    thermal_buffer: Array<f32, Ix2>,
}

impl FlirThermogram {
    /// Read a FLIR file referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the FLIR file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlirThermogram>` is returned, otherwise `None`. Values are in
    /// centigrades, as specified by the `ThermogramTrait` contract.
    pub fn from_file(file_path: &Path) -> Option<FlirThermogram> {
        FlirThermogram::read_thermal(file_path)
    }

    fn read_thermal(file_path: &Path) -> Option<FlirThermogram> {
        let thermogram = FlyrThermogram::new_from_path(file_path).ok()?;
        let thermal_buffer = thermogram.celsius_array()?;

        Some(FlirThermogram { thermogram, file_path: file_path.to_path_buf(), thermal_buffer })
    }
}

impl ThermogramTrait for FlirThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<Array<u8, Ix3>> {
        self.thermogram.optical_array().ok()
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

    /// Composite the thermal render onto the optical image using the embedded PIP geometry.
    /// Temperatures in celsius, palette colors in 0.0–1.0 RGB, as elsewhere in this crate.
    fn picture_in_picture(
        &self,
        min_temp: f32,
        max_temp: f32,
        palette: &[[f32; 3]],
    ) -> Option<Array<u8, Ix3>> {
        let to_u8 = |f: f32| (f * 255.0) as u8;
        let colors = palette.iter().map(|c| [to_u8(c[0]), to_u8(c[1]), to_u8(c[2])]).collect();
        let normalization = flyr::units::Normalization::Explicit { min: min_temp + 273.15, max: max_temp + 273.15 };
        let rgba = self
            .thermogram
            .picture_in_picture(&flyr::units::Palette::Custom(colors), &normalization)
            .ok()?;

        // The composite has the orientation-corrected optical dimensions.
        let ei = self.thermogram.embedded_image.as_ref()?;
        let orientation = self.thermogram.orientation.unwrap_or(1);
        let (w, h) = if (5..=8).contains(&orientation) {
            (ei.height as usize, ei.width as usize)
        } else {
            (ei.width as usize, ei.height as usize)
        };
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        Array::from_shape_vec((h, w, 3), rgb).ok()
    }

    fn embedded_render_range(&self) -> Option<[f32; 2]> {
        Some(self.thermogram.embedded_range(flyr::units::Temperature::Celsius))
    }
}

impl From<&FlirThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &FlirThermogram) -> Array<f32, Ix2> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pip_composite_has_optical_shape() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        assert!(t.has_pip());
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        // Optical is 640x480 vs 120x90 thermal; RGB channels last.
        assert_eq!(img.shape(), &[480, 640, 3]);
    }
}
