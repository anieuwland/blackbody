// http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
// https://en.wikipedia.org/wiki/JPEG#Syntax_and_structure
// http://gvsoft.no-ip.org/exif/exif-explanation.html
// https://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files
// https://rdrr.io/cran/Thermimage/man/readflirJPG.html
// https://exiftool.org/TagNames/FLIR.html
// https://github.com/kamadak/exif-rs https://docs.rs/kamadak-exif/0.5.1/exif/
// https://crates.io/crates/implex
// https://github.com/vadixidav/exifsd https://docs.rs/exifsd/0.1.0/exifsd/


use std::io;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use ndarray::*;

use super::thermogram::Thermogram;


#[derive(Debug, Clone)]
pub struct FlirThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

impl FlirThermogram {
    pub fn new_from_path(file_path: &Path) -> Option<FlirThermogram> {
        let thermal = FlirThermogram::read_thermal(file_path).unwrap();

        Some(FlirThermogram {
            thermal: thermal,
            file_path: (*file_path).to_path_buf(),
        })
    }

    fn read_thermal(file_path: &Path) -> Option<Array<f32, Ix2>> {
        let r_thermal = try_read_thermal(file_path);
        match r_thermal {
            Ok(thermal) => Some(thermal),
            _ => None
        }
    }
}

impl Thermogram for FlirThermogram {
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


fn try_read_thermal(file_path: &Path) -> Result<Array<f32, Ix2>, io::Error> {
    let mut stream = File::open(file_path)?;
    read_flir_jpeg_stream(&mut stream)
}


fn read_flir_jpeg_stream(stream: &mut File) -> Result<Array<f32, Ix2>, io::Error> {
    let mut magic_bytes = [0; 2];
    let magic_bytes = stream.read(&mut magic_bytes)?;
    println!("{}", magic_bytes);
    Ok(arr2(&[[1.,2.,3.], [4.,5.,6.]]))
}
