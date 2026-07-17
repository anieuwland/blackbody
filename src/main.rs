#![windows_subsystem = "windows"]

mod config;
mod gtkui;

use gettextrs::*;
use glib::ExitCode;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::app_window::AppState;

pub fn main() -> ExitCode {
    println!("Launching Blackbody {}", config::VERSION);
    init_env();

    let application = adw::Application::new(
        Some("eu.nimmerfort.blackbody"),
        gio::ApplicationFlags::HANDLES_COMMAND_LINE,
    );
    // Runs in the primary instance for every invocation, including ones forwarded
    // from a second launch — so a file argument always opens, in a new window.
    application.connect_command_line(|app, cmdline| {
        match cmdline.arguments().get(1) {
            Some(arg) => {
                let state = AppState::new(app);
                state.set_thermogram_from_path(Some(std::path::Path::new(arg)));
            }
            None => app.activate(),
        }
        0.into()
    });
    application.connect_startup(|_| {
        adw::StyleManager::default().set_color_scheme(adw::ColorScheme::PreferDark);
    });
    application.connect_activate(|app| {
        if app.windows().is_empty() {
            AppState::new(app);
        } else {
            app.windows()[0].present();
        }
    });
    application.run()
}

fn init_env() {
    // Enable localization
    setlocale(LocaleCategory::LcAll, "");
    bindtextdomain("blackbody", config::LOCALEDIR).ok();
    textdomain("blackbody").ok();

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
