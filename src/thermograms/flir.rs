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
use std::io::Seek;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use ndarray::*;
use byteorder::{BigEndian, ReadBytesExt};
use binread::*;

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

#[derive(BinRead)]
#[br(magic = b"\xff\xe1", assert(&magic_flir == b"FLIR\x00"))]
struct FlirApp1Chunk {
    length: u16,
    magic_flir: [u8; 5],
    skip_byte: u8,
    chunk_idx: u8,
    num_chunks: u8,
    #[br(big, count = length - 10)]
    data: Vec<u8>,
}

#[derive(BinRead)]
#[br(magic = b"FFF\0")]
struct FlirRecord {
    creator: [u8; 16],
    file_format_version: u32,
    offset_record: u32,
    num_record_entries: u32,
    next_free_idx: u32,
    swap_pattern: u16,
    spares: [u16; 7],
    reserved: [u32; 2],
    checksum: u32,
}

#[derive(BinRead)]
struct FlirRecordEntryMetadata {
    record_type: u16,
    record_subtype: u16,
    record_version: u32,
    index_id: u32,
    offset: u32,
    length: u32,
    parent: u32,
    object_number: u32,
    checksum: u32,
}

#[derive(Debug)]
#[derive(BinRead)]
struct FlirCameraInfo {
    //empty_space1: [u8; 32],

    emissivity: f32,
    object_distance: f32,
    reflected_apparant_temperature: f32,
    atmospheric_temperature: f32,
    ir_window_temperature: f32,
    ir_window_transmission: f32,

    empty_space2: [u8; 4],

    relative_humidity: f32,

    empty_space3: [u8; 24],

    planck_r1: f32,
    planck_b: f32,
    planck_f: f32,

    empty_space4: [u8; 12],

    atmospheric_trans_alpha1: f32,
    atmospheric_trans_alpha2: f32,
    atmospheric_trans_beta1: f32,
    atmospheric_trans_beta2: f32,
    atmospheric_trans_x: f32,

    //empty_space5: [u8; 648],

    //planck_o: f32,  // TODO CHECK
    //planck_r2: i32,
}

#[derive(Debug)]
#[derive(BinRead)]
struct FlirRawData {
    #[br(pad_before = 2)]
    #[br(little)]
    raw_thermal_image_width: u16,
    #[br(little)]
    raw_thermal_image_height: u16,
    raw_thermal_image_type: u16,
    #[br(pad_before = 26)]
    #[br(parse_with = raw_thermal_parser)]
    raw_thermal_image: Vec<u8>,
}

fn extract_flir_app1(file: &mut File) -> Result<Vec<u8>, io::Error> {
    // TODO rewrite stream variable to actually be a stream, not a File
    // TODO handle unwrap
    let mut bytes: Vec<u8> = Vec::with_capacity(file.metadata().unwrap().len() as usize);
    file.read_to_end(&mut bytes)?;
    let mut flir_app1_bytes = Vec::new();

    for (idx, byte) in bytes.iter().enumerate() {
        if byte != &b'\xff' { continue }

        let mut c = Cursor::new(&bytes[idx..]);
        match c.read_be::<FlirApp1Chunk>() {
            Ok(chunk) => {
                println!("CHUNK {:?} {:?} {:?}", chunk.length, chunk.chunk_idx, chunk.num_chunks);
                flir_app1_bytes.extend(chunk.data);
            }
            _ => continue,
        }
    }

    // if chunks_count == None || chunks_count == Some(0) {
    //     return Err(io::Error::new(io::ErrorKind::Other, "No data"));
    // };

    let mut c = Cursor::new(&flir_app1_bytes);
    while let Ok(record) = c.read_be::<FlirRecord>() {
        println!("RECORD OFFSET {:?}, NUM ENTRIES {:?}", record.offset_record, record.num_record_entries);
        let mut cursor = Cursor::new(&flir_app1_bytes);
        cursor.seek(SeekFrom::Current(record.offset_record as i64));

        let mut dir_bytes_buf = vec![];
        let mut dir_bytes_take = cursor.take(32u64 * record.num_record_entries as u64);
        dir_bytes_take.read_to_end(&mut dir_bytes_buf);
        let mut dir_bytes = Cursor::new(&dir_bytes_buf);
        while let Ok(e_entry_md) = dir_bytes.read_be::<FlirRecordEntryMetadata>() {
            match e_entry_md.record_type {
                1 => {
                    println!("PROCESSING RAW DATA");
                    let start = e_entry_md.offset as usize;
                    let end = start + e_entry_md.length as usize;
                    let raw_data_bytes = &flir_app1_bytes[start..end];
                    println!("{:?}", raw_data_bytes.len());
                    let e_raw_data = Cursor::new(raw_data_bytes).read_be::<FlirRawData>();
                    match e_raw_data {
                        Ok(raw_data) => {
                            println!("RAW WH {:?}x{:?}  -->  Lengths: {:?} =? {:?} =? {:?}",
                                raw_data.raw_thermal_image_width,
                                raw_data.raw_thermal_image_height,
                                raw_data.raw_thermal_image_width as u64 * raw_data.raw_thermal_image_height as u64,
                                e_entry_md.length,
                                raw_data.raw_thermal_image.len(),
                            );
                        },
                        _ => continue
                    }
                },
                32 => {
                    println!("PROCESSING CAMERA INFO");
                    let start = (e_entry_md.offset + 32) as usize;
                    //let end = e_entry_md.length as usize;
                    //let camera_info_slice = &flir_app1_bytes[start..end];
                    c.seek(SeekFrom::Start(start as u64));
                    let e_camera_info: FlirCameraInfo = c.read_be().unwrap();
                    // println!("{:?}", e_camera_info);
                },
                _ => println!("UNKNOWN RECORD TYPE"),
            }
            println!("RE MD: {:?} {:?} {:?}", e_entry_md.record_type, e_entry_md.offset, e_entry_md.length)
        }
    }

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

fn raw_thermal_parser<R>(reader: &mut R, ro: &ReadOptions, _: ()) -> BinResult<Vec<u8>>
where R: Read + Seek {
    let mut vec = Vec::new();
    Ok(vec)
}
