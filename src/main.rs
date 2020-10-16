#![windows_subsystem = "windows"]

mod config;
mod gtkui;

use gettextrs::*;
use gio::prelude::*;
use gtk::*;

use gtkui::app_window::*;


pub fn main() {
    // Ensure environment is correct for the app's theme and resource
    init_env();

    // Load application
    let application = Application::new(Some("eu.nimmerfort.blackbody"), Default::default())
        .expect("failed to initialize GTK application");
    AppState::new(&application, None);
    let ret = application.run(&std::env::args().collect::<Vec<_>>());
    std::process::exit(ret);
}

fn init_env() {
    gtk::init().expect("Couldn't start Blackbody");

    // Enable localization
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain("blackbody", config::LOCALEDIR);
    textdomain("blackbody");

    // Set dark theme for image viewer
    let settings = gtk::Settings::get_default().unwrap();
    let _ = settings.set_property("gtk-application-prefer-dark-theme", &true);

    // Load and register resource carrying the UI file
    let res = {
        let pkg_dir = std::path::Path::new(config::PKGDATADIR);
        let res_path = pkg_dir.join("blackbody.gresource");
        gio::Resource::load(res_path)
    }
    .or_else(|_| {
        let exe_path = std::env::current_exe().expect("Can't determine executable path");
        let exe_dir = exe_path.parent().expect("Can't determine executable's directory");
        let res_path = exe_dir.join("resources").join("blackbody.gresource");
        gio::Resource::load(res_path)
    })
    .expect("Could find resource bundle");
    gio::resources_register(&res);
}
