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
        decode_irg(bytes.as_slice(), &PathBuf::from(file_path)).ok()
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
        Some(&self.file_path)
    }
}
