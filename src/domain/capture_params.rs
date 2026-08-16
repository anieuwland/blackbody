use gettextrs::gettext;
use libblackbody::{Thermogram, ThermogramTrait};
use uom::si::length::meter;

use crate::domain::units::TempUnit;

/// The capture parameters the thermogram records, as label/value pairs for the sidebar.
pub fn get_make_dependent_params(
    thermogram: &Thermogram,
    unit: &TempUnit,
) -> Vec<(String, String)> {
    let p = thermogram.capture_parameters();
    let mut params = Vec::new();

    if let Some(v) = p.emissivity {
        params.push((gettext("Emissivity"), format!("{v:.2}")));
    }
    if let Some(v) = p.reflected_temperature {
        params.push((gettext("Reflected temperature"), unit.format(v)));
    }
    if let Some(v) = p.atmospheric_temperature {
        params.push((gettext("Atmospheric temperature"), unit.format(v)));
    }
    if let Some(v) = p.transmissivity {
        params.push((gettext("Transmission"), format!("{v:.2}")));
    }
    if let Some(v) = p.ir_window_temperature {
        params.push((gettext("IR window temperature"), unit.format(v)));
    }
    if let Some(v) = p.relative_humidity {
        // Stored as a fraction, shown as a percentage.
        params.push((gettext("Relative humidity"), format!("{:.0}%", v * 100.0)));
    }
    if let Some(v) = p.distance {
        params.push((gettext("Object distance"), format!("{:.2} m", v.get::<meter>())));
    }

    params
}
