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
        let thermogram = FlirThermogram::read_thermal(file_path);
        thermogram
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
}

impl ThermogramTrait for FlirThermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal_buffer
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
        self.thermogram.optical_array().ok()
    }

    fn identifier(&self) -> &str {
        // FIXME unwraps
        let file_name = self.file_path.file_name();
        file_name.unwrap().to_str().unwrap()
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

    return [r, g, b];
}
