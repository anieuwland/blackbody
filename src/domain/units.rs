//! Display units for temperatures. All thermal data stays in Celsius
//! internally; conversion happens only when formatting for the screen.

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

    pub fn convert(self, celsius: f32) -> f32 {
        match self {
            TempUnit::Celsius => celsius,
            TempUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
            TempUnit::Kelvin => celsius + 273.15,
        }
    }

    /// Inverse of `convert`: a value entered in this display unit, in celsius.
    pub fn to_celsius(self, value: f32) -> f32 {
        match self {
            TempUnit::Celsius => value,
            TempUnit::Fahrenheit => (value - 32.0) * 5.0 / 9.0,
            TempUnit::Kelvin => value - 273.15,
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
    pub fn format(self, celsius: f32) -> String {
        format!("{:.1} {}", self.convert(celsius), self.suffix())
    }
}

#[cfg(test)]
mod tests {
    use super::TempUnit;

    #[test]
    fn conversions() {
        assert_eq!(TempUnit::Celsius.format(20.0), "20.0 °C");
        assert_eq!(TempUnit::Fahrenheit.format(20.0), "68.0 °F");
        assert_eq!(TempUnit::Kelvin.format(20.0), "293.1 K");
        assert_eq!(TempUnit::from_key("fahrenheit"), TempUnit::Fahrenheit);
        assert_eq!(TempUnit::from_key("nonsense"), TempUnit::Celsius);
    }
}
