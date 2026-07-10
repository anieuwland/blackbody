// https://gtk-rs.org/docs/gtk/struct.DrawingArea.html
// https://www.reddit.com/r/rust/comments/6catf5/drawing_to_a_gtkdrawingarea/
// https://stackoverflow.com/questions/959675/what-is-the-fastest-way-to-draw-an-image-in-gtk
// https://github.com/gtk-rs/examples/blob/master/src/bin/cairotest.rs

// Or: Drawing using OpenGL
// https://github.com/gtk-rs/gdk/issues/81
// https://gtk-rs.org/docs/gtk/struct.GLArea.html
// https://stackoverflow.com/questions/45424802/how-to-embed-an-sdl-surface-into-gtk
// https://www.bassi.io/articles/2015/02/17/using-opengl-with-gtk/

use core::cmp::{max, min};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread;

use gdk_pixbuf::Pixbuf;
use gio::prelude::ActionMapExt;
use gio::{Cancellable, SimpleAction};
use glib::object::{ObjectExt, SendWeakRef};
use glib::{Bytes, MainContext};
use gtk4::prelude::*;
use gtk4::{
    AboutDialog, Application, ApplicationWindow, CheckButton, ComboBoxText, HeaderBar,
    Image, PopoverMenu, Revealer, SpinButton, ToggleButton,
};
use gtk4::{Builder, FileFilter, Tooltip};
use libblackbody::{Thermogram, ThermogramTrait};

use crate::gtkui::imagery_toggles::ImageryToggles;
use crate::gtkui::palettes::PALETTES;
use crate::gtkui::thermometer::Thermometer;

use super::imagery_toggles::ImageryKind;

#[derive(Clone)]
pub struct AppState {
    builder: Builder,

    // Controls
    window: ApplicationWindow,
    headerbar: HeaderBar,
    app_menu: PopoverMenu,
    palette_chooser: ComboBoxText,
    embedded_palette_toggle: ToggleButton,
    image: Image,
    min_spinner: SpinButton,
    max_spinner: SpinButton,
    thermometer_toggler: CheckButton,
    thermometer_revealer: Revealer,
    thermometer: Rc<RefCell<Thermometer>>,
    zoom_spinner: SpinButton,
    about_dialog: AboutDialog,
    filter_thermograms: FileFilter,
    filter_all_files: FileFilter,

    // Tool bar items
    imagery_toggles: Rc<RefCell<ImageryToggles>>,

    // Model members
    thermogram: RefCell<Option<Thermogram>>,
}

impl AppState {
    pub fn new(application: &impl IsA<Application>, thermogram: Option<Thermogram>) -> Rc<RefCell<AppState>> {
        // Create application from builder
        let ui = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";
        let builder = Builder::from_resource(ui);

        let thermometer =
            Thermometer::new(builder.object("thermometer").unwrap(), PALETTES[0].into());

        let ref_thermogram = RefCell::new(None);
        let state = AppState {
            builder: builder.clone(),

            // Application's state struct
            window: builder.object("blackbody_window").unwrap(),
            headerbar: builder.object("headerbar").unwrap(),
            app_menu: builder.object("app_menu").unwrap(),
            palette_chooser: builder.object("palette_chooser").unwrap(),
            embedded_palette_toggle: builder.object("embedded_palette_toggle").unwrap(),
            image: builder.object("viewed_image").unwrap(),
            min_spinner: builder.object("min_temp_spinner").unwrap(),
            max_spinner: builder.object("max_temp_spinner").unwrap(),
            thermometer_toggler: builder.object("thermometer_toggler").unwrap(),
            thermometer_revealer: builder.object("thermometer_revealer").unwrap(),
            thermometer: thermometer,
            zoom_spinner: builder.object("zoom_spinner").unwrap(),
            about_dialog: builder.object("about_dialog").unwrap(),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),

            // Tool bar items
            imagery_toggles: ImageryToggles::from_builder(&builder, &ref_thermogram),

            thermogram: ref_thermogram,
        };

        // Update child widget with application's state
        state.update_thermometer();

        state.filter_thermograms.set_name(Some("Thermograms: *.jpg (FLIR), *.tiff"));
        state.filter_all_files.set_name(Some("All files"));

        // Create an object containing the state that can be used in callbacks
        // and set up those callbacks.
        let this = Rc::new(RefCell::new(state));
        AppState::connect_signals(&this, application);

        // If given, set initial thermogram, then return final constructed app
        this.clone().borrow().set_thermogram(thermogram);
        this
    }

    pub fn set_thermogram_from_path(&self, o_path: Option<&Path>) {
        // Attempt to open the thermogram and show an error dialog if that fails
        let o_thermogram = o_path.and_then(Thermogram::from_file);
        match (&o_path, &o_thermogram) {
            // In case a path is set, but opening fails, show the error dialog
            (Some(path), None) => {
                // Construct the error message and attempt to include the file path in it
                let p = path.to_str().unwrap_or("<invalid file>");
                let mut msg = String::from(
                    "Failed to open file. This could be because the file is (partially) \
                    corrupted or the camera is unsupported. ",
                );

                msg = msg + "\n\nIssue encountered on file: " + p;

                self.show_failure_dialog(msg.as_str());
            }
            _ => (),
        }

        self.set_thermogram(o_thermogram);
        self.draw_render_threaded();
    }

    fn set_thermogram(&self, o_thermogram: Option<Thermogram>) {
        o_thermogram.map(|thermogram| {
            // Update controls
            self.window.set_title(Some(&thermogram.identifier()));
            self.min_spinner.set_value(thermogram.min_temp().into());
            self.max_spinner.set_value(thermogram.max_temp().into());

            // Update thermogram and draw
            self.thermogram.replace(Some(thermogram));
            self.draw_render_threaded();
            self.enable_thermogram_ui();
        });
    }

    fn show_thermogram_chooser(this: &Rc<RefCell<Self>>) {
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
                let path = file.path();
                that.borrow().set_thermogram_from_path(path.as_deref());
            }
        });
    }

    fn show_thermogram_exporter(this: &Rc<RefCell<Self>>) {
        let tiffs = FileFilter::new();
        tiffs.set_name(Some("TIFF"));
        tiffs.add_mime_type("image/tif");
        tiffs.add_mime_type("image/tiff");
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&tiffs);

        let default_name = this.borrow().thermogram.borrow().clone()
            .map(|t| { let mut p = PathBuf::from(t.identifier()); p.set_extension("tiff"); p })
            .unwrap_or_else(|| PathBuf::from("thermogram.tiff"));

        let dialog = gtk4::FileDialog::builder()
            .title("Export thermogram")
            .filters(&filters)
            .initial_name(default_name.file_name().and_then(|n| n.to_str()).unwrap_or("thermogram.tiff"))
            .build();
        let window = this.borrow().window.clone();
        let that = this.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let thermogram = that.borrow().thermogram.borrow().clone();
                    if let Some(thermogram) = thermogram {
                        if thermogram.export_thermal(&path).is_none() {
                            let p = path.to_str().unwrap_or("<invalid path>");
                            that.borrow().show_failure_dialog(&format!("Failed to export to {}", p));
                        }
                    }
                }
            }
        });
    }

    fn show_render_saver(this: &Rc<RefCell<Self>>) {
        let pngs = FileFilter::new();
        pngs.set_name(Some("PNG"));
        pngs.add_mime_type("image/png");
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&pngs);

        let default_name = this.borrow().thermogram.borrow().clone()
            .map(|t| { let mut p = PathBuf::from(t.identifier()); p.set_extension("png"); p })
            .unwrap_or_else(|| PathBuf::from("render.png"));

        let dialog = gtk4::FileDialog::builder()
            .title("Save render")
            .filters(&filters)
            .initial_name(default_name.file_name().and_then(|n| n.to_str()).unwrap_or("render.png"))
            .build();
        let window = this.borrow().window.clone();
        let that = this.clone();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(mut path) = file.path() {
                    path.set_extension("png");
                    let min_temp = that.borrow().minimum_temperature();
                    let max_temp = that.borrow().maximum_temperature();
                    let palette = that.borrow().palette();
                    let thermogram = that.borrow().thermogram.borrow().clone();
                    if let Some(thermogram) = thermogram {
                        if thermogram.save_render(path.clone(), min_temp, max_temp, &palette).is_none() {
                            let p = path.to_str().unwrap_or("<invalid path>");
                            that.borrow().show_failure_dialog(&format!("Failed to save render to {}", p));
                        }
                    }
                }
            }
        });
    }

    fn draw_render_threaded(&self) {
        let min_temp = self.minimum_temperature();
        let max_temp = self.maximum_temperature();
        let zoom = self.zoom() / 100f64;
        let palette = self.palette();
        let imagery_kind = self.imagery_toggles.borrow().kind();
        let img_ref = SendWeakRef::from(self.image.downgrade());

        self.thermogram.clone().into_inner().map(|thermogram| {
            thread::spawn(move || {
                let image = match (imagery_kind, thermogram.optical()) {
                    (ImageryKind::Optical, Some(optical)) => optical.to_owned(),
                    _ => thermogram.render(min_temp, max_temp, &palette).to_owned(),
                };

                if let Some(bytes) = image.as_slice() {
                    let src_height = image.shape()[0];
                    let src_width = image.shape()[1];
                    let tml_shape = thermogram.thermal_shape();
                    let dst_height = (tml_shape[0] as f64 * zoom).round() as usize;
                    let dst_width = (tml_shape[1] as f64 * zoom).round() as usize;
                    let glib_bytes = Bytes::from(bytes);

                    MainContext::default().invoke(move || {
                        let Some(img) = img_ref.upgrade() else { return };
                        let pixbuf = Pixbuf::from_bytes(
                            &glib_bytes,
                            gdk_pixbuf::Colorspace::Rgb,
                            false,
                            8,
                            src_width as i32,
                            src_height as i32,
                            3 * src_width as i32,
                        );
                        let pixbuf = pixbuf.scale_simple(
                            dst_width as i32,
                            dst_height as i32,
                            gdk_pixbuf::InterpType::Bilinear,
                        );
                        img.set_from_pixbuf(pixbuf.as_ref());
                    });
                }
            });
        });
    }

    fn zoom_from_scroll(&self, dy: f64) {
        if !self.zoom_spinner.is_sensitive() {
            return;
        }
        let delta = if dy < 0.0 { 5.0 } else if dy > 0.0 { -5.0 } else { 0.0 };
        self.update_zoom_factor(delta);
    }

    fn update_zoom_factor(&self, modifier: f64) {
        let adj_zoom = self.zoom() + modifier;
        let (min_zoom, max_zoom) = self.zoom_spinner.range();
        if adj_zoom >= min_zoom && adj_zoom <= max_zoom {
            self.zoom_spinner.set_value(self.zoom() + modifier);
        }
    }

    fn update_thermometer(&self) {
        let palette = self.palette();

        self.thermometer.borrow_mut().set_minimum(self.minimum_temperature());
        self.thermometer.borrow_mut().set_maximum(self.maximum_temperature());
        self.thermometer.borrow_mut().set_palette(palette);
        self.thermometer.borrow().queue_draw();
    }

    fn minimum_temperature(&self) -> f32 {
        self.min_spinner.value() as f32
    }

    fn maximum_temperature(&self) -> f32 {
        self.max_spinner.value() as f32
    }

    fn palette_idx(&self) -> usize {
        let idx = self.palette_chooser.active_id().map_or(0, |id| {
            id.as_str()
                .as_bytes()
                .into_iter()
                .fold(0, |acc, val| acc * 10 + (max(48, *val) as usize - 48))
        });

        min(max(0, idx), PALETTES.len() - 1)
    }

    fn zoom(&self) -> f64 {
        self.zoom_spinner.value()
    }

    fn set_thermogram_tooltip_text(&self, x: i32, y: i32, tooltip: &Tooltip) -> bool {
        self.thermogram.borrow().clone().map_or(false, |thermogram| {
            // Translate the pointer coordinates to the thermogram's coordinate system
            let shape = thermogram.thermal_shape();
            let zoom = self.zoom() / 100f64;
            let x = (x as f64 / zoom) as usize;
            let y = (y as f64 / zoom) as usize;

            // Show no tooltip if x and y fall outside of the thermogram
            if x >= shape[1] || y >= shape[0] {
                return false;
            }

            // Set tooltip to temperature from thermogram
            let val = thermogram.thermal()[[y, x]];
            let temp = format!("{:.2}", val);
            tooltip.set_text(Some(temp.as_str()));
            true
        })
    }

    fn enable_thermogram_ui(&self) {
        // Function set controls to sensitive that only make sense when a thermogram is open
        self.thermogram.borrow().as_ref().map(|thermogram| {
            self.window.application()
                .and_then(|app| app.lookup_action("export"))
                .and_then(|act| act.downcast::<SimpleAction>().ok())
                .map(|act| act.set_enabled(true));
            self.window.application()
                .and_then(|app| app.lookup_action("render"))
                .and_then(|act| act.downcast::<SimpleAction>().ok())
                .map(|act| act.set_enabled(true));
            self.min_spinner.set_sensitive(true);
            self.max_spinner.set_sensitive(true);
            self.zoom_spinner.set_sensitive(true);
            self.palette_chooser.set_sensitive(true);
            if thermogram.has_palette() {
                self.embedded_palette_toggle.set_sensitive(true);
                self.embedded_palette_toggle.set_active(true);
                self.palette_chooser.set_sensitive(false);
            };
        });
    }

    fn show_failure_dialog(&self, msg: &str) {
        // Construct dialog and show
        let fail_dialog = gtk4::MessageDialog::new(
            Some(&self.window),
            gtk4::DialogFlags::MODAL,
            gtk4::MessageType::Error,
            gtk4::ButtonsType::Close,
            msg,
        );

        fail_dialog.run_future();
        fail_dialog.destroy();
    }

    fn palette(&self) -> Vec<[f32; 3]> {
        self.embedded_palette_toggle
            .is_active()
            .then(|| Some(true))
            .and(self.thermogram.borrow().as_ref())
            .and_then(|thermogram: &Thermogram| thermogram.palette())
            .or(Some(PALETTES[self.palette_idx()].iter().map(|v| *v).collect()))
            .unwrap() // TODO Remove unwrap; remove possible failure in indexed access
    }

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &impl IsA<Application>) {
        let application = application.as_ref();
        {
            // Application activation: initial window size and other values
            let that = this.clone();
            application.connect_activate(move |app| {
                app.add_window(&that.borrow().window);
                that.borrow().window.set_default_size(680, 520);
                that.borrow().window.present();
            });
        }
        {
            // Show open thermogram dialog
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| AppState::show_thermogram_chooser(&that));
            application.add_action(&open);

            // Show save thermogram dialog
            let that = this.clone();
            let save = SimpleAction::new("export", None);
            save.connect_activate(move |_, _| AppState::show_thermogram_exporter(&that));
            save.set_enabled(false);
            application.add_action(&save);

            // Show export thermogram render dialog
            let that = this.clone();
            let export = SimpleAction::new("render", None);
            export.connect_activate(move |_, _| AppState::show_render_saver(&that));
            export.set_enabled(false);
            application.add_action(&export);
        }
        {
            // Show about dialog window
            let that = this.clone();
            let about = SimpleAction::new("about", None);
            about.connect_activate(move |_, _| {
                that.borrow().about_dialog.present();
            });
            application.add_action(&about);
        }
        {
            // Zoom spinner: redraw thermogram when changed
            let that = this.clone();
            this.borrow()
                .zoom_spinner
                .connect_value_changed(move |_| that.borrow().draw_render_threaded());
        }
        {
            // Zoom with scroll wheel over the image
            let that = this.clone();
            let scroll = gtk4::EventControllerScroll::new(
                gtk4::EventControllerScrollFlags::VERTICAL,
            );
            scroll.connect_scroll(move |_, _dx, dy| {
                that.borrow().zoom_from_scroll(dy);
                glib::Propagation::Stop
            });
            this.borrow().image.add_controller(scroll);
        }
        {
            // Show the temperature of a cell when hovering over it
            let that = this.clone();
            this.borrow().image.set_has_tooltip(true);
            this.borrow().image.connect_query_tooltip(move |_, x, y, _, tooltip| {
                that.borrow().set_thermogram_tooltip_text(x, y, tooltip)
            });
        }
        {
            // Lower bound spinner: redraw when changed
            let that = this.clone();
            this.borrow().min_spinner.set_increments(0.5, 5.0);
            this.borrow().min_spinner.connect_value_changed(move |_| {
                that.borrow().update_thermometer();
                that.borrow().draw_render_threaded()
            });
        }
        {
            // Upper bound spinner: redraw when changed
            let that = this.clone();
            this.borrow().max_spinner.set_increments(0.5, 5.0);
            this.borrow().max_spinner.connect_value_changed(move |_| {
                that.borrow().update_thermometer();
                that.borrow().draw_render_threaded()
            });
        }
        {
            // Redraw on palette change
            let that = this.clone();
            this.borrow().palette_chooser.connect_changed(move |_| {
                that.borrow().update_thermometer();
                that.borrow().draw_render_threaded()
            });
        }
        {
            // Show or hide the thermometer
            let that = this.clone();
            this.borrow().thermometer_toggler.connect_toggled(move |_| {
                let show = !that.borrow().thermometer_revealer.reveals_child();
                let active = that.borrow().embedded_palette_toggle.is_active();
                let sensitive_palette = that.borrow().thermogram.borrow().is_some() || show;
                let sensitive_palette = !active && sensitive_palette;
                that.borrow().palette_chooser.set_sensitive(sensitive_palette);
                that.borrow().thermometer_revealer.set_reveal_child(show);
            });
        }
        {
            let that = this.clone();
            this.borrow().imagery_toggles.borrow().set_on_change(move || {
                that.borrow().draw_render_threaded();
            })
        }
        {
            let that = this.clone();
            this.borrow().embedded_palette_toggle.connect_toggled(move |_| {
                let use_embedded = that.borrow().embedded_palette_toggle.is_active();
                that.borrow().palette_chooser.set_sensitive(!use_embedded);
                that.borrow().update_thermometer();
                that.borrow().draw_render_threaded()
            });
        }
    }
}
