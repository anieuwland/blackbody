use std::io::Cursor;
use std::path::PathBuf;

use image::{DynamicImage, codecs::jpeg::JpegDecoder};
use imgref::{Img, ImgVec};
use log::warn;
use rgb::{FromSlice, RGB8};

use crate::{ThermVec, ThermogramTrait, codecs::hti::decode::HtiThermogram};

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
}
