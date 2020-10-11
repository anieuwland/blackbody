use ndarray::*;
use std::path::{Path, PathBuf};

use crate::thermograms::ThermogramTrait;
use crate::thermograms::flir::parser::try_parse_flir;

#[derive(Debug, Clone)]
pub struct FlirThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

#[allow(dead_code)]
impl FlirThermogram {
    pub fn from_file(file_path: &Path) -> Option<FlirThermogram> {
        let thermogram = FlirThermogram::read_thermal(file_path);
        thermogram
    }

    fn read_thermal(file_path: &Path) -> Option<FlirThermogram> {
        let r_kelvin = try_parse_flir(file_path);
        match r_kelvin {
            Ok(kelvin) => Some(FlirThermogram {
                thermal: kelvin - 275.15,
                file_path: (*file_path).to_path_buf(),
            }),
            _ => None,
        }
    }

    fn celsius(&self) -> Array<f32, Ix2> {
        self.thermal.clone()
    }

    fn kelvin(&self) -> Array<f32, Ix2> {
        self.thermal.clone() + 273.15
    }
}

impl ThermogramTrait for FlirThermogram {
    fn identifier(&self) -> String {
        // FIXME unwraps
        //self.file_path.file_name().unwrap().to_str().unwrap().to_string();
        let file_name = self.file_path.file_name();
        file_name.unwrap().to_os_string().into_string().unwrap()
    }

    fn thermal(&self) -> &Array<f32, Ix2> {
        &self.thermal
    }

    fn optical(&self) -> Option<&Array<u8, Ix3>> {
        None
    }
}

impl From<&FlirThermogram> for Array<f32, Ix2> {
    fn from(thermogram: &FlirThermogram) -> Array<f32, Ix2> {
        thermogram.celsius()
    }
}

impl From<FlirThermogram> for Array<f32, Ix2> {
    fn from(thermogram: FlirThermogram) -> Array<f32, Ix2> {
        thermogram.celsius()
    }
}
