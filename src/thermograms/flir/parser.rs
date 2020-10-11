// http://vip.sugovica.hu/Sardi/kepnezo/JPEG%20File%20Layout%20and%20Format.htm
// https://en.wikipedia.org/wiki/JPEG#Syntax_and_structure
// http://gvsoft.no-ip.org/exif/exif-explanation.html
// https://dev.exiv2.org/projects/exiv2/wiki/The_Metadata_in_JPEG_files
// https://rdrr.io/cran/Thermimage/man/readflirJPG.html
// https://exiftool.org/TagNames/FLIR.html
// https://github.com/kamadak/exif-rs https://docs.rs/kamadak-exif/0.5.1/exif/
// https://crates.io/crates/implex
// https://github.com/vadixidav/exifsd https://docs.rs/exifsd/0.1.0/exifsd/

use binread::io::Read;
use binread::io::Seek;
use binread::*;
use ndarray::*;

use std::collections::HashMap;
use std::io;
use std::io::Cursor;
use std::io::SeekFrom;
use std::path::Path;

pub fn try_parse_flir(file_path: &Path) -> Result<Array<f32, Ix2>, io::Error> {
    let bytes = std::fs::read(file_path)?;
    read_flir_jpeg_stream(&mut bytes.as_slice())
}

fn read_flir_jpeg_stream(bytes: &[u8]) -> Result<Array<f32, Ix2>, io::Error> {
    let app1 = extract_flir_app1(bytes)?;
    let record_directory = parse_record_directory(&app1.as_slice())?;

    let dir_entries = parse_dir_entries(&app1.as_slice(), &record_directory);
    let o_enum_raw_data = dir_entries.get(&1);
    let o_enum_cam_info = dir_entries.get(&32);
    match (o_enum_raw_data, o_enum_cam_info) {
        (Some(FlirRecordType::RawData(raw_data)), Some(FlirRecordType::CameraInfo(cam_info))) => {
            parse_thermal(raw_data, cam_info)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Parsing thermal data failed",
        ))
    }
}

fn extract_flir_app1(bytes: &[u8]) -> Result<Vec<u8>, io::Error> {
    let mut flir_app1_bytes = Vec::new();

    for (idx, byte) in bytes.into_iter().enumerate() {
        if byte != &b'\xff' {
            continue;
        }

        let mut c = Cursor::new(&bytes[idx..]);
        match c.read_be::<FlirApp1Chunk>() {
            Ok(chunk) => flir_app1_bytes.extend(chunk.data),
            _ => (),
        }
    }

    Ok(flir_app1_bytes)
}

fn parse_record_directory(bytes: &[u8]) -> Result<Vec<FlirRecordEntryMetadata>, io::Error> {
    let mut c = Cursor::new(&bytes);
    let mut record_directory = Vec::with_capacity(10);
    while let Ok(record) = c.read_be::<FlirRecord>() {
        let mut cursor = Cursor::new(&bytes);
        cursor.seek(SeekFrom::Current(record.offset_record as i64))?;

        let capacity = 32usize * record.num_record_entries as usize;
        let mut dir_bytes_buf = vec![0u8; capacity];

        cursor.read(dir_bytes_buf.as_mut_slice())?;
        let mut dir_bytes = Cursor::new(&dir_bytes_buf);
        while let Ok(e_entry_md) = dir_bytes.read_be::<FlirRecordEntryMetadata>() {
            record_directory.push(e_entry_md);
        }
    }

    Ok(record_directory)
}

enum FlirRecordType {
    RawData(FlirRawData),
    CameraInfo(FlirCameraInfo),
}

fn parse_dir_entries(
    bytes: &[u8],
    record_directory: &Vec<FlirRecordEntryMetadata>,
) -> HashMap<u16, FlirRecordType> {
    let mut entries: HashMap<u16, FlirRecordType> = HashMap::new();
    for dir_entry in record_directory.iter() {
        match parse_dir_entry(bytes, dir_entry) {
            Ok(entry) => entries.insert(dir_entry.record_type, entry),
            _ => None,
        };
    }

    entries
}

fn parse_dir_entry(
    bytes: &[u8],
    metadata: &FlirRecordEntryMetadata,
) -> Result<FlirRecordType, io::Error> {
    match metadata.record_type {
        1 => Ok(FlirRecordType::RawData(parse_raw_data(bytes, metadata)?)),
        32 => Ok(FlirRecordType::CameraInfo(parse_camera_info(
            bytes, metadata,
        )?)),
        _ => Err(io::Error::new(io::ErrorKind::NotFound, "Nothing")),
    }
}

fn parse_raw_data(
    bytes: &[u8],
    metadata: &FlirRecordEntryMetadata,
) -> Result<FlirRawData, io::Error> {
    // Array<f32, Ix2>
    let start = metadata.offset as usize;
    let end = start + metadata.length as usize;
    let raw_data_bytes = &bytes[start..end]; // flir_app1_bytes

    // println!("RAW WH {:?}x{:?}  -->  Lengths: {:?} =? {:?} =? {:?}",
    //     raw_data.raw_thermal_image_width,
    //     raw_data.raw_thermal_image_height,
    //     raw_data.raw_thermal_image_width as u64 * raw_data.raw_thermal_image_height as u64,
    //     metadata.length,
    //     raw_data.raw_thermal_image.len(),
    // );
    // println!("IMG: {:?}", image::guess_format(raw_data.raw_thermal_image.as_slice()));
    match Cursor::new(raw_data_bytes).read_be::<FlirRawData>() {
        Ok(raw_data) => Ok(raw_data),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Failed reading FLIR image's raw data")),
    }
}

fn parse_camera_info(
    bytes: &[u8],
    metadata: &FlirRecordEntryMetadata,
) -> Result<FlirCameraInfo, io::Error> {
    let start = metadata.offset as usize;
    let end = metadata.length as usize;
    let camera_info_bytes = &bytes[start..end];


    match Cursor::new(camera_info_bytes).read_be::<FlirCameraInfo>() {
        Ok(camera_info) => Ok(camera_info),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "Failed reading FLIR image's camera info")),
    }
}

fn parse_thermal(
    raw_data: &FlirRawData,
    cam_info: &FlirCameraInfo,
) -> Result<Array<f32, Ix2>, io::Error> {
    let r_thermal_img = image::load_from_memory(raw_data.raw_thermal_image.as_slice());
    match r_thermal_img {
        Ok(thermal_img) => {
            let mut vals = Vec::new();
            for val in thermal_img.as_flat_samples_u16().unwrap().as_slice().iter() {
                vals.push(*val);  // FIXME unwrap in parent expression
            }
            let arr = Array::from(vals);
            let arr = arr.map(|x| (x >> 8) + ((x & 0x00FF) << 8));

            let shape = (
                raw_data.raw_thermal_image_height.into(),
                raw_data.raw_thermal_image_width.into(),
            );
            let arr = arr.into_shape(shape).unwrap(); // FIXME unwrap
            let arr = translate_raw2kelvin(arr, cam_info);

            Ok(arr)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Raw thermal is not a valid image",
        ))
    }
}

fn translate_raw2kelvin(raw: Array<u16, Ix2>, info: &FlirCameraInfo) -> Array<f32, Ix2> {
    // Transmission through window (calibrated)
    let emiss_wind = 1.0 - info.ir_window_transmission;
    let refl_wind = 0.0;

    // Transmission through the air
    let water = info.relative_humidity
        * std::f32::consts::E.powf(
            1.5587 + 0.06939 * (info.atmospheric_temperature - 273.15)
                - 0.00027816 * (info.atmospheric_temperature - 273.15).powf(2.0)
                + 0.00000068455 * (info.atmospheric_temperature - 273.15).powf(3.0),
        );

    let calc_atmos = |alpha: f32, beta: f32| -> f32 {
        let term1 = (info.object_distance / 2.0).sqrt();
        let term2 = alpha + beta * water.sqrt();
        std::f32::consts::E.powf(term1 * term2)
    };

    let atmos1 = calc_atmos(info.atmospheric_trans_alpha1, info.atmospheric_trans_beta1);
    let atmos2 = calc_atmos(info.atmospheric_trans_alpha2, info.atmospheric_trans_beta2);
    let tau1 = info.atmospheric_trans_x * atmos1 + (1.0 - info.atmospheric_trans_x) * atmos2;
    let tau2 = info.atmospheric_trans_x * atmos1 + (1.0 - info.atmospheric_trans_x) * atmos2; // FIXME CHECK

    // Radiance from the environment
    let plancked = |t: f32| -> f32 {
        let planck_tmp =
            info.planck_r2 * (std::f32::consts::E.powf(info.planck_b / t) - info.planck_f);
        info.planck_r1 / planck_tmp - (info.planck_o as f32)
    };

    let raw_refl1 = plancked(info.reflected_apparant_temperature);
    let raw_refl1_attn = (1.0 - info.emissivity) / info.emissivity * raw_refl1;

    let raw_atm1 = plancked(info.atmospheric_temperature);
    let raw_atm1_attn = (1.0 - tau1) / info.emissivity / tau1 * raw_atm1;

    let term3 = info.emissivity * tau1 * info.ir_window_transmission;
    let raw_wind = plancked(info.ir_window_temperature);
    let raw_wind_attn = emiss_wind / term3 * raw_wind;

    let raw_refl2 = plancked(info.reflected_apparant_temperature);
    let raw_refl2_attn = refl_wind / term3 * raw_refl2;

    let raw_atm2 = plancked(info.atmospheric_temperature);
    let raw_atm2_attn = (1.0 - tau2) / term3 / tau2 * raw_atm2;

    let subtraction =
        raw_atm1_attn + raw_atm2_attn + raw_wind_attn + raw_refl1_attn + raw_refl2_attn;

    let raw_obj = raw.mapv(|v| v as f32);
    let mut raw_obj = raw_obj / info.emissivity * tau1 * info.ir_window_transmission * tau2;
    raw_obj -= subtraction;

    // Temperature from radiance
    raw_obj += info.planck_o as f32;
    raw_obj *= info.planck_r2;
    let planck_term = info.planck_r1 / raw_obj + info.planck_f;

    info.planck_b / planck_term.ln()
}

trait Logarithmic {
    fn ln(&self) -> Self;
    fn log(&self, n: f32) -> Self;
    fn log2(&self) -> Self;
    fn log10(&self) -> Self;
}

impl Logarithmic for Array<f32, Ix2> {
    fn ln(&self) -> Self {
        self.mapv(|v| v.ln())
    }

    fn log(&self, base: f32) -> Self {
        self.mapv(|v| v.log(base))
    }

    fn log2(&self) -> Self {
        self.mapv(|v| v.log2())
    }

    fn log10(&self) -> Self {
        self.mapv(|v| v.log10())
    }
}

fn raw_thermal_parser<R: Read + Seek>(
    reader: &mut R,
    _ro: &ReadOptions,
    _: (),
) -> BinResult<Vec<u8>> {
    let mut buf = [0; 1]; // TODO Make buf a larger, more reasonable size and truncate when smaller
    let mut raw_thermal = Vec::new();
    while let Ok(read_length) = reader.read(&mut buf) {
        if read_length != buf.len() {
            break;
        }
        raw_thermal.push(buf[0]);
    }
    Ok(raw_thermal)
}

#[allow(dead_code)]
#[derive(Debug, BinRead)]
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

#[allow(dead_code)]
#[derive(Debug, BinRead)]
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

#[allow(dead_code)]
#[derive(Debug, BinRead)]
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

#[derive(Debug, BinRead)]
#[br(little)]
struct FlirCameraInfo {
    #[br(pad_before = 32)]
    emissivity: f32,
    object_distance: f32,
    reflected_apparant_temperature: f32,
    atmospheric_temperature: f32,
    ir_window_temperature: f32,
    ir_window_transmission: f32,
    #[br(pad_before = 4)]
    relative_humidity: f32,
    #[br(pad_before = 24)]
    planck_r1: f32,
    planck_b: f32,
    planck_f: f32,
    #[br(pad_before = 12)]
    atmospheric_trans_alpha1: f32,
    atmospheric_trans_alpha2: f32,
    atmospheric_trans_beta1: f32,
    atmospheric_trans_beta2: f32,
    atmospheric_trans_x: f32,
    #[br(pad_before = 644)]
    planck_o: i32, // TODO CHECK
    planck_r2: f32,
}

#[allow(dead_code)]
#[derive(Debug, BinRead)]
struct FlirRawData {
    #[br(pad_before = 2)]
    #[br(little)]
    raw_thermal_image_width: u16,
    #[br(little)]
    raw_thermal_image_height: u16,
    raw_thermal_image_type: u16,
    #[br(pad_before = 24)]
    #[br(parse_with = raw_thermal_parser)]
    raw_thermal_image: Vec<u8>,
}
