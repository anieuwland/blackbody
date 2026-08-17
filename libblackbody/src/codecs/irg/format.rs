use std::path::PathBuf;

use binrw::{NullString, prelude::*};

use crate::ThermVec;

#[derive(Clone, Debug)]
pub struct IrgThermogram {
    pub file_path: PathBuf,
    pub thermal: ThermVec,
    pub raw_data: RawIrgData,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BinRead)]
pub enum IrgMagic {
    #[br(magic = b"\xba\xab")]
    Baab,
    #[br(magic = b"\xca\xac")]
    Caac,
    #[br(magic = b"\x04\xa0")]
    O4ao,
}

impl IrgMagic {
    /// Divides the raw thermal values into kelvin.
    pub fn divider(self, divider_flag: u8) -> f32 {
        match self {
            Self::Caac if divider_flag != 1 => 16.0,
            _ => 10.0,
        }
    }

    pub fn has_tail(self) -> bool {
        self != Self::O4ao
    }
}

#[derive(Clone, Debug, BinRead)]
#[br(little, assert(!magic.has_tail() || tail == [0xac, 0xca]))]
pub struct RawIrgData {
    pub magic: IrgMagic,
    pub header_length: u16,
    pub grayscale_length: u32,
    pub grayscale_height: u16,
    pub grayscale_width: u16,
    pub divider_flag: u8,
    pub thermal_length: u32,
    pub thermal_height: u16,
    pub thermal_width: u16,
    #[br(pad_before = 1)]
    pub visual_length: u32,
    pub visual_height: u16,
    pub visual_width: u16,
    // 10.000 == 1.0
    pub emissivity: u32,
    /// In 10 millikelvin
    pub reflected_temperature: u32,
    /// In 10 millikelvin
    pub ambient_temperature: u32,
    /// In meters, but unclear in what unit
    pub object_distance: u32,
    #[br(pad_before = 4)]
    /// in 10.000 == 1.0
    pub transmissivity: u32,
    #[br(pad_before = 20)]
    /// The user's preferred display unit. 0 is celsius, 1 is kelvin, 2 is fahrenheit
    pub display_unit: u8,
    #[br(pad_before = 51)]
    pub tail: [u8; 2],
    #[br(pad_before = grayscale_length)]
    #[br(count = thermal_width as u32 * thermal_height as u32)]
    // thermal_length does not work for a vevor file
    pub thermal: Vec<u16>,
    #[br(args { variant: magic })]
    pub appendix: IrgAppendix,
}

#[derive(Clone, Debug, BinRead)]
#[br(import { variant: IrgMagic })]
pub enum IrgAppendix {
    #[br(pre_assert(variant == IrgMagic::O4ao))]
    Roi(NullString),
    #[br(pre_assert(variant != IrgMagic::O4ao))]
    Jpeg(#[br(parse_with = binrw::helpers::until_eof)] Vec<u8>), // visual_length does not work for a vevor file
}

impl IrgAppendix {
    pub fn jpeg(&self) -> Option<&[u8]> {
        match self {
            Self::Jpeg(bytes) if !bytes.is_empty() => Some(bytes.as_slice()),
            _ => None,
        }
    }
}
