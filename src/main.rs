#![windows_subsystem = "windows"]

//extern crate image;
//use image::{GenericImage, GenericImageView, ImageBuffer, RgbImage};

mod config;
mod gtkui;
use gtkui::app_window::*;

use gio::prelude::*;
use gtk::*;

pub fn main() {
    // Load application
    let application = Application::new(Some("eu.nimmerfort.blackbody"), Default::default())
        .expect("failed to initialize GTK application");
    let ret = match AppState::new(&application, None) {
        Some(_) => application.run(&std::env::args().collect::<Vec<_>>()),
        _ => 1,
    };
    std::process::exit(ret);
}
