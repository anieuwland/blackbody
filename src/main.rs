//extern crate image;
//use image::{GenericImage, GenericImageView, ImageBuffer, RgbImage};

mod gtkui;
use gtkui::app_window::*;

use gio::prelude::*;
use gtk::*;

pub fn main() {
    // Load application
    let application = Application::new(Some("eu.nimmerfort.fikkie"), Default::default())
        .expect("failed to initialize GTK application");
    AppState::new(&application, None);
    application.run(&[]);
}
