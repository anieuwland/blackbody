// TODO Draw image directly on drawingarea with cairo
// 	cairo_rectangle (cr, x, y, 1, 1);
//	cairo_set_source_rgb (cr, red, green, blue);
//	cairo_fill (cr);
// https://gtk-rs.org/docs/gtk/struct.DrawingArea.html
// https://www.reddit.com/r/rust/comments/6catf5/drawing_to_a_gtkdrawingarea/
// https://stackoverflow.com/questions/959675/what-is-the-fastest-way-to-draw-an-image-in-gtk
// https://github.com/gtk-rs/examples/blob/master/src/bin/cairotest.rs

// Or: Drawing using OpenGL
// https://github.com/gtk-rs/gdk/issues/81
// https://gtk-rs.org/docs/gtk/struct.GLArea.html
// https://stackoverflow.com/questions/45424802/how-to-embed-an-sdl-surface-into-gtk
// https://www.bassi.io/articles/2015/02/17/using-opengl-with-gtk/


use std::thread;
use std::cell::RefCell;
use std::rc::Rc;

use gtk::*;
use gio::SimpleAction;
use gdk_pixbuf::Pixbuf;
use gio::prelude::*;
use gtk::prelude::*;
use glib::{SyncSender, Bytes, MainContext};

use crate::thermograms::*;

#[derive(Clone)]
pub struct AppState {
    // Controls
    window: ApplicationWindow,
    headerbar: HeaderBar,
    image: Image,
    image_events: EventBox,
    zoom_spinner: SpinButton,
    min_spinner: SpinButton,
    max_spinner: SpinButton,
    app_menu: PopoverMenu,
    app_menu_button: MenuButton,

    // Model members
    thermogram: RefCell<Option<FlirThermogram>>,
    render_sender: SyncSender<(Bytes, usize, usize, f64)>,
    //rx: glib::Receiver<glib::Bytes>,
}

impl AppState {
    pub fn new(application: &Application, thermogram: Option<FlirThermogram>) -> Rc<RefCell<AppState>> {
        // Load application
        let builder = Builder::new_from_file("src/gtkui/app_window.ui");
        builder.set_application(application);
        let (render_s, render_r) = MainContext::sync_channel(
            glib::PRIORITY_DEFAULT, 256
        );

        let state = AppState {
            window: builder.get_object("fikkie_window").unwrap(),
            headerbar: builder.get_object("headerbar").unwrap(),
            image: builder.get_object("viewed_image").unwrap(),
            image_events: builder.get_object("viewed_image_events").unwrap(),
            zoom_spinner: builder.get_object("zoom_spinner").unwrap(),
            min_spinner: builder.get_object("min_temp_spinner").unwrap(),
            max_spinner: builder.get_object("max_temp_spinner").unwrap(),
            app_menu: builder.get_object("app_menu").unwrap(),
            app_menu_button: builder.get_object("app_menu_button").unwrap(),

            thermogram: RefCell::new(None),
            render_sender: render_s,
        };

        let img = state.image.clone();
        render_r.attach(None, move |(glib_bytes, width, height, zoom)| {
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
            let pixbuf_new = pixbuf.scale_simple(
                width, height, gdk_pixbuf::InterpType::Bilinear
            );

            img.set_from_pixbuf(pixbuf_new.as_ref());
            glib::Continue(true)
        });

        // Create an object containing the state that can be used in callbacks
        // and set up those callbacks.
        let this = Rc::new(RefCell::new(state));
        AppState::connect_signals(&this, application);
        this.clone().borrow().set_thermogram(thermogram);

        this
    }

    fn show_thermogram_chooser(&self) -> Option<FlirThermogram> {
        let parent = &self.window;
        let chooser = FileChooserNative::new(
            Some("Open warmtebeeld"), Some(parent), FileChooserAction::Open, None, None
        );

        // TODO Filter image types
        let response = chooser.run();
        if response != ResponseType::Accept {
            return None;
        }

        match chooser.get_filename() {
            Some(filepath) => {
                println!("Opening {:?}", filepath);
                let o_thermogram = FlirThermogram::from_file(&filepath);
                match o_thermogram {
                    Some(thermogram) => {
                        let thermogram_clone = thermogram.clone();
                        self.set_thermogram(Some(thermogram));
                        self.draw_render_threaded();
                        return Some(thermogram_clone)
                    },
                    _ => {
                        println!("Failed opening thermogram {:?}", filepath);
                        return None;
                    },
                }
            }
            _ => None,
        }
    }

    fn set_thermogram(&self, o_thermogram: Option<FlirThermogram>) {
        match o_thermogram{
            None => { self.thermogram.replace(None); },
            Some(thermogram) => {
                self.headerbar.set_title(Some(&thermogram.identifier()));
                self.min_spinner.set_value(thermogram.min_temp() as f64);
                self.max_spinner.set_value(thermogram.max_temp() as f64);
                self.thermogram.replace(Some(thermogram));
                self.draw_render_threaded();
            },
        }
    }

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &Application) {
        {   // Application activation: initial window size and other values
            let that = this.clone();
            application.connect_activate(move |app| {
                app.add_window(&that.borrow().window);
                that.borrow().window.set_default_size(680, 520);
                that.borrow().window.show_all();
            });
        }
        {   // Application menu: connecting buttons to actions
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            let menu = that.borrow().app_menu.clone();
            that.borrow().app_menu_button.set_popover(Some(&menu));
            open.connect_activate(move |_, _| { that.borrow().show_thermogram_chooser(); });
            application.add_action(&open);
        }
        {   // Zoom spinner: redraw thermogram when changed
            let that = this.clone();
            this.borrow().zoom_spinner.connect_value_changed(
                move |_| that.borrow().draw_render_threaded()
            );
        }
        {   // Zoom spinner: update zoom factor with scroll wheel and redraw
            let that = this.clone();
            this.borrow().image_events.connect_scroll_event(move |_, event| {
                let (_, y) = event.get_scroll_deltas().unwrap();
                let delta = if y < 0.0 { 5.0 }
                            else if y > 0.0 { -5.0 }
                            else { 0.0 };
                that.borrow().update_zoom_factor(delta);
                glib::signal::Inhibit(true)
            });
        }
        {   // Lower bound spinner: redraw when changed
            let that = this.clone();
            this.borrow().min_spinner.connect_value_changed(
                move |_| that.borrow().draw_render_threaded()
            );
        }
        {   // Upper bound spinner: redraw when changed
            let that = this.clone();
            this.borrow().max_spinner.connect_value_changed(
                move |_| that.borrow().draw_render_threaded()
            );
        }
    }

    fn draw_render_threaded(&self) {
        let min_temp = self.min_spinner.get_value() as f32;
        let max_temp = self.max_spinner.get_value() as f32;
        let zoom = self.zoom_spinner.get_value() / 100f64;
        let o_thermogram = self.thermogram.clone().into_inner();
        let sender_local = self.render_sender.clone();

        match o_thermogram {
            None => println!("No thermogram set"),
            Some(thermogram) => {
                thread::spawn(move || {
                    let render = thermogram.render(min_temp, max_temp);
                    let (bytes, width, height) = (
                        render.as_slice().unwrap(),
                        render.shape()[1],
                        render.shape()[0],
                    );

                    let glib_bytes = Bytes::from(bytes);
                    sender_local.send((glib_bytes, width, height, zoom))
                                .expect("Failed sending rendered bytes!");
                });
            }
        }

    }

    fn update_zoom_factor(&self, modifier: f64) {
        self.zoom_spinner
            .set_value(self.zoom_spinner.get_value() + modifier);
    }
}
