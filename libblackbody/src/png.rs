use image::DynamicImage;
use imgref::{Img, ImgVec};
use rgb::RGB8;
use std::path::{Path, PathBuf};

use crate::thermogram_trait::ThermogramTrait;

/// 16-bit grayscale PNG thermogram.
///
/// Values are interpreted as centikelvin (same convention as TIFF U16):
/// `celsius = (raw_u16 - 27315) / 100`
#[derive(Clone, Debug)]
pub struct PngThermogram {
    pub file_path: PathBuf,
    thermal: ImgVec<f32>,
}

impl PngThermogram {
    pub fn from_file(file_path: &Path) -> Option<Self> {
        let img = image::open(file_path).ok()?;
        let buf = match img {
            DynamicImage::ImageLuma16(b) => b,
            _ => return None,
        };
        let (w, h) = buf.dimensions();
        let values: Vec<f32> =
            buf.into_raw().into_iter().map(|v| (v as f32 - 27315.0) / 100.0).collect();
        let thermal = Img::new(values, w as usize, h as usize);
        Some(Self { thermal, file_path: file_path.to_path_buf() })
    }
}

impl ThermogramTrait for PngThermogram {
    fn thermal(&self) -> &ImgVec<f32> {
        &self.thermal
    }
    fn visual(&self) -> Option<ImgVec<RGB8>> {
        None
    }
    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }
    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}
