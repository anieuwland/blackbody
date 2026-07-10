#![windows_subsystem = "windows"]

mod config;
mod gtkui;

use gettextrs::*;
use glib::ExitCode;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::app_window::AppState;

pub fn main() -> ExitCode {
    let o_thermogram_path = std::env::args().nth(1);
    let o_thermogram_path = o_thermogram_path.as_ref().map(AsRef::as_ref);

    init_env();

    let application = adw::Application::new(Some("eu.nimmerfort.blackbody"), Default::default());
    let state = AppState::new(&application, None);
    state.borrow_mut().set_thermogram_from_path(o_thermogram_path);
    application.run()
}

fn init_env() {
    // Enable localization
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain("blackbody", config::LOCALEDIR);
    textdomain("blackbody");

    // Load and register resource bundle
    let res = {
        let pkg_dir = std::path::Path::new(config::PKGDATADIR);
        let res_path = pkg_dir.join("blackbody.gresource");
        gio::Resource::load(res_path)
    }
    .or_else(|_| {
        let exe_path = std::env::current_exe().expect("Can't determine executable path");
        let exe_dir = exe_path.parent().expect("Can't determine executable's directory");
        let res_path = exe_dir.join("blackbody.gresource");
        gio::Resource::load(res_path)
    })
    .or_else(|_| {
        let exe_path = std::env::current_exe().expect("Can't determine executable path");
        let exe_dir = exe_path.parent().expect("Can't determine executable's directory");
        let res_path = exe_dir.join("resources").join("blackbody.gresource");
        gio::Resource::load(res_path)
    })
    .expect("Could find resource bundle");
    gio::resources_register(&res);
}
