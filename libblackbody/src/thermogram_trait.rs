use std::cmp::{Ordering, PartialOrd};
use std::fs::File;
use std::path::PathBuf;

use enum_dispatch::enum_dispatch;
use flyr::camera_metadata::CameraMetadata;
use image::{save_buffer, ColorType};
use ndarray::*;
use tiff::encoder::*;

use crate::palettes;
use crate::{
    Error, FlirThermogram, FlukeThermogram, Measurement, PngThermogram, Thermogram, TiffThermogram,
};

/// All supported thermogram formats implement this trait.
#[enum_dispatch]
pub trait ThermogramTrait {
    /// Returns a reference to the 2D array of thermal data in celsius.
    fn thermal(&self) -> &Array<f32, Ix2>;

    /// Returns reference to the raw RGB values of the thermogram's corresponding
    /// visual light photo, if present. Otherwise `None`.
    fn visual(&self) -> Option<Array<u8, Ix3>>;

    /// Provide the identifier for this thermogram, which is typically the file path. It can also be
    /// a randomly generated uuid or similar, however, if there is no path associated with the data.
    fn identifier(&self) -> &str;

    /// Returns the file path, or `None` if not a file.
    fn path(&self) -> Option<&PathBuf>;

    /// Returns the palette this thermogram was originally rendered with, if available.
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        // Override in implementing format if available.
        None
    }

    /// Camera metadata (make, model, lens, GPS, …), if present.
    fn camera_metadata(&self) -> Option<&CameraMetadata> {
        // Override in implementing format if available.
        None
    }

    /// Measurements embedded in the file, in thermal-image pixel coordinates.
    fn measurements(&self) -> Vec<Measurement> {
        // Override in implementing format if available.
        Vec::new()
    }

    /// Whether the file has picture-in-picture geometry and an embedded optical image.
    fn has_pip(&self) -> bool {
        // Override in implementing format if available.
        false
    }

    /// Thermal render composited onto the optical image, if the file has PIP geometry.
    fn picture_in_picture(
        &self,
        _min_temp: f32,
        _max_temp: f32,
        _palette: &[[f32; 3]],
    ) -> Option<Array<u8, Ix3>> {
        None
    }

    /// Render the thermogram with the given color palette and using the given minimum and maximum
    /// temperature bounds.
    ///
    /// All values are clipped to be between the minimum and maximum value, then put in one of 256
    /// bins. Each bin is mapped to one of the colors in the palette to render an RGB color value.
    ///
    /// # Arguments
    /// * `min_temp` - The temperature value, and all values below it, that needs to be mapped to
    ///   the first color in the palette.
    /// * `max_temp` - The temperature value, and all values above it, that needs to be mapped to
    ///   the last color in the palette.
    /// * `palette` - A collection of 256 colors to which the 256 bins will be mapped.
    ///
    /// # Returns
    /// A three-dimensional RGB array of u8 values between 0 and 255.
    fn render(&self, min_temp: f32, max_temp: f32, palette: &[[f32; 3]]) -> Array<u8, Ix3> {
        let num_bands = 3;
        let num_shades = palette.len() - 1;
        let map_color = |v: &f32| {
            let idx = match (min_temp.partial_cmp(v), max_temp.partial_cmp(v)) {
                (Some(Ordering::Greater), _) => 0,
                (_, Some(Ordering::Less)) => num_shades,
                (_, _) => ((v - min_temp) / (max_temp - min_temp) * num_shades as f32) as usize,
            };

            let to_u8 = |f| (f * 255.0) as u8;
            let color = [
                // Create color array sized [u8; num_bands]
                to_u8(palette[idx][0]),
                to_u8(palette[idx][1]),
                to_u8(palette[idx][2]),
            ];

            // Create iterator out of the array so we can use this in flat_map
            (0..num_bands).map(move |i| color[i])
        };

        // Convert thermal array into a color array by iterating over all values,
        // converting thermal values to RGB arrays, flattening the result into a
        // single vector of u8s. Lastly we recreate an ndarray with the shape
        // (height, width, num_bands) from this vector.
        let colored_array: Vec<u8> = self.thermal().iter().flat_map(map_color).collect();

        let width = self.thermal().ncols();
        let height = self.thermal().nrows();
        Array::from_shape_vec((height, width, num_bands), colored_array).unwrap()
    }

    /// Render the thermogram using the minimum and maximum thermal value and the
    // `palette::TURBO` palette.
    fn render_defaults(&self) -> Array<u8, Ix3> {
        self.render(self.min_temp(), self.max_temp(), &palettes::TURBO)
    }

    /// Export thermal data to a 16-bit grayscale PNG in centikelvin.
    fn export_thermal_png(&self, path: &PathBuf) -> Result<(), Error> {
        let w = self.thermal_shape()[1] as u32;
        let h = self.thermal_shape()[0] as u32;
        let pixels: Vec<u16> = self
            .thermal()
            .iter()
            .map(|&c| (c * 100.0 + 27315.0).clamp(0.0, 65535.0) as u16)
            .collect();
        image::ImageBuffer::<image::Luma<u16>, _>::from_raw(w, h, pixels)
            .ok_or_else(|| Error::Encode("pixel buffer does not match dimensions".into()))?
            .save(path)
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// Export thermal data to a 32-bit float tiff file.
    ///
    /// # Arguments
    /// `path` - Where to save the thermogram export to. Regardless of the file extension, a tiff
    ///   file is created.
    fn export_thermal(&self, path: &PathBuf) -> Result<(), Error> {
        let thermal = self.thermal().iter().copied().collect::<Vec<f32>>();
        let width = self.thermal_shape()[1] as u32;
        let height = self.thermal_shape()[0] as u32;

        let mut file = File::create(path)?;
        let mut tiff = TiffEncoder::new(&mut file).map_err(|e| Error::Encode(e.to_string()))?;
        tiff.write_image::<colortype::Gray32Float>(width, height, &thermal)
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// Save render to file.
    ///
    /// # Arguments
    /// `path` - Where to save the render to. The image type is extrapolated from the extension.
    /// `min_temp` - The minimum temperature for the render, see `render(..)`.
    /// `max_temp` - The maximum temperature for the render, see `render(..)`.
    /// `palette` - The color palette to render the thermogram with, see `render(..)`.
    fn save_render(
        &self,
        path: PathBuf,
        min_temp: f32,
        max_temp: f32,
        palette: &[[f32; 3]],
    ) -> Result<(), Error> {
        let render = self.render(min_temp, max_temp, palette);
        let width = render.shape()[1] as u32;
        let height = render.shape()[0] as u32;
        let render = render.iter().copied().collect::<Vec<u8>>();

        save_buffer(path, render.as_slice(), width, height, ColorType::Rgb8)
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// Gives the shape of the thermal data, in the order of [height, width].
    fn thermal_shape(&self) -> [usize; 2] {
        let thermal = self.thermal();
        [thermal.nrows(), thermal.ncols()]
    }

    fn has_optical(&self) -> bool {
        self.visual().is_some()
    }

    fn has_palette(&self) -> bool {
        self.palette().is_some()
    }

    /// Returns the lowest temperature in the thermogram, or `f32::MAX` if there is no such value.
    fn min_temp(&self) -> f32 {
        self.thermal().fold(f32::MAX, |acc, elem| acc.min(*elem))
    }

    /// Returns the highest temperature in the thermogram, or `f32::MIN` if there is no such value.
    fn max_temp(&self) -> f32 {
        self.thermal().fold(f32::MIN, |acc, elem| acc.max(*elem))
    }
}

#[cfg(test)]
mod tests {
}
