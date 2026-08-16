use std::io::Cursor;
use std::path::PathBuf;

use flyr::camera_metadata::CameraMetadata;
use image::{DynamicImage, codecs::jpeg::JpegDecoder};
use imgref::{Img, ImgVec};
use log::warn;
use rgb::{FromSlice, RGB8};
use uom::si::f32::ThermodynamicTemperature;

use crate::capture::CaptureParameters;
use crate::codecs::hti::metadata::Spot;
use crate::{Measurement, ThermVec, ThermogramTrait, codecs::hti::decode::HtiThermogram};

impl ThermogramTrait for HtiThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        let decoder = JpegDecoder::new(Cursor::new(self.visual.as_slice()))
            .inspect_err(|e| warn!("Failed reading HTI visual light JPEG header: {e}"))
            .ok()?;
        let visual = DynamicImage::from_decoder(decoder)
            .inspect_err(|e| warn!("Failed decoding HTI visual light JPEG: {e}"))
            .ok()?;

        // Grayscale and CMYK JPEGs decode to other color types, so normalize to RGB8.
        let (width, height) = (visual.width() as usize, visual.height() as usize);
        let visual = visual.into_rgb8().into_raw();

        // as_rgb reinterprets the bytes in place; to_vec is then a single memcpy.
        Some(Img::new(visual.as_rgb().to_vec(), width, height))
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    fn camera_metadata(&self) -> Option<&CameraMetadata> {
        self.camera_metadata.as_ref()
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
