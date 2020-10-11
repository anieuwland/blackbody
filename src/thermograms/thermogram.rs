use ndarray::*;
use std::fs::File;
use std::path::Path;
use std::io::Read;

use crate::thermograms::*;

#[derive(Clone)]
pub enum Thermogram {
    Flir(FlirThermogram),
    Tiff(TiffThermogram),
}

impl Thermogram {
    pub fn from_file(path: &Path) -> Option<Self> {
        match File::open(path) {
            Ok(mut file) => {
                let mut magic_numbers = [0u8; 4];
                let read_success = file.read(&mut magic_numbers);

                match read_success {
                    Ok(count) => {
                        if magic_numbers.len() != count {
                            println!("Read insufficient bytes to determine type of {:?}", path);
                            return None;
                        }

                        // TODO JPG: Other magic numbers
                        if magic_numbers[..3] == [255, 216, 255] {
                            match FlirThermogram::from_file(path) {
                                Some(flir) => return Some(Thermogram::Flir(flir)),
                                _ => return None,
                            }
                        }

                        let tiff = &magic_numbers[..4];
                        if tiff == [73, 73, 42, 0] || tiff == [77, 77, 0, 42] {
                            match TiffThermogram::from_file(path) {
                                Some(tiff) => return Some(Thermogram::Tiff(tiff)),
                                _ => return None,
                            }
                        }

                        println!("Thermogram format not recognized: {:x?}=={:?}", magic_numbers, magic_numbers);
                        return None;
                    }
                    _ => {
                        println!("Failed reading file {:?}", path);
                        return None;
                    }
                }
            }
            _ => {
                println!("Failed opening file {:?}", path);
                return None;
            }
        }
    }
}

impl ThermogramTrait for Thermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        match self {
            Thermogram::Flir(t) => t.thermal(),
            Thermogram::Tiff(t) => t.thermal(),
        }
    }

    fn optical(&self) -> Option<&Array<u8, Ix3>> {
        match self {
            Thermogram::Flir(t) => t.optical(),
            Thermogram::Tiff(t) => t.optical(),
        }
    }

    fn identifier(&self) -> String {
        match self {
            Thermogram::Flir(t) => t.identifier(),
            Thermogram::Tiff(t) => t.identifier(),
        }
    }
}
