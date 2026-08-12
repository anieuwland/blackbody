//! Conversions of thermogram data to [`ndarray`] types.
//!
//! Available when the `ndarray` feature is enabled (off by default).

use imgref::ImgRef;
use ndarray::{Array2, Array3};
use rgb::RGB8;
use uom::si::f32::ThermodynamicTemperature;

use crate::ThermogramTrait;

/// Extension methods to access thermogram data as `ndarray` arrays.
///
/// Implemented for every [`ThermogramTrait`] type via a blanket impl.
pub trait ThermogramNdarrayExt: ThermogramTrait {
    /// Thermal data as a height × width array.
    fn thermal_ndarray(&self) -> Array2<ThermodynamicTemperature> {
        let thermal = self.thermal().as_ref();
        let pixels: Vec<ThermodynamicTemperature> = thermal.pixels().collect();
        Array2::from_shape_vec((thermal.height(), thermal.width()), pixels)
            .expect("pixel count matches height × width")
    }

    /// The visual light photo as a height × width × 3 RGB array, if present.
    fn visual_ndarray(&self) -> Option<Array3<u8>> {
        self.visual().map(|img| rgb_to_ndarray(img.as_ref()))
    }

    /// Render the thermogram as a height × width × 3 RGB array.
    ///
    /// See [`ThermogramTrait::render`] for the meaning of the arguments.
    fn render_ndarray(
        &self,
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> Array3<u8> {
        rgb_to_ndarray(self.render(min_temp, max_temp, palette).as_ref())
    }
}

impl<T: ThermogramTrait + ?Sized> ThermogramNdarrayExt for T {}

fn rgb_to_ndarray(img: ImgRef<RGB8>) -> Array3<u8> {
    let bytes: Vec<u8> = img.pixels().flat_map(|p| [p.r, p.g, p.b]).collect();
    Array3::from_shape_vec((img.height(), img.width(), 3), bytes)
        .expect("byte count matches height × width × 3")
}

#[cfg(test)]
mod tests {
    use uom::si::thermodynamic_temperature::kelvin;

    use super::ThermogramNdarrayExt;
    use crate::{
        fake::Fake,
        thermal::{into_temp, into_therm_vec},
    };

    #[test]
    fn thermal_ndarray_has_height_width_shape_and_row_major_values() {
        let t = into_therm_vec::<kelvin>(vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0], 3, 2);
        let t = Fake(t);
        let arr = t.thermal_ndarray();
        assert_eq!(arr.shape(), [2, 3]);
        assert_eq!(arr[[0, 1]].get::<kelvin>(), 10.0);
        assert_eq!(arr[[1, 0]].get::<kelvin>(), 30.0);
    }

    #[test]
    fn visual_ndarray_is_none_without_visual() {
        let t = into_therm_vec::<kelvin>(vec![0.0; 4], 2, 2);
        let t = Fake(t);
        assert_eq!(t.visual_ndarray(), None);
    }

    #[test]
    fn render_ndarray_has_height_width_channel_shape() {
        let t = into_therm_vec::<kelvin>(vec![0.0, 10.0, 20.0, 30.0], 2, 2);
        let t = Fake(t);
        let palette = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let render =
            t.render_ndarray(into_temp::<kelvin>(0.0), into_temp::<kelvin>(30.0), &palette);
        assert_eq!(render.shape(), [2, 2, 3]);
        assert_eq!(render[[0, 0, 0]], 0);
        assert_eq!(render[[1, 1, 2]], 255);
    }
}
