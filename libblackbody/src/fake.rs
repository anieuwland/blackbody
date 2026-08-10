use std::path::PathBuf;

use imgref::ImgVec;
use rgb::RGB8;

use crate::ThermogramTrait;

pub struct Fake(pub ImgVec<f32>);

impl ThermogramTrait for Fake {
    fn thermal(&self) -> &ImgVec<f32> {
        &self.0
    }
    fn visual(&self) -> Option<ImgVec<RGB8>> {
        None
    }
    fn identifier(&self) -> &str {
        "fake"
    }
    fn path(&self) -> Option<&PathBuf> {
        None
    }
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        None
    }
}
