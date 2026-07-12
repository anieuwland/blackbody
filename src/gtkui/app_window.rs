use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use cairo::LinearGradient;
use gdk_pixbuf::Pixbuf;
use gio::SimpleAction;
use glib::object::SendWeakRef;
use glib::{Bytes, MainContext};
use gtk4::prelude::*;
use gtk4::{Builder, Button, DrawingArea, FileFilter, Label, Picture, Scale, ToggleButton, Tooltip};
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::palettes::PALETTES;
use libblackbody::{Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

pub struct AppState {
    window: adw::ApplicationWindow,
    image: Picture,
    color_bar: DrawingArea,
    range_bar: gtk4::Box,
    min_scale: Scale,
    max_scale: Scale,
    min_label: Label,
    max_label: Label,
    auto_button: Button,
    mode_thermal: ToggleButton,
    mode_optical: ToggleButton,
    mode_pip: ToggleButton,
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
            color_bar: builder.object("color_bar").unwrap(),
            min_scale: builder.object("min_scale").unwrap(),
            max_scale: builder.object("max_scale").unwrap(),
            min_label: builder.object("min_label").unwrap(),
            max_label: builder.object("max_label").unwrap(),
            auto_button: builder.object("auto_button").unwrap(),
            range_bar: builder.object("range_bar").unwrap(),
            mode_thermal: builder.object("mode_thermal").unwrap(),
            mode_optical: builder.object("mode_optical").unwrap(),
            mode_pip: builder.object("mode_pip").unwrap(),
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
                    let min = thermogram.min_temp();
                    let max = thermogram.max_temp();
                    self.min_temp.set(min);
                    self.max_temp.set(max);
                    *self.thermogram.borrow_mut() = Some(thermogram);
                    self.min_scale.set_value(min as f64);
                    self.max_scale.set_value(max as f64);
                    self.min_label.set_text(&format!("{:.1} °C", min));
                    self.max_label.set_text(&format!("{:.1} °C", max));
                    self.auto_button.set_sensitive(true);
                    self.draw_render_threaded();
                    self.color_bar.queue_draw();
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

    fn draw_color_bar(
        context: &cairo::Context,
        width: f64,
        height: f64,
        palette: &[[f32; 3]],
        min_temp: f32,
        max_temp: f32,
    ) {
        let gradient = LinearGradient::new(0.0, 0.0, 0.0, height);
        let step = 1.0 / (palette.len() - 1) as f64;
        for (i, color) in palette.iter().enumerate() {
            // Palette index 0 = min (bottom of bar), last = max (top of bar)
            gradient.add_color_stop_rgb(
                1.0 - i as f64 * step,
                color[0] as f64,
                color[1] as f64,
                color[2] as f64,
            );
        }
        context.rectangle(0.0, 0.0, width, height);
        let _ = context.set_source(&gradient);
        let _ = context.fill();

        // Temperature labels (white, small)
        context.set_source_rgb(1.0, 1.0, 1.0);
        context.set_font_size(10.0);
        let _ = context.move_to(2.0, 12.0);
        let _ = context.show_text(&format!("{:.1}°", max_temp));
        let _ = context.move_to(2.0, height - 4.0);
        let _ = context.show_text(&format!("{:.1}°", min_temp));
    }

    fn draw_render_threaded(&self) {
        let min = self.min_temp.get();
        let max = self.max_temp.get();
        let palette: Vec<[f32; 3]> = self.palette.borrow().clone();
        let thermal_mode = self.is_thermal_mode();
        let img_ref = SendWeakRef::from(self.image.downgrade());

        if let Some(thermogram) = self.thermogram.borrow().clone() {
            std::thread::spawn(move || {
                let image = if !thermal_mode {
                    thermogram.optical().unwrap_or_else(|| thermogram.render(min, max, &palette))
                } else {
                    thermogram.render(min, max, &palette)
                };
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

    fn is_thermal_mode(&self) -> bool {
        self.mode_thermal.is_active()
    }

    fn apply_mode(this: &Rc<RefCell<Self>>, button: &ToggleButton) {
        // Mutual exclusion: activate the clicked button, deactivate others
        {
            let s = this.borrow();
            // Block all signals temporarily to avoid recursion
            for tb in [&s.mode_thermal, &s.mode_optical, &s.mode_pip] {
                tb.set_active(false);
            }
            button.set_active(true);
        }

        let s = this.borrow();
        let is_thermal = s.is_thermal_mode();
        s.color_bar.set_visible(is_thermal);
        s.range_bar.set_visible(is_thermal);

        // Re-render with the appropriate image
        s.draw_render_threaded();
        if is_thermal {
            s.color_bar.queue_draw();
        }
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
        {
            // Mode toggles: T / O / P
            let that = this.clone();
            this.borrow().mode_thermal.connect_toggled(move |btn| {
                if btn.is_active() { Self::apply_mode(&that, btn); }
            });
            let that = this.clone();
            this.borrow().mode_optical.connect_toggled(move |btn| {
                if btn.is_active() { Self::apply_mode(&that, btn); }
            });
        }
        {
            // Colour bar draw function
            let that = this.clone();
            this.borrow().color_bar.set_draw_func(move |_, ctx, _w, _h| {
                let s = that.borrow();
                let w = s.color_bar.width() as f64;
                let h = s.color_bar.height() as f64;
                let palette = s.palette.borrow().clone();
                Self::draw_color_bar(ctx, w, h, &palette, s.min_temp.get(), s.max_temp.get());
            });
        }
        {
            // Min scale: update min_temp, label, re-render
            let that = this.clone();
            this.borrow().min_scale.connect_value_changed(move |scale| {
                let v = scale.value() as f32;
                let s = that.borrow();
                s.min_temp.set(v);
                s.min_label.set_text(&format!("{:.1} °C", v));
                s.draw_render_threaded();
                s.color_bar.queue_draw();
            });
        }
        {
            // Max scale: update max_temp, label, re-render
            let that = this.clone();
            this.borrow().max_scale.connect_value_changed(move |scale| {
                let v = scale.value() as f32;
                let s = that.borrow();
                s.max_temp.set(v);
                s.max_label.set_text(&format!("{:.1} °C", v));
                s.draw_render_threaded();
                s.color_bar.queue_draw();
            });
        }
        {
            // Auto button: snap range to image min/max
            let that = this.clone();
            this.borrow().auto_button.connect_clicked(move |_| {
                let thermogram = that.borrow().thermogram.borrow().clone();
                if let Some(thermogram) = thermogram {
                    let s = that.borrow();
                    s.min_scale.set_value(thermogram.min_temp() as f64);
                    s.max_scale.set_value(thermogram.max_temp() as f64);
                }
            });
        }
    }
}
