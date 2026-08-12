//! The `ThermVec` thermal-data type and helpers to build it from raw `f32` values.

use imgref::{Img, ImgVec};
use uom::{
    Conversion,
    si::{f32::ThermodynamicTemperature, thermodynamic_temperature::Unit},
};

/// Alias for thermal data to annotate it with the temperature unit.
pub type ThermVec = ImgVec<ThermodynamicTemperature>;

/// Convert an iterable of f32 into a `ThermVec`.
///
/// The `U` generic is the unit the iterable's values are already in.
pub fn into_therm_vec<U>(
    thermal: impl IntoIterator<Item = f32>,
    width: usize,
    height: usize,
) -> ThermVec
where
    U: Unit + Conversion<f32, T = f32>,
{
    let therm_vec = thermal.into_iter().map(into_temp::<U>).collect();
    Img::new(therm_vec, width, height)
}

/// Convert a single f32 in unit `U` into a `ThermodynamicTemperature`.
pub fn into_temp<U>(val: f32) -> ThermodynamicTemperature
where
    U: Unit + Conversion<f32, T = f32>,
{
    ThermodynamicTemperature::new::<U>(val)
}

#[cfg(test)]
mod tests {
    use uom::si::thermodynamic_temperature::{centikelvin, degree_celsius, kelvin};

    use super::{into_temp, into_therm_vec};

    #[test]
    fn into_temp_converts_units_to_kelvin() {
        assert_eq!(into_temp::<degree_celsius>(0.0).get::<kelvin>(), 273.15);
        assert_eq!(into_temp::<centikelvin>(27315.0).get::<kelvin>(), 273.15);
        assert_eq!(into_temp::<kelvin>(300.0).get::<kelvin>(), 300.0);
    }

    #[test]
    fn into_therm_vec_keeps_dimensions_and_values() {
        let t = into_therm_vec::<degree_celsius>(vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0], 3, 2);
        assert_eq!((t.width(), t.height()), (3, 2));
        // Row-major: pixel (1, 1) is the fifth value, 40 °C.
        assert_eq!(t[(1usize, 1usize)].get::<degree_celsius>(), 40.0);
    }
}
