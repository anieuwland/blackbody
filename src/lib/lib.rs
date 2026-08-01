//! This the library [libblackbody](https://bitbucket.org/nimmerwoner/libblackbody/) which intends
//! to be a general purpose thermogram file reading library. Currently it supports TIFF files and
//! some FLIR cameras. It is used by [Blackbody](https://bitbucket.org/nimmerwoner/blackbody/),
//! a simple thermogram viewer.
//!
//! Support for FLIR files is provided by the [flyr](https://docs.rs/flyr/)
//! library. A list of supported cameras can be found in the project repository's
//! README. Tiff files are read making use of image-rs/tiff.
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
//! The file is allowed to be a TIFF, a 16-bit grayscale PNG or a FLIR jpeg.
//!
//! # Interface of a thermogram
//! See [`ThermogramTrait`] for the methods available on a thermogram: extracting thermal data,
//! the embedded optical photo, rendering with a palette, exporting, and measurement statistics.
//!
//! # Issue tracking
//! Issue tracking happens in the [Blackbody repository](https://bitbucket.org/nimmerwoner/blackbody/issues/).

pub mod error;
pub mod flir;
pub mod fluke;
pub mod palettes;
pub mod png;
pub mod thermogram;
pub mod thermogram_trait;
pub mod tiff;

pub use crate::error::Error;
pub use crate::flir::FlirThermogram;
pub use crate::fluke::FlukeThermogram;
pub use flyr::measurement_info::Measurement;
pub use crate::png::PngThermogram;
pub use crate::thermogram::Thermogram;
pub use crate::thermogram_trait::{CaptureParams, TempStats, ThermogramTrait};
pub use crate::tiff::TiffThermogram;
