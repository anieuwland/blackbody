use std::path::PathBuf;

use imgref::ImgVec;
use rgb::RGB8;

use crate::{ThermVec, ThermogramTrait, codecs::hti::decode::Hti};

impl ThermogramTrait for Hti {
    fn thermal(&self) -> &ThermVec {
        &self.thermal_buffer
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        todo!()
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }
}
