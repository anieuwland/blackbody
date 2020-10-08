use std::io::Read;
use std::io::Seek;


#[derive(Debug)]
#[derive(BinRead)]
struct FlirRawData {
    #[br(pad_before = 26)]
    #[br(parse_with = raw_thermal_parser)]
    raw_thermal_image: Vec<u8>,
}

fn raw_thermal_parser<R>(reader: &mut R, ro: &ReadOptions, _: ()) -> BinResult<Vec<u8>>
where R: Read + Seek {
    let mut vec = Vec::new();
    Ok(vec)
}
