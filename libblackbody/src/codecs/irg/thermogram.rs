use std::path::{Path, PathBuf};

use imgref::ImgVec;
use rgb::RGB8;

use crate::{
    ThermVec, ThermogramTrait,
    codecs::irg::{decode::decode_irg, format::IrgThermogram},
    visual::decode_jpeg_into_imgvec,
};

impl IrgThermogram {
    pub fn from_file(file_path: &Path) -> Option<IrgThermogram> {
        let bytes = std::fs::read(file_path).ok()?;
        let mut thermogram = IrgThermogram::from_bytes(&bytes)?;
        thermogram.file_path = Some(file_path.to_path_buf());
        Some(thermogram)
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<IrgThermogram> {
        decode_irg(bytes).ok()
    }
}

impl ThermogramTrait for IrgThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        decode_jpeg_into_imgvec(self.raw_data.appendix.jpeg()?)
    }

    fn path(&self) -> Option<&PathBuf> {
        self.file_path.as_ref()
    }
}
