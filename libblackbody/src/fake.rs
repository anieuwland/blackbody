use std::path::PathBuf;

use ndarray::{Array, Ix2, Ix3};

use crate::ThermogramTrait;

pub struct Fake(pub Array<f32, Ix2>);

impl ThermogramTrait for Fake {
    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.0
    }
    fn visual(&self) -> Option<Array<u8, Ix3>> {
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
