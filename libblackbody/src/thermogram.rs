use enum_dispatch::enum_dispatch;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::codecs::hti::decode::is_hti_jpeg;
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
#[enum_dispatch(ThermogramTrait)]
pub enum Thermogram {
    Flir(pub FlirThermogram),
    Tiff(pub TiffThermogram),
    Png(pub PngThermogram),
    Fluke(pub FlukeThermogram),
    Hti(pub HtiThermogram),
}

impl Thermogram {
    /// Tries to recognize the file type and return a `Thermogram`. Fluke `.is2` files are
    /// recognized by their extension; all other formats by their magic number.
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
        // Fluke is2 files do not have consistent and distinctive magic bytes
        let is_is2 = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("is2"));
        if is_is2 {
            return FlukeThermogram::from_file(path)
                .map(Thermogram::Fluke)
                .ok_or_else(|| Error::Decode("corrupt or unsupported Fluke IS2 file".into()));
        }

        let mut file = File::open(path)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;

        // HTI/ToolTop files are JPEGs without a magic number of their own, so they would be
        // claimed by the FLIR branch below. Detection walks the whole trailer layout, which
        // needs the entire file, so only read it back for candidate JPEGs.
        if magic[..3] == [255, 216, 255]
            && std::fs::read(path).is_ok_and(|bytes| is_hti_jpeg(&bytes))
        {
            return HtiThermogram::from_file(path)
                .map(Thermogram::Hti)
                .ok_or_else(|| Error::Decode("corrupt or unsupported HTI file".into()));
        }

        // TODO JPG: Other magic numbers
        // FLIR: either a JPEG containing FLIR APP1 segments, or a raw FFF/AFF stream.
        if magic[..3] == [255, 216, 255] || magic == *b"FFF\0" || magic == *b"AFF\0" {
            return FlirThermogram::from_file(path).map(Thermogram::Flir).ok_or_else(|| {
                Error::Decode("not a FLIR file, or the camera model is unsupported".into())
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

#[cfg(test)]
mod tests {
    use uom::si::thermodynamic_temperature::degree_celsius;

    use crate::thermal::into_temp;

    use super::*;

    #[test]
    fn missing_file_is_io_error() {
        let r = Thermogram::from_file(Path::new("/nonexistent/no.jpg"));
        assert!(matches!(r, Err(Error::Io(_))));
    }

    /// HTI files share the JPEG magic number with FLIR, so `from_file` has to tell them apart
    /// on the trailer layout alone. Both directions matter: an HTI file must not fall through
    /// to the FLIR branch, and a FLIR file must not be claimed by the HTI branch.
    #[test]
    fn jpeg_variants_route_to_the_right_decoder() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/");

        let hti = Thermogram::from_file(&Path::new(dir).join("hti_ht-04d_1.jpg"))
            .expect("HTI test thermogram");
        assert!(matches!(hti, Thermogram::Hti(_)));
        assert_eq!(hti.identifier(), "hti_ht-04d_1.jpg");
        assert_eq!(hti.thermal_shape(), [160, 120]);
        assert!(hti.has_visual());

        let flir = Thermogram::from_file(&Path::new(dir).join("flir_e5_2-pip.jpg"))
            .expect("FLIR test thermogram");
        assert!(matches!(flir, Thermogram::Flir(_)));
    }

    #[test]
    fn unknown_format_reports_magic_number() {
        let path = std::env::temp_dir().join("blackbody_unknown_format_test");
        std::fs::write(&path, b"text file, not a thermogram").unwrap();
        let r = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(r, Err(Error::UnrecognizedFormat(m)) if &m == b"text"));
    }

    /// Pins down enum_dispatch's handling of default trait methods: the generated
    /// enum impl forwards every method to the inner type, so overrides (Flir) and
    /// defaults (Png) must both resolve correctly through the enum.
    #[test]
    fn capability_methods_dispatch_through_enum() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let flir = Thermogram::from_file(Path::new(path)).expect("test thermogram");
        assert!(matches!(flir, Thermogram::Flir(_)));
        assert!(flir.camera_metadata().is_some());
        assert!(flir.has_pip());
        assert_eq!(flir.capture_parameters().emissivity, Some(0.95));

        let path = std::env::temp_dir().join("blackbody_enum_dispatch_pin_test.png");
        image::ImageBuffer::<image::Luma<u16>, _>::from_raw(2, 2, vec![27315u16; 4])
            .unwrap()
            .save(&path)
            .unwrap();
        let png = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        let png = png.expect("16-bit grayscale PNG decodes");
        assert!(matches!(png, Thermogram::Png(_)));
        assert!(png.camera_metadata().is_none());
        assert!(png.measurements().is_empty());
        assert!(!png.has_pip());
        assert!(!png.has_capture_parameters());
        assert!(
            png.picture_in_picture(
                into_temp::<degree_celsius>(0.0),
                into_temp::<degree_celsius>(100.0),
                &palettes::TURBO
            )
            .is_none()
        );
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
