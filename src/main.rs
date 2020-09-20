//extern crate image;
//use image::{GenericImage, GenericImageView, ImageBuffer, RgbImage};

use std::env;
use std::path::Path;

mod thermograms;
mod gtkui;
use crate::thermograms::*;
use gtkui::app_window::*;

use gio::prelude::*;
use gtk::*;

pub fn main() {
    // Access CLI arg
    let _args: Vec<String> = env::args().collect();
    //println!("{:?}", &args[1]);
    //let fp = Path::new(&args[1]);
    let fp = Path::new("/home/anieuwland/Afbeeldingen/FLIR9139.jpg");

    // Load thermogram and render to pixbuf
    let thermogram = load_thermogram(&fp);

    // Load application
    let application = Application::new(
            Some("eu.nimmerfort.fikkie"),
            Default::default()
        )
        .expect("failed to initialize GTK application");
    AppState::new(&application, thermogram);
    application.run(&[]);
}

fn load_thermogram(fp: &Path) -> FlirThermogram {
    let thermogram = FlirThermogram::new_from_path(fp).unwrap();
    println!();
    println!("Identifier: {:?}", thermogram.identifier());
    println!("Thermal shape: {:?}", thermogram.thermal_shape());
    let thermal = thermogram.thermal();
    println!(
        "Stats: min: {:?}; avg: {:?}; max: {:?}",
        thermogram.min_temp(),
        thermal.sum() / thermal.len() as f32,
        thermogram.max_temp(),
    );
    println!("{:?}\n", thermogram);

    thermogram
}

// use gettextrs::*;
// use gio::prelude::*;
// use gtk::prelude::*;

// mod config;
// mod window;
// use crate::window::Window;

// fn main() {
//     gtk::init().unwrap_or_else(|_| panic!("Failed to initialize GTK."));

//     setlocale(LocaleCategory::LcAll, "");
//     bindtextdomain("blackbody", config::LOCALEDIR);
//     textdomain("blackbody");

//     let res = gio::Resource::load(config::PKGDATADIR.to_owned() + "/blackbody.gresource")
//         .expect("Could not load resources");
//     gio::resources_register(&res);

//     let app = gtk::Application::new(Some("eu.nimmerfort.blackbody"), Default::default()).unwrap();
//     app.connect_activate(move |app| {
//         let window = Window::new();

//         window.widget.set_application(Some(app));
//         app.add_window(&window.widget);
//         window.widget.present();
//     });

//     let ret = app.run(&std::env::args().collect::<Vec<_>>());
//     std::process::exit(ret);
// }
