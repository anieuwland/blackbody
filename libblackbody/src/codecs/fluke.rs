use imgref::ImgVec;
use rgb::RGB8;
use serendip::Thermogram as Serendip;
use std::path::{Path, PathBuf};
use uom::si::{
    f32::ThermodynamicTemperature,
    thermodynamic_temperature::{degree_celsius, kelvin},
};

use crate::camera::CameraMetadata;
use crate::capture::CaptureParameters;
use crate::pip::{PipGeometry, PipRect};
use crate::{Measurement, ThermVec, ThermogramTrait, thermal::into_therm_vec};

/// This is the struct and `ThermogramTrait` implementation for Fluke thermograms, using
/// [serendip](https://crates.io/crates/serendip).
#[derive(Clone, Debug)]
pub struct FlukeThermogram {
    pub thermogram: Serendip,
    pub file_path: Option<PathBuf>,
    thermal: ThermVec,
}

impl FlukeThermogram {
    /// Read a Fluke file (is2) referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlukeThermogram>` is returned, otherwise `None`.
    pub fn from_file(file_path: &Path) -> Option<FlukeThermogram> {
        let bytes = std::fs::read(file_path).ok()?;
        let mut thermogram = FlukeThermogram::from_bytes(&bytes)?;
        thermogram.file_path = Some(file_path.to_path_buf());
        Some(thermogram)
    }

    /// Whether the buffer starts like a Fluke is2 file: the blob format's magic number, or a
    /// zip archive (shared with any zip-based format). A candidate check, not a guarantee.
    pub fn matches_magic(bytes: &[u8]) -> bool {
        bytes.starts_with(serendip::parsing::blob::BLOB_FILE_MAGIC_BYTES)
            || bytes.starts_with(b"PK\x03\x04")
    }

    /// Decode a Fluke is2 thermogram from an in-memory buffer.
    ///
    /// # Returns
    /// In case of success, `Some<FlukeThermogram>` is returned, otherwise `None`.
    pub fn from_bytes(bytes: &[u8]) -> Option<FlukeThermogram> {
        let thermogram = Serendip::new_from_bytes(bytes).ok()?;
        let thermal = thermogram.kelvin()?;
        let thermal = into_therm_vec::<kelvin>(thermal.pixels(), thermal.width(), thermal.height());

        Some(FlukeThermogram { thermogram, file_path: None, thermal })
    }
}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        self.thermogram.visual()
    }

    fn has_visual(&self) -> bool {
        match &self.thermogram {
            Serendip::Zip(t) => !t.visuals.is_empty(),
            Serendip::Blob(t) => t.visual_data.is_some(),
        }
    }

    fn path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }

    /// Palette in RGB, normalized to 0.0–1.0. Alpha is discarded.
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        self.thermogram.palette().map(|p| {
            p.iter().map(|c| [c.r, c.g, c.b].map(|channel| f32::from(channel) / 255.0)).collect()
        })
    }

    /// Only zip files identify the camera; blob files leave the section unparsed.
    fn camera_metadata(&self) -> CameraMetadata {
        let Serendip::Zip(zip) = &self.thermogram else { return CameraMetadata::default() };
        let text = |s: &String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        CameraMetadata {
            make: text(&zip.camera_info.manufacturer),
            serial_number: text(&zip.camera_info.camera_serial),
            ..Default::default()
        }
    }

    /// serendip reports the background temperature in Celsius.
    fn capture_parameters(&self) -> CaptureParameters {
        let info = self.thermogram.ir_image_info();
        // Blob files store no transmission; serendip substitutes 1.0 to run its correction.
        let transmissivity = match &self.thermogram {
            Serendip::Zip(_) => Some(info.transmission()),
            Serendip::Blob(_) => None,
        };
        CaptureParameters {
            emissivity: Some(info.emissivity()),
            reflected_temperature: Some(ThermodynamicTemperature::new::<degree_celsius>(
                info.background_temperature(),
            )),
            transmissivity,
            ..Default::default()
        }
    }

    fn embedded_render_range(&self) -> Option<[ThermodynamicTemperature; 2]> {
        let scale = self.thermogram.embedded_render_range()?;
        Some([
            ThermodynamicTemperature::new::<kelvin>(scale[0]),
            ThermodynamicTemperature::new::<kelvin>(scale[1]),
        ])
    }

    fn measurements(&self) -> Vec<Measurement> {
        self.thermogram.markers().iter().map(Into::into).collect()
    }

    /// The whole thermal render lands on the file's IR footprint; only zip-style files store one.
    fn pip_geometry(&self) -> Option<PipGeometry> {
        let footprint = self.thermogram.ir_footprint()?;
        let thermal = self.thermal();
        Some(PipGeometry {
            source: PipRect {
                x: 0,
                y: 0,
                width: thermal.width() as u32,
                height: thermal.height() as u32,
            },
            destination: PipRect {
                x: i64::from(footprint.x),
                y: i64::from(footprint.y),
                width: footprint.width,
                height: footprint.height,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ti400_sample() -> FlukeThermogram {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/fluke_ti400_1.is2");
        FlukeThermogram::from_file(Path::new(path)).expect("test thermogram")
    }

    #[test]
    fn camera_metadata_comes_from_the_camera_info_section() {
        let info = ti400_sample().camera_metadata();
        assert_eq!(info.make.as_deref(), Some("Fluke Thermography"));
        assert_eq!(info.serial_number.as_deref(), Some("M13080110"));
        assert_eq!(info.model, None);
        assert_eq!(info.description().as_deref(), Some("Fluke Thermography"));
    }

    /// Reading the value back in both units pins that Celsius was not mistaken for kelvin.
    #[test]
    fn capture_parameters_convert_from_celsius() {
        let params = ti400_sample().capture_parameters();
        assert_eq!(params.emissivity, Some(0.95));
        assert_eq!(params.transmissivity, Some(1.0));

        let reflected = params.reflected_temperature.expect("records one");
        assert!((reflected.get::<degree_celsius>() - 22.0).abs() < 0.01);
        assert!((reflected.get::<kelvin>() - 295.15).abs() < 0.01);

        assert_eq!(params.atmospheric_temperature, None);
        assert_eq!(params.relative_humidity, None);
        assert_eq!(params.distance, None);
    }

    /// Footprint values verified against the visual frame in serendip.
    #[test]
    fn pip_geometry_maps_whole_thermal_onto_footprint() {
        let t = ti400_sample();
        let geometry = t.pip_geometry().expect("geometry present");

        assert_eq!(geometry.source, PipRect { x: 0, y: 0, width: 320, height: 240 });
        assert_eq!(geometry.destination, PipRect { x: 399, y: 252, width: 462, height: 346 });
    }

    /// The composite takes the full visual frame's shape, not the thermal's (320×240) or the
    /// display crop's (640×480, which `visual()` returns).
    #[test]
    fn pip_composite_has_visual_frame_shape() {
        let t = ti400_sample();
        assert!(t.has_pip());
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        assert_eq!([img.width(), img.height()], [1280, 960]);
    }

    /// Pixels inside the footprint must differ from the plain visual frame, pixels outside must not.
    #[test]
    fn pip_overlays_thermal_inside_footprint_only() {
        let t = ti400_sample();
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        let frame = t.thermogram.visual().expect("visual frame");

        // Footprint on this sample: x 399, y 252, 462 × 346 (see serendip)
        let inside = (399usize + 231, 252usize + 173);
        let outside = (10usize, 10usize);
        assert_ne!(img[inside], frame[inside]);
        assert_eq!(img[outside], frame[outside]);
    }
}
