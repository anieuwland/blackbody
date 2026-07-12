use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use gdk_pixbuf::Pixbuf;
use gio::SimpleAction;
use glib::object::SendWeakRef;
use glib::{Bytes, MainContext};
use gtk4::prelude::*;
use gtk4::{Builder, FileFilter, Picture, Tooltip};
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::palettes::PALETTES;
use libblackbody::{Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

pub struct AppState {
    window: adw::ApplicationWindow,
    image: Picture,
    filter_thermograms: FileFilter,
    filter_all_files: FileFilter,
    thermogram: RefCell<Option<Thermogram>>,
    min_temp: Cell<f32>,
    max_temp: Cell<f32>,
    palette: RefCell<Vec<[f32; 3]>>,
}

impl AppState {
    pub fn new(application: &impl IsA<adw::Application>) -> Rc<RefCell<AppState>> {
        let builder = Builder::from_resource(UI);

        let state = AppState {
            window: builder.object("blackbody_window").unwrap(),
            image: builder.object("viewed_image").unwrap(),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
            thermogram: RefCell::new(None),
            min_temp: Cell::new(0.0),
            max_temp: Cell::new(0.0),
            palette: RefCell::new(PALETTES[0].iter().copied().collect()),
        };

        let this = Rc::new(RefCell::new(state));
        AppState::connect_signals(&this, application);
        this
    }

    pub fn set_thermogram_from_path(&self, path: Option<&Path>) {
        if let Some(path) = path {
            match Thermogram::from_file(path) {
                Some(thermogram) => {
                    self.window.set_title(Some(thermogram.identifier()));
                    self.min_temp.set(thermogram.min_temp());
                    self.max_temp.set(thermogram.max_temp());
                    *self.thermogram.borrow_mut() = Some(thermogram);
                    self.draw_render_threaded();
                }
                None => {
                    let p = path.to_str().unwrap_or("<invalid path>");
                    self.show_error_dialog(&format!(
                        "Failed to open file. The file may be corrupted or the camera \
                         unsupported.\n\nFile: {p}"
                    ));
                }
            }
        }
    }

    fn draw_render_threaded(&self) {
        let min = self.min_temp.get();
        let max = self.max_temp.get();
        let palette: Vec<[f32; 3]> = self.palette.borrow().clone();
        let img_ref = SendWeakRef::from(self.image.downgrade());

        if let Some(thermogram) = self.thermogram.borrow().clone() {
            std::thread::spawn(move || {
                let image = thermogram.render(min, max, &palette);
                if let Some(bytes) = image.as_slice() {
                    let h = image.shape()[0] as i32;
                    let w = image.shape()[1] as i32;
                    let glib_bytes = Bytes::from(bytes);
                    MainContext::default().invoke(move || {
                        let Some(img) = img_ref.upgrade() else { return };
                        let pixbuf = Pixbuf::from_bytes(
                            &glib_bytes,
                            gdk_pixbuf::Colorspace::Rgb,
                            false,
                            8,
                            w,
                            h,
                            3 * w,
                        );
                        let texture = gtk4::gdk::Texture::for_pixbuf(&pixbuf);
                        img.set_paintable(Some(&texture));
                    });
                }
            });
        }
    }

    fn query_tooltip(&self, x: i32, y: i32, tooltip: &Tooltip) -> bool {
        let thermogram = self.thermogram.borrow();
        let Some(thermogram) = thermogram.as_ref() else { return false };

        let shape = thermogram.thermal_shape(); // [height, width]
        let img_w = shape[1] as f64;
        let img_h = shape[0] as f64;

        // Compute the actual painted image rect inside the GtkPicture widget
        // (content-fit = contain: image is centred, scaled to fit while keeping aspect ratio)
        let widget_w = self.image.width() as f64;
        let widget_h = self.image.height() as f64;
        let scale = (widget_w / img_w).min(widget_h / img_h);
        let painted_w = img_w * scale;
        let painted_h = img_h * scale;
        let offset_x = (widget_w - painted_w) / 2.0;
        let offset_y = (widget_h - painted_h) / 2.0;

        // Map widget coordinates to image coordinates
        let ix = ((x as f64 - offset_x) / scale) as usize;
        let iy = ((y as f64 - offset_y) / scale) as usize;

        if ix >= shape[1] || iy >= shape[0] {
            return false;
        }

        let temp = thermogram.thermal()[[iy, ix]];
        tooltip.set_text(Some(&format!("{:.1} °C", temp)));
        true
    }

    fn show_open_dialog(this: &Rc<RefCell<Self>>) {
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&this.borrow().filter_thermograms);
        filters.append(&this.borrow().filter_all_files);
        let dialog = gtk4::FileDialog::builder()
            .title("Open thermogram")
            .filters(&filters)
            .build();
        let window = this.borrow().window.clone();
        let that = this.clone();
        dialog.open(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                that.borrow().set_thermogram_from_path(file.path().as_deref());
            }
        });
    }

    fn show_error_dialog(&self, msg: &str) {
        let dialog = adw::AlertDialog::new(Some("Could not open file"), Some(msg));
        dialog.add_response("close", "Close");
        dialog.present(Some(&self.window));
    }

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &impl IsA<adw::Application>) {
        let application = application.as_ref();
        {
            let that = this.clone();
            application.connect_activate(move |app| {
                app.add_window(&that.borrow().window);
                that.borrow().window.present();
            });
        }
        {
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| Self::show_open_dialog(&that));
            application.add_action(&open);
        }
        {
            let that = this.clone();
            this.borrow().image.set_has_tooltip(true);
            this.borrow().image.connect_query_tooltip(move |_, x, y, _, tooltip| {
                that.borrow().query_tooltip(x, y, tooltip)
            });
        }
    }
}
