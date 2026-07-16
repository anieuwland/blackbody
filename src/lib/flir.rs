use flyr::camera_metadata::CameraMetadata;
use flyr::thermogram::FlyrThermogram;
use ndarray::*;
use std::path::{Path, PathBuf};

use crate::{CaptureParams, ThermogramTrait};

/// This is the struct and `ThermogramTrait` implementation for FLIR thermograms, using
/// [flyr](https://crates.io/crates/flyr).
///
/// While a file can be directly read with `from_file`, it is recommended to instead use the
/// `Thermogram::from_file` instead. The latter detects what kind of file (TIFF, FLIR) it is dealing
/// with, subsequently choosing the right reader for it. This way your application support different
/// thermogram formats.
#[derive(Clone, Debug)]
pub struct FlirThermogram {
    thermogram: FlyrThermogram,
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

        Some(FlirThermogram {
            thermogram,
            file_path: file_path.to_path_buf(),
            thermal_buffer,
        })
    }
}

impl FlirThermogram {
    pub fn capture_params(&self) -> CaptureParams {
        let ci = &self.thermogram.camera_info;
        CaptureParams {
            emissivity: ci.emissivity,
            object_distance_m: ci.object_distance,
            reflected_temp_k: ci.reflected_apparent_temperature,
            relative_humidity: ci.relative_humidity,
            planck_r1: ci.planck_r1,
            planck_r2: ci.planck_r2,
            planck_b: ci.planck_b,
            planck_f: ci.planck_f,
            planck_o: ci.planck_o,
        }
    }

    pub fn camera_metadata(&self) -> Option<&CameraMetadata> {
        self.thermogram.camera_metadata.as_ref()
    }

    /// Measurement tools (spots, areas, lines, …) embedded in the file.
    /// Coordinates are in thermal-image pixels.
    pub fn measurements(&self) -> &[flyr::measurement_info::Measurement] {
        &self.thermogram.measurements
    }

    pub fn has_pip(&self) -> bool {
        self.thermogram.pip_info.is_some() && self.thermogram.embedded_image.is_some()
    }

    /// Composite the thermal render onto the optical image using the embedded PIP geometry.
    /// Temperatures in celsius, palette colors in 0.0–1.0 RGB, as elsewhere in this crate.
    pub fn picture_in_picture(
        &self,
        min_temp: f32,
        max_temp: f32,
        palette: &[[f32; 3]],
    ) -> Option<Array<u8, Ix3>> {
        let to_u8 = |f: f32| (f * 255.0) as u8;
        let colors = palette.iter().map(|c| [to_u8(c[0]), to_u8(c[1]), to_u8(c[2])]).collect();
        let normalization = flyr::units::Normalization::Explicit {
            min: min_temp + 273.15,
            max: max_temp + 273.15,
        };
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
}

impl ThermogramTrait for FlirThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
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
    use crate::Measurement;

    // The reference values in the two measurement tests below are the min/max/avg the
    // camera itself rendered into the JPEG overlay. Tolerances absorb the difference
    // between flyr's and FLIR's Planck evaluation, not geometry errors: a wrong region
    // (e.g. reading width/height as a second corner) is off by whole degrees.

    #[test]
    fn area_stats_match_camera_overlay() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../flyr-rs/thermograms/flir_sc660_1.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let area = t.measurements().iter()
            .find(|m| matches!(m, Measurement::Area { .. }))
            .expect("sc660_1 should contain an area measurement");
        let s = t.measurement_stats(area).expect("area stats");
        // Camera overlay: Max 34.8, Min 22.7, Avg 28.1
        assert!((s.avg - 28.1).abs() < 0.1, "avg {} != 28.1", s.avg);
        assert!((s.min - 22.7).abs() < 0.5, "min {} != 22.7", s.min);
        assert!((s.max - 34.8).abs() < 0.7, "max {} != 34.8", s.max);
    }

    #[test]
    fn ellipse_stats_match_camera_overlay() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"), "/../flyr-rs/thermograms/flir_thermocam_b400_2.jpg"
        );
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let ellipse = t.measurements().iter()
            .find(|m| matches!(m, Measurement::Ellipse { .. }))
            .expect("b400_2 should contain an ellipse measurement");
        let s = t.measurement_stats(ellipse).expect("ellipse stats");
        // Camera overlay: El1 Max -0.1
        assert!((s.max - -0.1).abs() < 0.2, "max {} != -0.1", s.max);
    }

    #[test]
    fn pip_composite_has_optical_shape() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../flyr-rs/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        assert!(t.has_pip());
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        // Optical is 640x480 vs 120x90 thermal; RGB channels last.
        assert_eq!(img.shape(), &[480, 640, 3]);
    }
}
