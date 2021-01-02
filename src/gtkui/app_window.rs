// https://gtk-rs.org/docs/gtk/struct.DrawingArea.html
// https://www.reddit.com/r/rust/comments/6catf5/drawing_to_a_gtkdrawingarea/
// https://stackoverflow.com/questions/959675/what-is-the-fastest-way-to-draw-an-image-in-gtk
// https://github.com/gtk-rs/examples/blob/master/src/bin/cairotest.rs

// Or: Drawing using OpenGL
// https://github.com/gtk-rs/gdk/issues/81
// https://gtk-rs.org/docs/gtk/struct.GLArea.html
// https://stackoverflow.com/questions/45424802/how-to-embed-an-sdl-surface-into-gtk
// https://www.bassi.io/articles/2015/02/17/using-opengl-with-gtk/

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::thread;

use gdk_pixbuf::Pixbuf;
use gio::prelude::*;
use gio::SimpleAction;
use glib::{Bytes, MainContext, SyncSender};
use gtk::prelude::*;
use gtk::*;

use libblackbody::{Thermogram, ThermogramTrait};

use crate::gtkui::palettes::PALETTES;
use crate::gtkui::thermometer::Thermometer;

#[derive(Clone)]
pub struct AppState {
    builder: Builder,

    // Controls
    window: ApplicationWindow,
    headerbar: HeaderBar,
    app_menu: PopoverMenu,
    app_menu_button: MenuButton,
    app_menu_item_open: ModelButton,
    app_menu_item_about: ModelButton,
    palette_chooser: ComboBoxText,
    image: Image,
    image_events: EventBox,
    min_spinner: SpinButton,
    max_spinner: SpinButton,
    thermometer: Rc<RefCell<Thermometer>>,
    zoom_spinner: SpinButton,
    about_dialog: AboutDialog,
    filter_thermograms: FileFilter,
    filter_all_files: FileFilter,
    accel_group: AccelGroup,

    // Model members
    thermogram: RefCell<Option<Thermogram>>,
    render_sender: SyncSender<(Bytes, usize, usize, f64)>,
    //rx: glib::Receiver<glib::Bytes>,
}

impl AppState {
    pub fn new(application: &Application, thermogram: Option<Thermogram>) -> Rc<RefCell<AppState>> {
        // Create application from builder
        let ui = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";
        let builder = Builder::new_from_resource(ui);
        builder.set_application(application);
        let (render_s, render_r) = MainContext::sync_channel(glib::PRIORITY_DEFAULT, 256);

        let thermometer = Thermometer::new(builder.get_object("thermometer").unwrap(), PALETTES[0]);

        let state = AppState {
            builder: builder.clone(),

            // Application's state struct
            window: builder.get_object("blackbody_window").unwrap(),
            headerbar: builder.get_object("headerbar").unwrap(),
            app_menu: builder.get_object("app_menu").unwrap(),
            app_menu_button: builder.get_object("app_menu_button").unwrap(),
            app_menu_item_open: builder.get_object("app_menu_item_open").unwrap(),
            app_menu_item_about: builder.get_object("app_menu_item_about").unwrap(),
            palette_chooser: builder.get_object("palette_chooser").unwrap(),
            image: builder.get_object("viewed_image").unwrap(),
            image_events: builder.get_object("viewed_image_events").unwrap(),
            min_spinner: builder.get_object("min_temp_spinner").unwrap(),
            max_spinner: builder.get_object("max_temp_spinner").unwrap(),
            thermometer: thermometer,
            zoom_spinner: builder.get_object("zoom_spinner").unwrap(),
            about_dialog: builder.get_object("about_dialog").unwrap(),
            filter_thermograms: builder.get_object("filter_thermograms").unwrap(),
            filter_all_files: builder.get_object("filter_all_files").unwrap(),
            accel_group: builder.get_object("app_accel_group").unwrap(),

            thermogram: RefCell::new(None),
            render_sender: render_s,
        };

        // Update child widget with application's state
        state.update_thermometer();

        // Some initial configuration
        state.window.add_accel_group(&state.accel_group);
        state.filter_thermograms.set_name(Some("Warmtebeelden: *.jpg (FLIR), *.tiff"));
        state.filter_all_files.set_name(Some("Alle bestanden"));

        // Set up cross-thread channel for rendering thermogram on separate thread, but
        // actually drawing in the window on the main thread. Helps against blocking UI.
        let mut img = state.image.clone();
        render_r.attach(None, move |args| AppState::connect_channel(&mut img, args));

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
            self.headerbar.set_title(Some(&thermogram.identifier()));
            self.headerbar.set_subtitle(thermogram.path());
            self.min_spinner.set_value(thermogram.min_temp().into());
            self.max_spinner.set_value(thermogram.max_temp().into());
            self.enable_thermogam_ui();

            // Update thermogram and draw
            self.thermogram.replace(Some(thermogram));
            self.draw_render_threaded();
        });
    }

    fn connect_channel(img: &mut Image, args: (Bytes, usize, usize, f64)) -> glib::Continue {
        let (glib_bytes, width, height, zoom) = args;
        let pixbuf = Pixbuf::new_from_bytes(
            &glib_bytes,
            gdk_pixbuf::Colorspace::Rgb,
            false,
            8,
            width as i32,
            height as i32,
            3 * width as i32,
        );

        let width = (pixbuf.get_width() as f64 * zoom) as i32;
        let height = (pixbuf.get_height() as f64 * zoom) as i32;
        let pixbuf_new = pixbuf.scale_simple(width, height, gdk_pixbuf::InterpType::Bilinear);

        img.set_from_pixbuf(pixbuf_new.as_ref());
        glib::Continue(true)
    }

    fn show_thermogram_chooser(&self) {
        // Prepare file chooser dialog window
        let parent = &self.window;
        let chooser = FileChooserNative::new(
            Some("Open warmtebeeld"),
            Some(parent),
            FileChooserAction::Open,
            None,
            None,
        );
        chooser.add_filter(&self.filter_thermograms);
        chooser.add_filter(&self.filter_all_files);

        // Show dialog and return if nothing chosen
        let response = chooser.run();
        if response != ResponseType::Accept {
            return;
        }

        // Handle opening a thermogram
        let path = chooser.get_filename();
        let path = path.as_ref().map(AsRef::as_ref);
        self.set_thermogram_from_path(path);
    }

    fn show_thermogram_exporter(&self) {
        // Prepare file chooser dialog window to save as png
        let parent = &self.window;
        let chooser = FileChooserNative::new(
            Some("Open warmtebeeld"),
            Some(parent),
            FileChooserAction::Save,
            None,
            None,
        );
        let tiffs = FileFilter::new();
        tiffs.set_name(Some("TIFF"));
        tiffs.add_mime_type("image/tif");
        tiffs.add_mime_type("image/tiff");
        chooser.add_filter(&tiffs);
        chooser.set_current_name("thermogram.tiff");

        // Show dialog and return if nothing chosen
        let response = chooser.run();
        if response != ResponseType::Accept {
            return;
        }

        // Handle opening a thermogram
        chooser.get_filename().map(|path| {
            self.thermogram.borrow().clone().map(|thermogram| {
                let success = thermogram.export_thermal(&path);
                if success.is_none() {
                    // Inform user of export failure
                    let p = path.to_str().unwrap_or("<invalid path>");
                    let msg = format!("Failed to export to {}", p);
                    self.show_failure_dialog(msg.as_str());
                }
            });
        });
    }

    fn show_render_saver(&self) {
        // Prepare file chooser dialog window to save as png
        let parent = &self.window;
        let chooser = FileChooserNative::new(
            Some("Open warmtebeeld"),
            Some(parent),
            FileChooserAction::Save,
            None,
            None,
        );
        let pngs = FileFilter::new();
        pngs.set_name(Some("PNG"));
        pngs.add_mime_type("image/png");
        chooser.add_filter(&pngs);
        chooser.set_current_name("export.png");

        // Show dialog and return if nothing chosen
        let response = chooser.run();
        if response != ResponseType::Accept {
            return;
        }

        // Handle opening a thermogram
        chooser.get_filename().map(|path| {
            let mut path = path;
            path.set_extension("png");

            self.thermogram.borrow().clone().map(|thermogram| {
                let min_temp = self.get_minimum_temperature();
                let max_temp = self.get_maximum_temperature();
                let palette_idx = self.get_palette_idx();
                let palette = PALETTES[palette_idx];

                let success = thermogram.save_render(path.clone(), min_temp, max_temp, palette);
                if success.is_none() {
                    // Inform user of save failure
                    let p = path.to_str().unwrap_or("<invalid path>");
                    let msg = format!("Failed to export to {}", p);
                    self.show_failure_dialog(msg.as_str());
                }
            });
        });
    }

    fn draw_render_threaded(&self) {
        let min_temp = self.get_minimum_temperature();
        let max_temp = self.get_maximum_temperature();
        let zoom = self.get_zoom() / 100f64;
        let o_thermogram = self.thermogram.clone().into_inner();
        let sender_local = self.render_sender.clone();
        let palette_idx = self.get_palette_idx();

        o_thermogram.map(|thermogram| {
            thread::spawn(move || {
                let palette = PALETTES[palette_idx];
                let render = thermogram.render(min_temp, max_temp, palette);
                let (bytes, width, height) =
                    (render.as_slice().unwrap(), render.shape()[1], render.shape()[0]);

                let glib_bytes = Bytes::from(bytes);
                sender_local
                    .send((glib_bytes, width, height, zoom))
                    .expect("Failed sending rendered bytes!");
            });
        });
    }

    fn zoom_from_scroll(&self, event: &gdk::EventScroll) -> Inhibit {
        if !self.zoom_spinner.is_sensitive() {
            // Return without updating if zoom spinner is not sensitive
            return Inhibit(true);
        }

        let (_, y) = event.get_scroll_deltas().unwrap();
        let delta = if y < 0.0 {
            5.0
        } else if y > 0.0 {
            -5.0
        } else {
            0.0
        };

        self.update_zoom_factor(delta);
        Inhibit(true)
    }

    fn update_zoom_factor(&self, modifier: f64) {
        let adj_zoom = self.get_zoom() + modifier;
        let (min_zoom, max_zoom) = self.zoom_spinner.get_range();
        if adj_zoom >= min_zoom && adj_zoom <= max_zoom {
            self.zoom_spinner.set_value(self.get_zoom() + modifier);
        }
    }

    fn update_thermometer(&self) {
        self.thermometer.borrow_mut().set_minimum(self.get_minimum_temperature());
        self.thermometer.borrow_mut().set_maximum(self.get_maximum_temperature());
        self.thermometer.borrow_mut().set_palette(PALETTES[self.get_palette_idx()]);

        self.thermometer.borrow().queue_draw();
    }

    fn get_minimum_temperature(&self) -> f32 {
        self.min_spinner.get_value() as f32
    }

    fn get_maximum_temperature(&self) -> f32 {
        self.max_spinner.get_value() as f32
    }

    fn get_palette_idx(&self) -> usize {
        self.palette_chooser.get_active_id().map_or(0, |id| id.as_str().as_bytes()[0] as usize - 48)
    }

    fn get_zoom(&self) -> f64 {
        self.zoom_spinner.get_value()
    }

    fn set_thermogram_tooltip_text(&self, x: i32, y: i32, tooltip: &Tooltip) -> bool {
        self.thermogram.borrow().clone().map_or(false, |thermogram| {
            // Translate the pointer coordinates to the thermogram's coordinate system
            let shape = thermogram.thermal_shape();
            let zoom = self.get_zoom() / 100f64;
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

    fn enable_thermogam_ui(&self) {
        // Function set controls to sensitive that only make sense when a thermogram is open
        self.builder
            .get_application()
            .and_then(|app| app.lookup_action("export"))
            .and_then(|act| act.downcast::<SimpleAction>().ok())
            .and_then(|act| Some(act.set_enabled(true)));
        self.builder
            .get_application()
            .and_then(|app| app.lookup_action("render"))
            .and_then(|act| act.downcast::<SimpleAction>().ok())
            .and_then(|act| Some(act.set_enabled(true)));
        self.min_spinner.set_sensitive(true);
        self.max_spinner.set_sensitive(true);
        self.zoom_spinner.set_sensitive(true);
    }

    fn show_failure_dialog(&self, msg: &str) {
        // Construct dialog and show
        let fail_dialog = gtk::MessageDialog::new(
            Some(&self.window),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Error,
            gtk::ButtonsType::Close,
            msg,
        );

        fail_dialog.run();
        fail_dialog.destroy();
    }

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &Application) {
        {
            // Application activation: initial window size and other values
            let that = this.clone();
            application.connect_activate(move |app| {
                app.add_window(&that.borrow().window);
                that.borrow().window.set_default_size(680, 520);
                that.borrow().window.show_all();
            });
        }
        {
            // Show open thermogram dialog
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| that.borrow().show_thermogram_chooser());
            application.add_action(&open);

            // Show save thermogram dialog
            let that = this.clone();
            let save = SimpleAction::new("export", None);
            save.connect_activate(move |_, _| that.borrow().show_thermogram_exporter());
            save.set_enabled(false);
            application.add_action(&save);

            // Show export thermogram render dialog
            let that = this.clone();
            let export = SimpleAction::new("render", None);
            export.connect_activate(move |_, _| that.borrow().show_render_saver());
            export.set_enabled(false);
            application.add_action(&export);
        }
        {
            // Show about dialog window
            let that = this.clone();
            let about = SimpleAction::new("about", None);
            about.connect_activate(move |_, _| {
                let _ = that.borrow().about_dialog.run();
                that.borrow().about_dialog.hide();
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
            // Zoom spinner: update zoom factor with scroll wheel and redraw
            let that = this.clone();
            this.borrow()
                .image_events
                .connect_scroll_event(move |_, event| that.borrow().zoom_from_scroll(event));
        }
        {
            // Show the temperature of a cell when hovering over it
            let that = this.clone();
            this.borrow().image.set_property("has-tooltip", &true.to_value()).ok().map(|_| {
                this.borrow().image.connect_query_tooltip(move |_, x, y, _, tooltip| {
                    that.borrow().set_thermogram_tooltip_text(x, y, tooltip)
                });
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
    }
}
