//! Display units for temperatures. Thermal data is carried as
//! `uom::si::f32::ThermodynamicTemperature` throughout the app; conversion to
//! a bare number happens only when formatting for the screen or parsing user
//! input.

use uom::si::f32::ThermodynamicTemperature;
use uom::si::thermodynamic_temperature::{degree_celsius, degree_fahrenheit, kelvin};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
    Kelvin,
}

impl TempUnit {
    /// GSettings key value / action target ↔ unit. Unknown strings fall back
    /// to Celsius rather than erroring: the value comes from user config.
    pub fn from_key(key: &str) -> TempUnit {
        match key {
            "fahrenheit" => TempUnit::Fahrenheit,
            "kelvin" => TempUnit::Kelvin,
            _ => TempUnit::Celsius,
        }
    }

    /// The temperature's numeric value in this display unit.
    pub fn convert(self, temp: ThermodynamicTemperature) -> f32 {
        match self {
            TempUnit::Celsius => temp.get::<degree_celsius>(),
            TempUnit::Fahrenheit => temp.get::<degree_fahrenheit>(),
            TempUnit::Kelvin => temp.get::<kelvin>(),
        }
    }

    /// Inverse of `convert`: a value entered in this display unit.
    pub fn to_temperature(self, value: f32) -> ThermodynamicTemperature {
        match self {
            TempUnit::Celsius => ThermodynamicTemperature::new::<degree_celsius>(value),
            TempUnit::Fahrenheit => ThermodynamicTemperature::new::<degree_fahrenheit>(value),
            TempUnit::Kelvin => ThermodynamicTemperature::new::<kelvin>(value),
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            TempUnit::Celsius => "°C",
            TempUnit::Fahrenheit => "°F",
            TempUnit::Kelvin => "K",
        }
    }

    /// "23.5 °C" / "74.3 °F" / "296.6 K"
    pub fn format(self, temp: ThermodynamicTemperature) -> String {
        format!("{:.1} {}", self.convert(temp), self.suffix())
    }
}

#[cfg(test)]
mod tests {
    use super::TempUnit;
    use uom::si::f32::ThermodynamicTemperature;
    use uom::si::thermodynamic_temperature::degree_celsius;

    #[test]
    fn conversions() {
        let t = ThermodynamicTemperature::new::<degree_celsius>(20.0);
        assert_eq!(TempUnit::Celsius.format(t), "20.0 °C");
        assert_eq!(TempUnit::Fahrenheit.format(t), "68.0 °F");
        assert_eq!(TempUnit::Kelvin.format(t), "293.1 K");
        assert_eq!(TempUnit::from_key("fahrenheit"), TempUnit::Fahrenheit);
        assert_eq!(TempUnit::from_key("nonsense"), TempUnit::Celsius);
    }

    #[test]
    fn to_temperature_roundtrips() {
        for unit in [TempUnit::Celsius, TempUnit::Fahrenheit, TempUnit::Kelvin] {
            let t = unit.to_temperature(21.5);
            assert!((unit.convert(t) - 21.5).abs() < 1e-3);
        }
    }
}
