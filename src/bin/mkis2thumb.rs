//! Thumbnailer for Fluke is2 files. Install by
//!
//! 1. Registering the image/x-fluke-is2 mime type. To do so, place below xml
//!    file in /usr/share/local/packages or ~/.local/share/mime/packages.
//! 2. Executing `update-mime-database <path>` where path is mime directory
//!    you used in the previous step (without the subdir packages).
//! 3. Registering a thumbnailer for is2 files by placing the below thumbnailer
//!    entry in /usr/local/share/thumbnailers/mkis2thumb.thumbnailer or
//!    ~/.local/share/thumbnailers/mkis2thumb.thumbnailer.
//! 4. Building this binary using `cargo build --release`. It should appear in
//!    `target/release/`.
//! 5. Installing it with
//!    `sudo install -m 755 target/release/mkis2thumb /usr/local/bin/mkis2thumb`.
//!    Be aware it has to be in `/usr` as the thumbnailer is sandboxed. It does
//!    not work from for example ~/.local/bin.
//! 6. Cleaning the thumbnail cache using `rm -rv ~/.cache/thumbnails/fail`.
//! 7. Revisiting the directory with nautilus after restarting it with
//!    `nautilus -q`.
//!
//! ```xml
//! <!-- ~/.local/share/mime/packages/fluke-is2.xml -->
//! <?xml version="1.0" encoding="UTF-8"?>
//! <mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
//!   <mime-type type="image/x-fluke-is2">
//!     <glob pattern="*.is2"/>
//!     <comment>Fluke Thermal Image File</comment>
//!   </mime-type>
//! </mime-info>
//! ```
//!
//! ```
//! # ~/.local/share/thumbnailers/mkis2thumb.thumbnailer
//! [Thumbnailer Entry]
//! TryExec=/usr/local/bin/mkis2thumb
//! Exec=/usr/local/bin/mkis2thumb %i %o -s %s
//! MimeType=image/x-fluke-is2;
//! ```

use std::process::ExitCode;

use clap::Parser;
use image::{DynamicImage, RgbImage};
use libblackbody::{Thermogram, ThermogramTrait};
use rgb::ComponentBytes;

#[derive(Parser)]
#[command(version, about = "Generate Fluke is2 thermogram thumbnails")]
struct Arguments {
    input: std::path::PathBuf,
    output: std::path::PathBuf,
    /// Maximum thumbnail dimension in pixels
    #[arg(short, default_value_t = 128)]
    size: u32,
}

pub fn main() -> ExitCode {
    // Usage: mkis2thumb <input> <output.png> -s <size>
    let args = Arguments::parse();
    let thermogram = match Thermogram::from_file(&args.input) {
        Ok(thermogram) => thermogram,
        Err(e) => {
            eprintln!("Failed to read {}: {}", args.input.display(), e);
            return ExitCode::FAILURE;
        }
    };

    let (pixels, width, height) = thermogram.render_defaults().into_contiguous_buf();
    let width = width as u32;
    let height = height as u32;
    let pixels = pixels.as_bytes().to_owned();
    let Some(img) = RgbImage::from_raw(width, height, pixels) else {
        eprintln!("Render does not match its reported dimensions");
        return ExitCode::FAILURE;
    };

    let thumbnail = DynamicImage::ImageRgb8(img).thumbnail(args.size, args.size);
    match thumbnail.save(&args.output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Failed to save {}: {}", args.output.display(), e);
            ExitCode::FAILURE
        }
    }
}
