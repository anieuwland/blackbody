//! This the library [libblackbody](https://crates.io/crates/libblackbody) which intends
//! to be a general purpose thermogram file reading library. Currently it supports FLIR jpegs,
//! Fluke `.is2` files, TIFF files and 16-bit grayscale PNGs. It is used by
//! [Blackbody](https://github.com/anieuwland/blackbody), a thermogram viewer.
//!
//! [flyr](https://docs.rs/flyr/) and [serendip](https://crates.io/crates/serendip)
//! allow decoding FLIR and Fluke files, respectively. TIFF and PNG files are
//! decoded by the [image](https://crates.io/crates/image) project.
//!
//! # Installation
//! This library is available on [crates.io](https://crates.io/crates/libblackbody).
//! Install by adding it to your Cargo.toml.
//!
//! # Usage
//! Call `Thermogram::from_file` on your file to get a `Thermogram` object. It can be used
//! according to the specification of `ThermogramTrait`.
//!
//! ```rust
//! use libblackbody::Thermogram;
//! use std::path::Path;
//!
//! let file_path = Path::new("/home/user/FLIR0123.jpg");
//! match Thermogram::from_file(file_path) {
//!     Err(e) => println!("Failed opening thermogram {:?}: {}", file_path, e),
//!     Ok(thermogram) => {
//!         println!("Successfully opened thermogram {:?}", file_path);
//!         // Do something with `thermogram`
//!         // ...
//!     },
//! }
//! ```
//!
//! The file is allowed to be a FLIR jpeg, a Fluke `.is2`, a TIFF or a 16-bit grayscale PNG.
//!
//! # Interface of a thermogram
//! See [`ThermogramTrait`] for the methods available on a thermogram: extracting thermal data,
//! the embedded visual photo, camera metadata, measurements and their statistics, rendering
//! with a palette, and exporting. A number of color palettes to render with are provided in
//! the [`palettes`] module.
//!
//! # Issue tracking
//! Issue tracking happens in the [Blackbody repository](https://github.com/anieuwland/blackbody/issues).

pub mod error;
pub mod flir;
pub mod fluke;
pub mod palettes;
pub mod png;
pub mod thermogram;
pub mod thermogram_trait;
pub mod tiff;
pub mod measurements;

pub use crate::error::Error;
pub use crate::flir::FlirThermogram;
pub use crate::fluke::FlukeThermogram;
pub use crate::measurements::Measurement;
pub use crate::png::PngThermogram;
pub use crate::thermogram::Thermogram;
pub use crate::thermogram_trait::{CaptureParams, TempStats, ThermogramTrait};
pub use crate::tiff::TiffThermogram;
