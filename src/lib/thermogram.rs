use ndarray::*;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::*;

/// The wrapper enum through which most processing of thermograms is recommend to
/// happen. Use `Thermogram::from_file()` to read files.
///
/// The enum itself, and all thermogram formats it wraps, implement `ThermogramTrait`. Below
/// several of its methods are listed. Consult the documentation of `ThermogramTrait` for
/// more details.
///
/// ```rust
/// pub trait ThermogramTrait {
///     fn thermal(&self) -> &Array<f32, Ix2>;  // Extract the thermal data
///     fn optical(&self) -> &Array<u8, Ix3>>;  // Extract embedded photos, if present
///     fn identifier(&self) -> &str;  // A uniquely identifying string for this thermogram
///     fn render(&self min_temp: f32, max_temp: f32, palette: [[f32; 3]; 256]) -> Array<u8, Ix3>;  // Thermal data render using the given palette
///     fn render_defaults(&self) -> Array<u8, Ix3>;  // Thermal data rendered using the minimum and maximum thermal value and the `palette::TURBO` palette.
///     fn thermal_shape(&self) -> [usize; 2];  // The [height, width] of the thermal data
///     fn normalized_minmax(&self) -> Array<f32, Ix2>;  // Thermal data normalized to lie in the range 0.0..=1.0
/// }
/// ```
#[derive(Clone, Debug)]
pub enum Thermogram {
    Flir(FlirThermogram),
    Tiff(TiffThermogram),
}

impl Thermogram {
    /// Tries to recognize the file type based on its magic number and return a `Thermogram`.
    ///
    /// # Arguments
    /// * `path` - A path to a thermogram file.
    ///
    /// # Returns
    /// In case of success an `Some<Thermogram>`, otherwise `None`. A `Thermogam` implements
    /// `ThermogramTrait`, forwarding them to the wrapped struct.
    ///
    /// # Examples
    /// ```rust
    /// let file_path = "/home/user/FLIR0123.jpg";
    /// let r_thermogram = Thermogram::from_file(&file_path);
    /// match r_thermogram {
    ///     None => println!("Failed opening thermogram {:?}", file_path),
    ///     Some(thermogram) => {
    ///         println!("Successfully opened thermogram {:?}", file_path);
    ///         // Do something with `thermogram`
    ///         // ...
    ///     },
    /// }
    /// ```
    pub fn from_file(path: &Path) -> Option<Self> {
        let mut file = File::open(path).ok()?;
        let mut magic_numbers = [0u8; 4];
        let count = file.read(&mut magic_numbers).ok()?;

        if magic_numbers.len() != count {
            println!("Read insufficient bytes to determine type of {:?}", path);
            return None;
        }

        // TODO JPG: Other magic numbers
        if magic_numbers[..3] == [255, 216, 255] {
            let flir = FlirThermogram::from_file(path)?;
            return Some(Thermogram::Flir(flir));
        }

        let tiff = &magic_numbers[..4];
        if tiff == [73, 73, 42, 0] || tiff == [77, 77, 0, 42] {
            let tiff = TiffThermogram::from_file(path)?;
            return Some(Thermogram::Tiff(tiff));
        }

        println!("Thermogram format not recognized: {:x?}=={:?}", magic_numbers, magic_numbers);
        return None;
    }

    pub fn correct_orientation(
        thermal: &Array<f32, Ix2>,
        orientation: Option<u16>,
    ) -> Array<f32, Ix2> {
        let thermal = match orientation {
            Some(1) => thermal.to_owned(),
            Some(2) => thermal.slice(s![..; 1,..;-1]).to_owned(),
            Some(3) => thermal.slice(s![..;-1,..;-1]).to_owned(),
            Some(4) => thermal.slice(s![..;-1,..; 1]).to_owned(),
            // Some(5) => thermal.reversed_axes().to_owned(),
            // Some(6) => thermal.reversed_axes().slice(s![..; 1,..;-1]).to_owned(),
            // Some(7) => thermal.reversed_axes().slice(s![..;-1,..;-1]).to_owned(),
            // Some(8) => thermal.reversed_axes().slice(s![..;-1,..; 1]).to_owned(),
            _ => thermal.to_owned(),
        };

        thermal
    }

    pub fn orientation(path: &PathBuf) -> Option<u16> {
        let res = rexif::parse_file(path).ok()?;

        for e in res.entries {
            match (e.tag, e.value) {
                (rexif::ExifTag::Orientation, rexif::TagValue::U16(mut u16s)) => return u16s.pop(),
                _ => (),
            };
        }

        None
    }
}

/// The `ThermogramTrait` implemented for the `Thermogram` enum. Method calls are forwarded to the
/// specific format wrapped by the enum. Consult the trait for documentation on the supported
/// methods.
impl ThermogramTrait for Thermogram {
    fn thermal(&self) -> &Array<f32, Ix2> {
        match self {
            Thermogram::Flir(t) => t.thermal(),
            Thermogram::Tiff(t) => t.thermal(),
        }
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
        if let Some(optical) = match self {
            Thermogram::Flir(t) => t.optical(),
            Thermogram::Tiff(t) => t.optical(),
        } {
            let optical = match self.path().and_then(|p| Self::orientation(p)) {
                Some(1) => optical,
                Some(2) => optical.slice(s![..; 1,..;-1,..]).to_owned(),
                Some(3) => optical.slice(s![..;-1,..;-1,..]).to_owned(),
                Some(4) => optical.slice(s![..;-1,..; 1,..]).to_owned(),
                Some(5) => optical.permuted_axes([1, 0, 2]),
                Some(6) => optical.permuted_axes([1, 0, 2]).slice(s![..; 1,..;-1,..]).to_owned(),
                Some(7) => optical.permuted_axes([1, 0, 2]).slice(s![..;-1,..;-1,..]).to_owned(),
                Some(8) => optical.permuted_axes([1, 0, 2]).slice(s![..;-1,..; 1,..]).to_owned(),
                _ => optical,
            };

            Array::from_shape_vec(optical.raw_dim(), optical.iter().cloned().collect()).ok()
        } else {
            None
        }
    }

    fn identifier(&self) -> &str {
        match self {
            Thermogram::Flir(t) => t.identifier(),
            Thermogram::Tiff(t) => t.identifier(),
        }
    }

    fn path(&self) -> Option<&PathBuf> {
        match self {
            Thermogram::Flir(t) => t.path(),
            Thermogram::Tiff(t) => t.path(),
        }
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        match self {
            Thermogram::Flir(t) => t.palette(),
            Thermogram::Tiff(t) => t.palette(),
        }
    }
}

impl From<&Thermogram> for Array<f32, Ix2> {
    fn from(thermogram: &Thermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}
