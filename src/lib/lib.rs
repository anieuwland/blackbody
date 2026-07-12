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
//! let file_path = "/home/user/FLIR0123.jpg";
//! let r_thermogram = Thermogram::from_file(&file_path);
//! match r_thermogram {
//!     None => println!("Failed opening thermogram {:?}", file_path),
//!     Some(thermogram) => {
//!         println!("Successfully opened thermogram {:?}", file_path);
//!         // Do something with `thermogram`
//!         // ...
//!     },
//! }
//! ```
//!
//! The file is allowed to be a TIFF or a FLIR jpeg.
//!
//! # Interface of a thermogram
//! The methods listed below are available and recommended for use. Consult the documentation of the
//! trait for more details on the functions.
//!
//! ```rust
//! pub trait ThermogramTrait {
//!     fn thermal(&self) -> &Array<f32, Ix2>;  // Extract the thermal data
//!     fn optical(&self) -> &Array<u8, Ix3>>;  // Extract embedded photos, if present
//!     fn identifier(&self) -> &str;  // A uniquely identifying string for this thermogram
//!     fn render(&self min_temp: f32, max_temp: f32, palette: [[f32; 3]; 256]) -> Array<u8, Ix3>;  // Thermal data render using the given palette
//!     fn render_defaults(&self) -> Array<u8, Ix3>;  // Thermal data rendered using default settings
//!     fn thermal_shape(&self) -> [usize; 2];  // The [height, width] of the thermal data
//! }
//! ```
//!
//! # Issue tracking
//! Issue tracking happens in the [Blackbody repository](https://bitbucket.org/nimmerwoner/blackbody/issues/).

pub mod flir;
pub mod palettes;
pub mod png;
pub mod thermogram;
pub mod thermogram_trait;
pub mod tiff;

pub use crate::flir::FlirThermogram;
pub use crate::png::PngThermogram;
pub use crate::thermogram::Thermogram;
pub use crate::thermogram_trait::{CaptureParams, ThermogramTrait};
pub use crate::tiff::TiffThermogram;
