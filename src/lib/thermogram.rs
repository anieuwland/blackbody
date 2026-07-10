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
        match self {
            Thermogram::Flir(t) => t.optical(),
            Thermogram::Tiff(t) => t.optical(),
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
