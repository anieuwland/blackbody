//! The object and environment settings a camera measured temperatures with.

use uom::si::f32::{Length, ThermodynamicTemperature};

/// The capture parameters a thermogram records; `None` where the format stores none.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureParameters {
    pub emissivity: Option<f32>,
    pub reflected_temperature: Option<ThermodynamicTemperature>,
    pub atmospheric_temperature: Option<ThermodynamicTemperature>,
    pub transmissivity: Option<f32>,
    pub ir_window_temperature: Option<ThermodynamicTemperature>,
    pub relative_humidity: Option<f32>,
    pub distance: Option<Length>,
}

impl CaptureParameters {
    /// Whether every field is `None`.
    pub fn is_empty(&self) -> bool {
        *self == CaptureParameters::default()
    }
}

#[cfg(test)]
mod tests {
    use uom::si::length::meter;
    use uom::si::thermodynamic_temperature::kelvin;

    use super::*;

    #[test]
    fn default_is_empty_and_any_field_fills_it() {
        assert!(CaptureParameters::default().is_empty());
        assert!(!CaptureParameters { emissivity: Some(0.95), ..Default::default() }.is_empty());
        assert!(
            !CaptureParameters { distance: Some(Length::new::<meter>(1.0)), ..Default::default() }
                .is_empty()
        );
    }

    #[test]
    fn temperatures_are_unit_agnostic() {
        use uom::si::thermodynamic_temperature::degree_celsius;
        let params = CaptureParameters {
            reflected_temperature: Some(ThermodynamicTemperature::new::<degree_celsius>(20.0)),
            ..Default::default()
        };
        let reflected = params.reflected_temperature.expect("set above");
        assert!((reflected.get::<kelvin>() - 293.15).abs() < 0.01);
    }
}
