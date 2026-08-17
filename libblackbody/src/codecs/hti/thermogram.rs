use std::path::PathBuf;

use imgref::ImgVec;
use rgb::RGB8;
use uom::si::f32::ThermodynamicTemperature;

use crate::camera::CameraMetadata;
use crate::capture::CaptureParameters;
use crate::codecs::hti::metadata::Spot;
use crate::visual::decode_jpeg_into_imgvec;
use crate::{Measurement, ThermVec, ThermogramTrait, codecs::hti::decode::HtiThermogram};

impl ThermogramTrait for HtiThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        decode_jpeg_into_imgvec(self.visual.as_slice())
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn camera_metadata(&self) -> CameraMetadata {
        self.camera_metadata.clone()
    }

    /// The metadata block records emissivity and nothing else.
    fn capture_parameters(&self) -> CaptureParameters {
        CaptureParameters {
            emissivity: self.info.as_ref().map(|info| info.emissivity),
            ..Default::default()
        }
    }

    /// The camera's min and max spots: 3x3-averaged, so narrower than the raw thermal extremes.
    fn embedded_render_range(&self) -> Option<[ThermodynamicTemperature; 2]> {
        let info = self.info.as_ref()?;
        Some([info.min.temperature(), info.max.temperature()])
    }

    /// The three spots the camera always records: centre, hottest and coldest.
    fn measurements(&self) -> Vec<Measurement> {
        let Some(info) = self.info.as_ref() else { return Vec::new() };
        let (width, height) = (self.thermal_buffer.width(), self.thermal_buffer.height());

        let spot = |label: &str, spot: &Spot| {
            let (x, y) = spot.thermal_xy(width, height);
            Measurement::Spot { label: label.to_string(), x, y }
        };
        vec![spot("Center", &info.center), spot("Max", &info.max), spot("Min", &info.min)]
    }
}
