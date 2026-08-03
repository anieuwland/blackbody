# libblackbody
This the library [libblackbody](https://crates.io/crates/libblackbody)
which intends to be a general purpose thermogram file reading library. Currently
it supports FLIR jpegs, Fluke `.is2` files, TIFF files and 16-bit grayscale
PNGs. It is used by [Blackbody](https://github.com/anieuwland/blackbody),
a thermogram viewer.

[`flyr`](https://docs.rs/flyr/) and [`serendip`](https://crates.io/crates/serendip) 
allow decoding FLIR and Fluke files, respectively. TIFF and PNG files are
decoded by the [`image`](https://crates.io/crates/image) project.

## Installation
This library is available on [crates.io](https://crates.io/crates/libblackbody).
Install by adding it to your Cargo.toml.

## Usage
Call `Thermogram::from_file` on your file:

```rust
use libblackbody::Thermogram;
use std::path::Path;

let file_path = Path::new("/home/user/FLIR0123.jpg");
match Thermogram::from_file(file_path) {
    Err(e) => println!("Failed opening thermogram {:?}: {}", file_path, e),
    Ok(thermogram) => {
        println!("Successfully opened thermogram {:?}", file_path);
        // Do something with `thermogram`
        // ...
    },
}
```

The file is allowed to be a FLIR jpeg, a Fluke `.is2`, a TIFF or a 16-bit
grayscale PNG.

# Interface of a thermogram
The returned `Thermogram` implements `ThermogramTrait`, which gives access to
the following data. Not every format provides everything; such methods return
an `Option` or an empty collection.

```rust
pub trait ThermogramTrait {
    fn thermal(&self) -> &Array<f32, Ix2>;
    fn visual(&self) -> Option<Array<u8, Ix3>>;
    fn identifier(&self) -> &str;
    fn path(&self) -> Option<&PathBuf>;
    fn thermal_shape(&self) -> [usize; 2];
    fn min_temp(&self) -> f32;
    fn max_temp(&self) -> f32;
    fn palette(&self) -> Option<Vec<[f32; 3]>>;
    fn camera_metadata(&self) -> Option<&CameraMetadata>;
    fn measurements(&self) -> Vec<Measurement>;
    fn measurement_stats(&self, measurement: &Measurement) -> Option<TempStats>;
    fn render(&self, min_temp: f32, max_temp: f32, palette: &[[f32; 3]]) -> Array<u8, Ix3>;
    fn render_defaults(&self) -> Array<u8, Ix3>;
    fn has_pip(&self) -> bool;
    fn picture_in_picture(&self, min_temp: f32, max_temp: f32, palette: &[[f32; 3]]) -> Option<Array<u8, Ix3>>;
    fn save_render(&self, path: PathBuf, min_temp: f32, max_temp: f32, palette: &[[f32; 3]]) -> Result<(), Error>;
    fn export_thermal(&self, path: &PathBuf) -> Result<(), Error>;
    fn export_thermal_png(&self, path: &PathBuf) -> Result<(), Error>;
}
```

A number of color palettes to render with are provided in the `palettes`
module.

# Issue tracking
Issue tracking happens in the [Blackbody repository](https://github.com/anieuwland/blackbody).

The [libblackbody repository](https://github.com/anieuwland/blackbody/tree/main/libblackbody) is a subdirectory of the main Blackbody project's repository.
