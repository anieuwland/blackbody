use enum_dispatch::enum_dispatch;
use std::path::Path;

use crate::codecs::encode_format::EncodeFormat;
use crate::codecs::hti::decode::is_hti_jpeg;
use crate::*;

/// The wrapper enum through which most processing of thermograms is recommend to
/// happen. Use `Thermogram::from_file()` to read files and `Thermogram::from_bytes()`
/// for in-memory data.
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
    Fluke(pub FlukeThermogram),
    Hti(pub HtiThermogram),
    Irg(pub IrgThermogram),
    Png(pub PngThermogram),
    Tiff(pub TiffThermogram),
}

impl Thermogram {
    /// Reads the file and decodes it with [`Thermogram::from_bytes`], associating the path
    /// for `path()` and `identifier()`.
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
        let bytes = std::fs::read(path)?;
        let mut thermogram = Self::from_bytes(&bytes)?;
        thermogram.set_path(path);
        Ok(thermogram)
    }

    /// Tries to recognize the format from the buffer's contents and return a `Thermogram`.
    ///
    /// # Arguments
    /// * `bytes` - The complete contents of a thermogram file.
    ///
    /// # Returns
    /// In case of success an `Ok(Thermogram)`, otherwise an [`Error`] describing whether the
    /// buffer was of an unrecognized format or failed to decode.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if PngThermogram::matches_magic(bytes) {
            return PngThermogram::from_bytes(bytes)
                .map(Thermogram::Png)
                .ok_or_else(|| Error::Decode("not a 16-bit grayscale PNG".into()));
        }

        if TiffThermogram::matches_magic(bytes) {
            return TiffThermogram::from_bytes(bytes)
                .map(Thermogram::Tiff)
                .ok_or_else(|| Error::Decode("corrupt or unsupported TIFF".into()));
        }

        if FlukeThermogram::matches_magic(bytes) {
            return FlukeThermogram::from_bytes(bytes)
                .map(Thermogram::Fluke)
                .ok_or_else(|| Error::Decode("corrupt or unsupported Fluke IS2 file".into()));
        }

        // HTI can only be detected by decoding it as it has no magic bytes of its own
        if FlirThermogram::matches_magic(bytes) && is_hti_jpeg(bytes) {
            return HtiThermogram::from_bytes(bytes)
                .map(Thermogram::Hti)
                .ok_or_else(|| Error::Decode("corrupt or unsupported HTI file".into()));
        }

        if FlirThermogram::matches_magic(bytes) {
            return FlirThermogram::from_bytes(bytes).map(Thermogram::Flir).ok_or_else(|| {
                Error::Decode("not a FLIR file, or the camera model is unsupported".into())
            });
        }

        // IRG checked last because its magic bytes is only 2 long
        if IrgThermogram::matches_magic(bytes)
            && let Some(thermogram) = IrgThermogram::from_bytes(bytes)
        {
            return Ok(Thermogram::Irg(thermogram));
        }

        let mut magic = [0u8; 4];
        let length = bytes.len().min(4);
        magic[..length].copy_from_slice(&bytes[..length]);
        Err(Error::UnrecognizedFormat(magic))
    }

    fn set_path(&mut self, path: &Path) {
        let path = Some(path.to_path_buf());
        match self {
            Thermogram::Flir(t) => t.file_path = path,
            Thermogram::Fluke(t) => t.file_path = path,
            Thermogram::Hti(t) => t.file_path = path,
            Thermogram::Irg(t) => t.file_path = path,
            Thermogram::Png(t) => t.file_path = path,
            Thermogram::Tiff(t) => t.file_path = path,
        }
    }

    /// Writes the current thermogram to file at the given path in the specified format.
    pub fn to_file(&self, path: &Path, format: EncodeFormat) -> Result<(), Error> {
        let bytes = self.encode(format)?;
        std::fs::write(path, bytes).map_err(Error::Io)
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

    #[test]
    fn from_bytes_routes_every_format_and_leaves_no_path() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/");
        let read = |name: &str| std::fs::read(Path::new(dir).join(name)).expect("test thermogram");

        let cases = [
            ("flir_e5_2-pip.jpg", "FLIR jpeg"),
            ("flir_sc660_1.jpg", "FLIR jpeg"),
            ("fluke_ti400_1.is2", "Fluke zip is2"),
            ("hti_ht-04d_1.jpg", "HTI jpeg"),
            ("topdon_tc004_1.irg", "IRG"),
        ];
        for (name, kind) in cases {
            let t = Thermogram::from_bytes(&read(name)).expect(kind);
            let matches = match kind {
                "FLIR jpeg" => matches!(t, Thermogram::Flir(_)),
                "Fluke zip is2" => matches!(t, Thermogram::Fluke(_)),
                "HTI jpeg" => matches!(t, Thermogram::Hti(_)),
                "IRG" => matches!(t, Thermogram::Irg(_)),
                _ => unreachable!(),
            };
            assert!(matches, "{name} should decode as {kind}");
            assert_eq!(t.path(), None, "{name} decoded from bytes must have no path");
            assert_eq!(t.identifier(), "<thermogram>");
        }
    }

    /// IRG routing is content-based, so the extension must not matter in either direction.
    #[test]
    fn renamed_irg_files_decode_by_content() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/");
        let bytes = std::fs::read(Path::new(dir).join("topdon_tc004_1.irg")).unwrap();

        let path = std::env::temp_dir().join("blackbody_renamed_irg_test.dat");
        std::fs::write(&path, &bytes).unwrap();
        let t = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(t, Ok(Thermogram::Irg(_))));

        // A FLIR file with an .irg extension must still decode as FLIR.
        let bytes = std::fs::read(Path::new(dir).join("flir_e5_2-pip.jpg")).unwrap();
        let path = std::env::temp_dir().join("blackbody_mislabeled_flir_test.irg");
        std::fs::write(&path, &bytes).unwrap();
        let t = Thermogram::from_file(&path);
        let _ = std::fs::remove_file(&path);
        assert!(matches!(t, Ok(Thermogram::Flir(_))));
    }

    #[test]
    fn from_file_associates_the_path() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_e5_2-pip.jpg");
        let t = Thermogram::from_file(Path::new(path)).expect("test thermogram");
        assert_eq!(t.path(), Some(&Path::new(path).to_path_buf()));
        assert_eq!(t.identifier(), "flir_e5_2-pip.jpg");
    }

    #[test]
    fn short_buffers_are_unrecognized_not_a_panic() {
        assert!(matches!(Thermogram::from_bytes(&[]), Err(Error::UnrecognizedFormat(_))));
        assert!(matches!(Thermogram::from_bytes(b"II"), Err(Error::UnrecognizedFormat(_))));
    }

    /// A zip archive that is not a Fluke file must fail with a decode error, not get
    /// misreported as an unrecognized format.
    #[test]
    fn non_fluke_zip_is_a_decode_error() {
        let zip = b"PK\x03\x04not really a zip but claimed as one";
        assert!(matches!(Thermogram::from_bytes(zip), Err(Error::Decode(_))));
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
        assert!(flir.has_camera_metadata());
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
        assert!(!png.has_camera_metadata());
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
