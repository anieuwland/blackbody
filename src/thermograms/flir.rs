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
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::collections::HashMap;

use ndarray::*;
use byteorder::{BigEndian, ReadBytesExt};

use super::thermogram::Thermogram;


#[derive(Debug, Clone)]
pub struct FlirThermogram {
    thermal: Array<f32, Ix2>,
    file_path: PathBuf,
}

impl FlirThermogram {
    pub fn new_from_path(file_path: &Path) -> Option<FlirThermogram> {
        println!("Reading {:?}", file_path);
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
    stream.read(&mut magic_bytes)?;
    println!("MAGIC BYTES {:?}", magic_bytes);

    let app1 = extract_flir_app1(stream)?;
    println!("APP1 LENGTH! {:?}", app1.len());

    Ok(arr2(&[[1.,2.,3.], [4.,5.,6.]]))
}

fn extract_flir_app1(file: &mut File) -> Result<Vec<u8>, io::Error> {
    // TODO rewrite stream variable to actually be a stream, not a File
    // TODO handle unwrap
    // let mut chunks_count: Option<u8> = None;

    let mut chunks = HashMap::new();
    let mut bytes: Vec<u8> = Vec::with_capacity(file.metadata().unwrap().len() as usize);
    file.read_to_end(&mut bytes)?;
    let mut flir_app1_bytes = Vec::new();

    let segment_sep = b'\xff';
    for (idx, byte) in bytes.iter().enumerate() {
        if byte != &segment_sep { continue }

        let o_parsed_chunk = parse_flir_chunk(&bytes[idx + 1..]);
        match o_parsed_chunk {
            Ok((chunk_num, chunk_bytes)) => {
                if chunks.get(&chunk_num) != None {  // Check if chunk already exists
                    // FIXME Using io::Error throught this function
                    return Err(io::Error::new(io::ErrorKind::Other, "Chunk exists"));
                };

                chunks.insert(chunk_num, true);  // register as previously found
                flir_app1_bytes.extend(chunk_bytes);  // create app1 bytes string

                // if Some(chunk_num) == chunks_count {
                //     break;
                // };
            }
            _ => continue,
        }
    }

    // if chunks_count == None || chunks_count == Some(0) {
    //     return Err(io::Error::new(io::ErrorKind::Other, "No data"));
    // };

    Ok(flir_app1_bytes)
}


fn parse_flir_chunk(bytes: &[u8]) -> Result<(u8, &[u8]), std::io::Error> {
    // TODO handle inconsistencies between array length and read bytes length
    let app1_marker = b"\xe1";
    let magic_flir = b"FLIR\x00";
    let chunk_metadata_length = 11;  //1+2+5+1+1+1

    let mut potential_app1_marker = [0; 1];
    let mut chunk_length_buf = [0; 2];
    let mut potential_magic_flir = [0; 5];

    let mut stream = bytes;
    stream.read(&mut potential_app1_marker)?;
    stream.read(&mut chunk_length_buf)?;
    stream.read(&mut potential_magic_flir)?;

    if potential_app1_marker != *app1_marker || potential_magic_flir != *magic_flir {
        return Err(io::Error::new(io::ErrorKind::Other, "Chunk header invalid"));
    }

    // Read a single byte to skip. We don't care about the data in there
    stream.read(&mut potential_app1_marker)?;

    // Read chunk details: which chunk it is, how many there are in total and
    // check whether that matches with what we know so far
    let mut chunk_num = [0; 1];
    let mut chunks_count = [0; 1];
    stream.read(&mut chunk_num)?;
    stream.read(&mut chunks_count)?;  // TODO use

    // TODO keep track of chunks_count
    // let mut o_chunks_count = Some(chunks_count[0]);
    // if _chunks_count == &mut None {
    //     println!("No chunk count known!");
    //     _chunks_count = &mut o_chunks_count;
    // }
    // else if _chunks_count != &mut o_chunks_count {
    //     return Err(io::Error::new(io::ErrorKind::Other, "Inconsistent total chunks count"));
    // }

    // let mut chunk_bytes = Vec::with_capacity(chunk_length as usize);
    let chunk_length = chunk_length_buf.as_ref().read_u16::<BigEndian>().unwrap() - chunk_metadata_length;
    let chunk_bytes = &stream[..chunk_length.into()];
    println!("READ CHUNK NO {:?}/{:?} OF LENGTH {:?} =? {:?}",
        chunk_num[0], chunks_count[0], chunk_length as usize, chunk_bytes.len());

    return Ok((chunk_num[0], chunk_bytes))
}
