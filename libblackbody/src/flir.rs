use flyr::thermogram::Thermogram as Flyr;
use imgref::{Img, ImgVec};
use log::warn;
use rgb::{FromSlice, RGB8};
use std::path::{Path, PathBuf};
use uom::si::{
    f32::{Length, ThermodynamicTemperature},
    length::meter,
    thermodynamic_temperature::kelvin,
};

use crate::camera::CameraMetadata;
use crate::capture::CaptureParameters;
use crate::pip::{PipGeometry, PipRect};
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

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        self.thermogram
            .palette_info
            .as_ref()
            .map(|info| info.palette.iter().map(|[y, cb, cr]| ycc_to_rgb(*y, *cr, *cb)).collect())
    }

    fn camera_metadata(&self) -> CameraMetadata {
        self.thermogram.camera_metadata.as_ref().map(CameraMetadata::from).unwrap_or_default()
    }

    /// FLIR records the full atmospheric model.
    fn capture_parameters(&self) -> CaptureParameters {
        let info = &self.thermogram.camera_info;
        let temperature = |k: f32| Some(ThermodynamicTemperature::new::<kelvin>(k));
        CaptureParameters {
            emissivity: Some(info.emissivity),
            reflected_temperature: temperature(info.reflected_apparent_temperature),
            atmospheric_temperature: temperature(info.atmospheric_temperature),
            transmissivity: Some(info.ir_window_transmission),
            ir_window_temperature: temperature(info.ir_window_temperature),
            relative_humidity: Some(info.relative_humidity),
            distance: Some(Length::new::<meter>(info.object_distance)),
        }
    }

    /// Measurements (spots, areas, lines, …) embedded in the file.
    /// Coordinates are in thermal-image pixels.
    fn measurements(&self) -> Vec<Measurement> {
        self.thermogram.measurements.iter().map(Into::into).collect()
    }

    /// The embedded crop (`x1..x2`, `y1..y2`), scaled by the real-to-IR ratio and centered
    /// on the visual light image, shifted by the stored offsets.
    fn pip_geometry(&self) -> Option<PipGeometry> {
        let pip = self.thermogram.pip_info.as_ref()?;
        let thermal = self.thermal();
        let (ir_w, ir_h) = (thermal.width() as i64, thermal.height() as i64);

        let x1 = i64::from(pip.x1).clamp(0, ir_w);
        let y1 = i64::from(pip.y1).clamp(0, ir_h);
        let x2 = i64::from(pip.x2).clamp(0, ir_w);
        let y2 = i64::from(pip.y2).clamp(0, ir_h);
        if x2 <= x1 || y2 <= y1 {
            warn!("PiP crop region is empty");
            return None;
        }
        let (src_w, src_h) = ((x2 - x1) as u32, (y2 - y1) as u32);

        // From the embedded image record rather than `visual()`, which decodes the whole JPEG.
        let ei = self.thermogram.embedded_image.as_ref()?;
        let orientation = self.thermogram.orientation.unwrap_or(1);
        let (opt_w, opt_h) = if (5..=8).contains(&orientation) {
            (i64::from(ei.height), i64::from(ei.width))
        } else {
            (i64::from(ei.width), i64::from(ei.height))
        };

        let ratio = opt_w as f32 / ir_w as f32 / pip.real_to_ir;
        let dst_w = (src_w as f32 * ratio).round() as u32;
        let dst_h = (src_h as f32 * ratio).round() as u32;
        let dst_x = opt_w / 2 - i64::from(dst_w) / 2 + i64::from(pip.offset_x);
        let dst_y = opt_h / 2 - i64::from(dst_h) / 2 + i64::from(pip.offset_y);

        Some(PipGeometry {
            source: PipRect { x: x1, y: y1, width: src_w, height: src_h },
            destination: PipRect { x: dst_x, y: dst_y, width: dst_w, height: dst_h },
        })
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
    fn camera_metadata_converts_from_flyr() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let info = t.camera_metadata();

        assert!(t.has_camera_metadata());
        assert_eq!(info.make.as_deref(), Some("FLIR Systems AB"));
        assert_eq!(info.model.as_deref(), Some("FLIR E5"));
        assert!(info.date_time.is_some());

        assert_eq!(info.serial_number, None);
    }

    /// Reading the sample's 20 °C values back in both units pins that no double conversion happens.
    #[test]
    fn capture_parameters_are_read_in_kelvin() {
        use uom::si::length::meter;
        use uom::si::thermodynamic_temperature::degree_celsius;

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_one_g2_1.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let params = t.capture_parameters();

        assert_eq!(params.emissivity, Some(0.95));
        assert_eq!(params.transmissivity, Some(1.0));
        assert_eq!(params.relative_humidity, Some(0.5)); // A fraction, not a percentage

        let reflected = params.reflected_temperature.expect("records one");
        assert!((reflected.get::<kelvin>() - 293.15).abs() < 0.01);
        assert!((reflected.get::<degree_celsius>() - 20.0).abs() < 0.01);
        assert!((params.distance.expect("records one").get::<meter>() - 1.0).abs() < 0.01);
    }

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

    /// Sample stores crop 22..96 × 16..72, ratio 1.3134, offsets (9, -1); on its 120×90 thermal
    /// and 640×480 visual that scales by 640/120/1.3134 ≈ 4.06 and centers at (179, 126).
    #[test]
    fn pip_geometry_translates_stored_form() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let geometry = t.pip_geometry().expect("geometry present");

        assert_eq!(geometry.source, PipRect { x: 22, y: 16, width: 74, height: 56 });
        assert_eq!(geometry.destination, PipRect { x: 179, y: 126, width: 300, height: 227 });
    }

    /// Pixels inside the destination must differ from the plain visual, pixels outside must not.
    #[test]
    fn pip_overlays_thermal_inside_destination_only() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        let visual = t.visual().expect("visual");

        // Destination on this sample: x 179, y 126, 300 × 227 (see geometry test)
        let inside = (179usize + 150, 126usize + 113);
        let outside = (10usize, 10usize);
        assert_ne!(img[inside], visual[inside]);
        assert_eq!(img[outside], visual[outside]);
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
