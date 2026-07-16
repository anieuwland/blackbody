use ndarray::*;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use flyr::camera_metadata::CameraMetadata;

use crate::*;

/// The wrapper enum through which most processing of thermograms is recommend to
/// happen. Use `Thermogram::from_file()` to read files.
///
/// The enum itself, and all thermogram formats it wraps, implement [`ThermogramTrait`];
/// consult its documentation for the available methods.
// Boxing the Flir variant to silence large_enum_variant would break the
// published API; the size gap only matters for moves, which are rare here.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub enum Thermogram {
    Flir(FlirThermogram),
    Tiff(TiffThermogram),
    Png(PngThermogram),
}

impl Thermogram {
    /// Tries to recognize the file type based on its magic number and return a `Thermogram`.
    ///
    /// # Arguments
    /// * `path` - A path to a thermogram file.
    ///
    /// # Returns
    /// In case of success an `Ok(Thermogram)`, otherwise an [`Error`] describing whether the
    /// file could not be read, was of an unrecognized format, or failed to decode. A
    /// `Thermogram` implements `ThermogramTrait`, forwarding calls to the wrapped struct.
    ///
    /// # Examples
    /// ```rust
    /// use libblackbody::Thermogram;
    /// use std::path::Path;
    ///
    /// let file_path = Path::new("/home/user/FLIR0123.jpg");
    /// match Thermogram::from_file(file_path) {
    ///     Err(e) => println!("Failed opening thermogram {:?}: {}", file_path, e),
    ///     Ok(thermogram) => {
    ///         println!("Successfully opened thermogram {:?}", file_path);
    ///         // Do something with `thermogram`
    ///         // ...
    ///     },
    /// }
    /// ```
    pub fn from_file(path: &Path) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;

        // TODO JPG: Other magic numbers
        if magic[..3] == [255, 216, 255] {
            return FlirThermogram::from_file(path).map(Thermogram::Flir).ok_or_else(|| {
                Error::Decode("not a FLIR JPEG, or the camera model is unsupported".into())
            });
        }

        if magic == [73, 73, 42, 0] || magic == [77, 77, 0, 42] {
            return TiffThermogram::from_file(path)
                .map(Thermogram::Tiff)
                .ok_or_else(|| Error::Decode("corrupt or unsupported TIFF".into()));
        }

        // PNG: \x89PNG
        if magic == [137, 80, 78, 71] {
            return PngThermogram::from_file(path)
                .map(Thermogram::Png)
                .ok_or_else(|| Error::Decode("not a 16-bit grayscale PNG".into()));
        }

        Err(Error::UnrecognizedFormat(magic))
    }
}

impl Thermogram {
    pub fn capture_params(&self) -> Option<CaptureParams> {
        match self {
            Thermogram::Flir(t) => Some(t.capture_params()),
            Thermogram::Tiff(_) | Thermogram::Png(_) => None,
        }
    }

    pub fn camera_metadata(&self) -> Option<&CameraMetadata> {
        match self {
            Thermogram::Flir(t) => t.camera_metadata(),
            Thermogram::Tiff(_) | Thermogram::Png(_) => None,
        }
    }

    /// Measurement tools embedded in the file, in thermal-image pixel coordinates.
    pub fn measurements(&self) -> &[flyr::measurement_info::Measurement] {
        match self {
            Thermogram::Flir(t) => t.measurements(),
            Thermogram::Tiff(_) | Thermogram::Png(_) => &[],
        }
    }

    pub fn has_pip(&self) -> bool {
        match self {
            Thermogram::Flir(t) => t.has_pip(),
            Thermogram::Tiff(_) | Thermogram::Png(_) => false,
        }
    }

    /// Thermal render composited onto the optical image, if the file has PIP geometry.
    pub fn picture_in_picture(
        &self,
        min_temp: f32,
        max_temp: f32,
        palette: &[[f32; 3]],
    ) -> Option<Array<u8, Ix3>> {
        match self {
            Thermogram::Flir(t) => t.picture_in_picture(min_temp, max_temp, palette),
            Thermogram::Tiff(_) | Thermogram::Png(_) => None,
        }
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
            Thermogram::Png(t) => t.thermal(),
        }
    }

    fn optical(&self) -> Option<Array<u8, Ix3>> {
        match self {
            Thermogram::Flir(t) => t.optical(),
            Thermogram::Tiff(t) => t.optical(),
            Thermogram::Png(t) => t.optical(),
        }
    }

    fn identifier(&self) -> &str {
        match self {
            Thermogram::Flir(t) => t.identifier(),
            Thermogram::Tiff(t) => t.identifier(),
            Thermogram::Png(t) => t.identifier(),
        }
    }

    fn path(&self) -> Option<&PathBuf> {
        match self {
            Thermogram::Flir(t) => t.path(),
            Thermogram::Tiff(t) => t.path(),
            Thermogram::Png(t) => t.path(),
        }
    }

    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        match self {
            Thermogram::Flir(t) => t.palette(),
            Thermogram::Tiff(t) => t.palette(),
            Thermogram::Png(t) => t.palette(),
        }
    }
}

impl From<&Thermogram> for Array<f32, Ix2> {
    fn from(thermogram: &Thermogram) -> Array<f32, Ix2> {
        thermogram.thermal().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_io_error() {
        let r = Thermogram::from_file(Path::new("/nonexistent/no.jpg"));
        assert!(matches!(r, Err(Error::Io(_))));
    }

    #[test]
    fn unknown_format_reports_magic_number() {
        let path = std::env::temp_dir().join("blackbody_unknown_format_test");
        std::fs::write(&path, b"text file, not a thermogram").unwrap();
        let r = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(r, Err(Error::UnrecognizedFormat(m)) if &m == b"text"));
    }

    #[test]
    fn corrupt_tiff_is_decode_error() {
        let path = std::env::temp_dir().join("blackbody_corrupt_enum_test.tif");
        std::fs::write(&path, b"II*\0this is not a valid tiff body").unwrap();
        let r = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(r, Err(Error::Decode(_))));
    }
}
