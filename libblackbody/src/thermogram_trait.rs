use std::cmp::{Ordering, PartialOrd};
use std::fs::File;
use std::path::PathBuf;

use enum_dispatch::enum_dispatch;
use image::{ColorType, save_buffer};
use imgref::{Img, ImgVec};
use rgb::{ComponentBytes, RGB8};
use tiff::encoder::*;
use uom::si::f32::ThermodynamicTemperature;
use uom::si::thermodynamic_temperature::{centikelvin, kelvin};

use crate::camera::CameraMetadata;
use crate::capture::CaptureParameters;
use crate::palettes;
use crate::pip::{self, PipGeometry};
use crate::thermal::ThermVec;
use crate::{
    Error, FlirThermogram, FlukeThermogram, HtiThermogram, IrgThermogram, Measurement,
    PngThermogram, Thermogram, TiffThermogram,
};

/// All supported thermogram formats implement this trait.
#[enum_dispatch]
pub trait ThermogramTrait {
    /// Returns a reference to the thermal data in kelvin, as a width × height image.
    fn thermal(&self) -> &ThermVec;

    /// Returns the raw RGB values of the thermogram's corresponding
    /// visual light photo, if present. Otherwise `None`.
    fn visual(&self) -> Option<ImgVec<RGB8>>;

    /// Provide the identifier for this thermogram, which is typically the file path. It can also be
    /// a randomly generated uuid or similar, however, if there is no path associated with the data.
    fn identifier(&self) -> &str {
        self.path().and_then(|p| p.file_name().and_then(|n| n.to_str())).unwrap_or("<thermogram>")
    }

    /// Returns the file path, or `None` if not a file.
    fn path(&self) -> Option<&PathBuf>;

    /// Returns the palette this thermogram was originally rendered with, if available.
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        // Override in implementing format if available.
        None
    }

    /// Camera details (make, model, serial, lens, GPS, …), as far as the format records them.
    fn camera_metadata(&self) -> CameraMetadata {
        // Override in implementing format if available.
        CameraMetadata::default()
    }

    /// The capture parameters the camera measured with, as far as the format records them.
    fn capture_parameters(&self) -> CaptureParameters {
        // Override in implementing format if available.
        CaptureParameters::default()
    }

    /// Measurements embedded in the file, in thermal-image pixel coordinates.
    fn measurements(&self) -> Vec<Measurement> {
        // Override in implementing format if available.
        Vec::new()
    }

    /// The file's embedded picture-in-picture geometry. Implementing this is all a format
    /// needs for PiP support; `picture_in_picture` composites with it.
    fn pip_geometry(&self) -> Option<PipGeometry> {
        // Override in implementing format if available.
        None
    }

    /// Whether the file has picture-in-picture geometry and an embedded visual image.
    fn has_pip(&self) -> bool {
        self.pip_geometry().is_some() && self.has_visual()
    }

    /// Thermal render (see `render` for range and palette semantics) composited onto the visual
    /// light image per `pip_geometry`. The result has the visual light image's dimensions.
    fn picture_in_picture(
        &self,
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> Option<ImgVec<RGB8>> {
        let geometry = self.pip_geometry()?;
        let visual = self.visual()?;
        let render = self.render(min_temp, max_temp, palette);
        pip::composite(&visual, &render, &geometry)
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
    /// An RGB image with channel values between 0 and 255.
    fn render(
        &self,
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> ImgVec<RGB8> {
        let (min_temp, max_temp) = (min_temp.get::<kelvin>(), max_temp.get::<kelvin>());
        let num_shades = palette.len() - 1;
        let map_color = |v: ThermodynamicTemperature| {
            let v = v.get::<kelvin>();
            let idx = match (min_temp.partial_cmp(&v), max_temp.partial_cmp(&v)) {
                (Some(Ordering::Greater), _) => 0,
                (_, Some(Ordering::Less)) => num_shades,
                (_, _) => ((v - min_temp) / (max_temp - min_temp) * num_shades as f32) as usize,
            };

            let to_u8 = |f| (f * 255.0) as u8;
            RGB8::new(to_u8(palette[idx][0]), to_u8(palette[idx][1]), to_u8(palette[idx][2]))
        };

        let thermal = self.thermal();
        let pixels: Vec<RGB8> = thermal.pixels().map(map_color).collect();
        Img::new(pixels, thermal.width(), thermal.height())
    }

    /// Render the thermogram with the `palettes::TURBO` palette, using the file's embedded
    /// render range if available and the minimum and maximum thermal value otherwise.
    fn render_defaults(&self) -> ImgVec<RGB8> {
        let range =
            self.embedded_render_range().unwrap_or_else(|| [self.min_temp(), self.max_temp()]);
        self.render(range[0], range[1], &palettes::TURBO)
    }

    /// Export thermal data to a 16-bit grayscale PNG in centikelvin.
    ///
    /// # Arguments
    /// `path` - Where to save the thermogram export to. Regardless of the file extension, a png
    ///   file is created.
    fn export_thermal_png(&self, path: &PathBuf) -> Result<(), Error> {
        let thermal = self.thermal();
        let width = thermal.width() as u32;
        let height = thermal.height() as u32;
        // Round to the nearest centikelvin, otherwise the values truncate on export
        let pixels: Vec<u16> = thermal
            .pixels()
            .map(|c| c.get::<centikelvin>().round().clamp(0.0, 65535.0) as u16)
            .collect();
        image::ImageBuffer::<image::Luma<u16>, _>::from_raw(width, height, pixels)
            .ok_or_else(|| Error::Encode("pixel buffer does not match dimensions".into()))?
            .save(path)
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// Export thermal data to a 32-bit float tiff file in kelvin.
    ///
    /// # Arguments
    /// `path` - Where to save the thermogram export to. Regardless of the file extension, a tiff
    ///   file is created.
    fn export_thermal(&self, path: &PathBuf) -> Result<(), Error> {
        let thermal = self.thermal();
        let width = self.thermal_shape()[1] as u32;
        let height = self.thermal_shape()[0] as u32;
        let thermal = thermal.pixels().map(|t| t.get::<kelvin>()).collect::<Vec<f32>>();

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
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> Result<(), Error> {
        let render = self.render(min_temp, max_temp, palette);
        let width = render.width() as u32;
        let height = render.height() as u32;

        save_buffer(path, render.buf().as_bytes(), width, height, ColorType::Rgb8)
            .map_err(|e| Error::Encode(e.to_string()))
    }

    /// Gives the shape of the thermal data, in the order of [height, width].
    fn thermal_shape(&self) -> [usize; 2] {
        let thermal = self.thermal();
        [thermal.height(), thermal.width()]
    }

    fn has_visual(&self) -> bool {
        self.visual().is_some()
    }

    fn has_palette(&self) -> bool {
        self.palette().is_some()
    }

    fn has_capture_parameters(&self) -> bool {
        !self.capture_parameters().is_empty()
    }

    fn has_camera_metadata(&self) -> bool {
        !self.camera_metadata().is_empty()
    }

    fn embedded_render_range(&self) -> Option<[ThermodynamicTemperature; 2]> {
        None
    }

    /// Returns the lowest temperature in the thermogram, or `f32::MAX` kelvin if there is no
    /// such value.
    fn min_temp(&self) -> ThermodynamicTemperature {
        let max = ThermodynamicTemperature::new::<kelvin>(f32::MAX);
        self.thermal().pixels().fold(max, |acc, elem| acc.min(elem))
    }

    /// Returns the highest temperature in the thermogram, or `f32::MIN` kelvin if there is no
    /// such value.
    fn max_temp(&self) -> ThermodynamicTemperature {
        let min = ThermodynamicTemperature::new::<kelvin>(f32::MIN);
        self.thermal().pixels().fold(min, |acc, elem| acc.max(elem))
    }
}

#[cfg(test)]
mod tests {
    use rgb::RGB8;
    use uom::si::f32::ThermodynamicTemperature;
    use uom::si::thermodynamic_temperature::kelvin;

    use super::ThermogramTrait;
    use crate::codecs::fake::Fake;
    use crate::thermal::into_therm_vec;

    /// 2×2 image; pixel (x, y) values in kelvin: (0,0)=0, (1,0)=10, (0,1)=20, (1,1)=30.
    fn fake() -> Fake {
        Fake(into_therm_vec::<kelvin>(vec![0.0, 10.0, 20.0, 30.0], 2, 2))
    }

    #[test]
    fn min_max_temp() {
        assert_eq!(
            (fake().min_temp().get::<kelvin>(), fake().max_temp().get::<kelvin>()),
            (0.0, 30.0)
        );
    }

    #[test]
    fn thermal_shape_is_height_width() {
        let t = Fake(into_therm_vec::<kelvin>(vec![0.0; 6], 3, 2));
        assert_eq!(t.thermal_shape(), [2, 3]);
    }

    #[test]
    fn render_maps_range_onto_palette_and_clips() {
        let palette = [[0.0, 0.0, 0.0], [0.5, 0.5, 0.5], [1.0, 1.0, 1.0]];
        let min = ThermodynamicTemperature::new::<kelvin>(10.0);
        let max = ThermodynamicTemperature::new::<kelvin>(20.0);
        let render = fake().render(min, max, &palette);
        assert_eq!([render.width(), render.height()], [2, 2]);
        assert_eq!(render[(0usize, 0usize)], RGB8::new(0, 0, 0)); // 0 clips below min
        assert_eq!(render[(1usize, 0usize)], RGB8::new(0, 0, 0)); // 10 = min → first color
        assert_eq!(render[(0usize, 1usize)], RGB8::new(255, 255, 255)); // 20 = max → last color
        assert_eq!(render[(1usize, 1usize)], RGB8::new(255, 255, 255)); // 30 clips above max
    }

    #[test]
    fn capability_defaults_are_absent() {
        let t = fake();
        assert!(!t.has_visual() && !t.has_pip() && !t.has_palette());
        assert!(t.measurements().is_empty() && t.embedded_render_range().is_none());
        assert!(!t.has_capture_parameters() && t.capture_parameters().is_empty());
        assert!(!t.has_camera_metadata() && t.camera_metadata().is_empty());
    }
}
