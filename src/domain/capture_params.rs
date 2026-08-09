use gettextrs::gettext;
use libblackbody::Thermogram::{self, Flir};

use crate::domain::units::TempUnit;

pub fn get_make_dependent_params(thermogram: &Thermogram, unit: &TempUnit) -> Vec<(String, String)> {
    let mut params = Vec::new();
    match thermogram {
        Flir(src) => {
            let t = &src.thermogram;
            params.push((gettext("Emissivity"), format!("{:.2}", t.camera_info.emissivity)));
            params.push((gettext("Object distance"), format!("{:.2}", t.camera_info.object_distance)));
            params.push((gettext("Reflected temperature"), unit.format(t.camera_info.reflected_apparent_temperature - 273.15)));
            params.push((gettext("Relative humidity"), format!("{:.0}%", t.camera_info.relative_humidity)));
        },
        Thermogram::Fluke(src) => {
            let t = &src.thermogram;
            params.push((gettext("Emissivity"), format!("{:.2}", t.ir_image_info().emissivity())));
            params.push((gettext("Transmission"), format!("{:.2}", t.ir_image_info().transmission())));
            params.push((gettext("Background temperature"), unit.format(t.ir_image_info().background_temperature())));
        },
        Thermogram::Tiff(_) | Thermogram::Png(_) => {},
    };
    return params;
}
