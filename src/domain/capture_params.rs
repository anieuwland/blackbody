use gettextrs::gettext;
use libblackbody::Thermogram::{self, Flir};
use uom::si::f32::ThermodynamicTemperature;
use uom::si::thermodynamic_temperature::{degree_celsius, kelvin};

use crate::domain::units::TempUnit;

pub fn get_make_dependent_params(
    thermogram: &Thermogram,
    unit: &TempUnit,
) -> Vec<(String, String)> {
    let mut params = Vec::new();
    match thermogram {
        Flir(src) => {
            let t = &src.thermogram;
            params.push((gettext("Emissivity"), format!("{:.2}", t.camera_info.emissivity)));
            params.push((
                gettext("Object distance"),
                format!("{:.2}", t.camera_info.object_distance),
            ));
            // flyr reports the reflected apparent temperature in kelvin.
            let reflected = ThermodynamicTemperature::new::<kelvin>(
                t.camera_info.reflected_apparent_temperature,
            );
            params.push((gettext("Reflected temperature"), unit.format(reflected)));
            params.push((
                gettext("Relative humidity"),
                format!("{:.0}%", t.camera_info.relative_humidity),
            ));
        }
        Thermogram::Fluke(src) => {
            let t = &src.thermogram;
            params.push((gettext("Emissivity"), format!("{:.2}", t.ir_image_info().emissivity())));
            params.push((
                gettext("Transmission"),
                format!("{:.2}", t.ir_image_info().transmission()),
            ));
            // serendip reports the background temperature in celsius.
            let background = ThermodynamicTemperature::new::<degree_celsius>(
                t.ir_image_info().background_temperature(),
            );
            params.push((gettext("Background temperature"), unit.format(background)));
        }
        // HTI's metadata block is not parsed yet, so no capture parameters are available.
        Thermogram::Tiff(_) | Thermogram::Png(_) | Thermogram::Hti(_) => {}
    };
    params
}
