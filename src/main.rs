#![windows_subsystem = "windows"]

mod config;
mod gtkui;

use gettextrs::*;
use glib::ExitCode;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::app_window::AppState;

pub fn main() -> ExitCode {
    let cli_path: Option<std::path::PathBuf> = std::env::args().nth(1).map(Into::into);

    init_env();

    let application = adw::Application::new(
        Some("eu.nimmerfort.blackbody"),
        gio::ApplicationFlags::HANDLES_COMMAND_LINE,
    );
    // Forward command-line invocations to activate; we read args ourselves via std::env::args()
    application.connect_command_line(|app, _| { app.activate(); 0.into() });
    application.connect_activate(move |app| {
        if app.windows().is_empty() {
            let state = AppState::new(app);
            if let Some(path) = &cli_path {
                state.borrow().set_thermogram_from_path(Some(path));
            }
        } else {
            app.windows()[0].present();
        }
    });
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
