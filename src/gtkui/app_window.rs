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

use crate::thermograms::*;

use gdk_pixbuf::Pixbuf;
use gio::prelude::*;
use glib::{clone, SyncSender, Bytes, MainContext};
use gtk::prelude::*;
use gtk::*;


#[derive(Clone)]
pub struct AppState {
    // Controls
    window: ApplicationWindow,
    headerbar: HeaderBar,
    maximize_button: Button,
    image: Image,
    image_events: EventBox,
    zoom_spinner: SpinButton,
    min_spinner: SpinButton,
    max_spinner: SpinButton,

    // Model members
    thermogram: FlirThermogram,
    render_sender: SyncSender<(Bytes, usize, usize, f64)>,
    //rx: glib::Receiver<glib::Bytes>,
}

impl AppState {
    pub fn new(application: &Application, thermogram: FlirThermogram) -> AppState {
        // Load application
        let builder = Builder::new_from_file("src/gtkui/app_window.ui");
        builder.set_application(application);
        let (render_s, render_r) = MainContext::sync_channel(
            glib::PRIORITY_DEFAULT, 256
        );

        let state = AppState {
            window: builder.get_object("fikkie_window").unwrap(),
            headerbar: builder.get_object("headerbar").unwrap(),
            maximize_button: builder.get_object("maximize_button").unwrap(),
            image: builder.get_object("viewed_image").unwrap(),
            image_events: builder.get_object("viewed_image_events").unwrap(),
            zoom_spinner: builder.get_object("zoom_spinner").unwrap(),
            min_spinner: builder.get_object("min_temp_spinner").unwrap(),
            max_spinner: builder.get_object("max_temp_spinner").unwrap(),

            thermogram: thermogram,
            render_sender: render_s,
        };

        state.connect_signals(application);

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

        state
            .min_spinner
            .set_value(state.thermogram.min_temp() as f64);
        state
            .max_spinner
            .set_value(state.thermogram.max_temp() as f64);

        state
    }

    fn connect_signals(&self, application: &Application) {
        application.connect_activate(clone!(@strong self as this => move |app| {
            this.headerbar.set_title(Some(&this.thermogram.identifier()));
            app.add_window(&this.window);

            #[allow(unused)]
            this.min_spinner.connect_output(
                clone!(@strong this => move |min_spinner| {
                    min_spinner.set_text(&format!("{:?} °C", min_spinner.get_value()));
                    glib::signal::Inhibit(false)
                }),
            );

            this.min_spinner.connect_value_changed(
                clone!(@strong this => move |_| {
                    this.draw_render_threaded();
                }),
            );

            this.max_spinner.connect_value_changed(
                clone!(@strong this => move |_| {
                    this.draw_render_threaded();
                }),
            );

            this.zoom_spinner.connect_value_changed(
                clone!(@strong this => move |_| {
                    this.draw_render_threaded();
                }),
            );

            this.maximize_button.connect_clicked(
                clone!(@strong this => move |_| {
                    this.window.maximize();
                }),
            );


            this.image_events.connect_scroll_event(
                clone!(@strong this => move |_, event| {
                    let (_, y) = event.get_scroll_deltas().unwrap();
                    let delta = if y < 0.0 { 5.0 }
                                else if y > 0.0 { -5.0 }
                                else { 0.0 };
                    this.update_zoom_factor(delta);
                    glib::signal::Inhibit(true)
                }),
            );

            this.draw_render_threaded();
            this.window.set_default_size(680, 520);
            this.window.show_all();
        }));
    }

    fn draw_render_threaded(&self) {
        let min_temp = self.min_spinner.get_value() as f32;
        let max_temp = self.max_spinner.get_value() as f32;
        let zoom = self.zoom_spinner.get_value() / 100f64;
        let thermogram = self.thermogram.clone();
        let sender_local = self.render_sender.clone();

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

    fn _draw_render(&self) {
        let min_temp = self.min_spinner.get_value() as f32;
        let max_temp = self.max_spinner.get_value() as f32;
        let render = self._render_thermogram(min_temp, max_temp);
        self.image.set_from_pixbuf(Some(&render));
    }

    fn _render_thermogram(&self, min_temp: f32, max_temp: f32) -> Pixbuf {
        println!("Rendendering thermogram from scratch");
        let render = self.thermogram.render(min_temp, max_temp);
        let (bytes, width, height) = (
            render.as_slice().unwrap(),
            render.shape()[1],
            render.shape()[0],
        );

        let glib_bytes = Bytes::from(bytes);
        let pixbuf = Pixbuf::new_from_bytes(
            &glib_bytes,
            gdk_pixbuf::Colorspace::Rgb,
            false,
            8,
            width as i32,
            height as i32,
            3 * width as i32,
        );

        pixbuf
    }

    fn _zoom_pixbuf(&self) {
        let min_temp = self.min_spinner.get_value() as f32;
        let max_temp = self.max_spinner.get_value() as f32;
        let src_pb = self._render_thermogram(min_temp, max_temp); // FIXME
        let zoom = self.zoom_spinner.get_value() / 100f64;
        println!("{:?}", zoom);
        let width = (src_pb.get_width() as f64 * zoom) as i32;
        let height = (src_pb.get_height() as f64 * zoom) as i32;
        let pb_new = src_pb.scale_simple(width, height, gdk_pixbuf::InterpType::Bilinear);
        self.image.set_from_pixbuf(pb_new.as_ref())
    }

    fn update_zoom_factor(&self, modifier: f64) {
        self.zoom_spinner
            .set_value(self.zoom_spinner.get_value() + modifier);
    }
}
