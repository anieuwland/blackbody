use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cairo::LinearGradient;
use gio::SimpleAction;
use glib::object::SendWeakRef;
use glib::MainContext;
use gtk4::prelude::*;
use gtk4::{
    Builder, Button, DrawingArea, EventControllerKey, EventControllerMotion, EventControllerScroll,
    EventControllerScrollFlags, FileFilter, FlowBox, Label,
    MenuButton, Orientation, Overlay, Picture, Scale, ScrolledWindow, SelectionMode,
    ToggleButton, Tooltip,
};
use libadwaita::{ActionRow, PreferencesGroup};
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::gtkui::palettes::PALETTES;
use libblackbody::{Measurement, Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

/// BGRA pixels plus width and height, shared with the render thread.
type SharedImage = Arc<Mutex<Option<(Vec<u8>, i32, i32)>>>;

pub struct AppState {
    window: adw::ApplicationWindow,
    image: DrawingArea,
    color_bar: DrawingArea,
    range_bar: gtk4::Box,
    min_scale: Scale,
    max_scale: Scale,
    min_label: Label,
    max_label: Label,
    mode_thermal: ToggleButton,
    mode_optical: ToggleButton,
    mode_pip: ToggleButton,
    palette_button: MenuButton,
    palette_box: gtk4::Box,
    palette_idx: Cell<usize>,
    canvas_overlay: Overlay,
    placeholder: gtk4::Box,
    all_swatches: Rc<RefCell<Vec<Button>>>,
    embedded_section: gtk4::Box,
    embedded_swatch: RefCell<Option<Button>>,
    self_ref: RefCell<std::rc::Weak<RefCell<AppState>>>,
    osd_container: gtk4::CenterBox,
    osd_show_anim: adw::TimedAnimation,
    osd_hide_anim: adw::TimedAnimation,
    osd_hide_source: Rc<Cell<Option<glib::SourceId>>>,
    scrolled_window: ScrolledWindow,
    zoom_button: MenuButton,
    zoom_label: Label,
    zoom_fit: Cell<bool>,
    zoom_factor: Cell<f64>,
    image_bgra: SharedImage,
    /// Cairo surface over the latest frame, built once per frame by
    /// `current_surface` (main thread only — ImageSurface is not Send).
    image_surface: RefCell<Option<cairo::ImageSurface>>,
    /// Bumped per render request; stale render threads compare and drop out.
    render_generation: Arc<AtomicU64>,
    mouse_pos: Cell<(f64, f64)>,
    action_export: SimpleAction,
    action_render: SimpleAction,
    info_button: ToggleButton,
    info_sidebar: gtk4::Box,
    measurements_button: ToggleButton,
    measurements_sidebar: gtk4::Box,
    draw_measurements: Rc<Cell<bool>>,
    info_split_view: adw::OverlaySplitView,
    filter_thermograms: FileFilter,
    filter_all_files: FileFilter,
    /// Arc so render threads share the (large) thermogram instead of deep-copying it.
    thermogram: RefCell<Option<Arc<Thermogram>>>,
    dir_files: RefCell<Vec<PathBuf>>,
    dir_idx: Cell<usize>,
    min_temp: Cell<f32>,
    max_temp: Cell<f32>,
    palette: RefCell<Vec<[f32; 3]>>,
}

impl AppState {
    pub fn new(application: &impl IsA<adw::Application>) -> Rc<RefCell<AppState>> {
        let builder = Builder::from_resource(UI);

        let osd_container: gtk4::CenterBox = builder.object("osd_container").unwrap();
        let show_target = adw::PropertyAnimationTarget::new(&osd_container, "opacity");
        let osd_show_anim = adw::TimedAnimation::new(&osd_container, 0.0, 1.0, 200, show_target);
        let hide_target = adw::PropertyAnimationTarget::new(&osd_container, "opacity");
        let osd_hide_anim = adw::TimedAnimation::new(&osd_container, 1.0, 0.0, 1000, hide_target);

        let state = AppState {
            window: builder.object("blackbody_window").unwrap(),
            image: builder.object("viewed_image").unwrap(),
            color_bar: builder.object("color_bar").unwrap(),
            min_scale: builder.object("min_scale").unwrap(),
            max_scale: builder.object("max_scale").unwrap(),
            min_label: builder.object("min_label").unwrap(),
            max_label: builder.object("max_label").unwrap(),
            range_bar: builder.object("range_bar").unwrap(),
            mode_thermal: builder.object("mode_thermal").unwrap(),
            mode_optical: builder.object("mode_optical").unwrap(),
            mode_pip: builder.object("mode_pip").unwrap(),
            palette_button: builder.object("palette_button").unwrap(),
            palette_box: builder.object("palette_box").unwrap(),
            palette_idx: Cell::new(0),
            canvas_overlay: builder.object("canvas_overlay").unwrap(),
            placeholder: gtk4::Box::new(Orientation::Vertical, 24),
            all_swatches: Rc::new(RefCell::new(Vec::new())),
            embedded_section: gtk4::Box::new(Orientation::Vertical, 8),
            embedded_swatch: RefCell::new(None),
            self_ref: RefCell::new(std::rc::Weak::new()),
            osd_container,
            osd_show_anim,
            osd_hide_anim,
            osd_hide_source: Rc::new(Cell::new(None)),
            scrolled_window: builder.object("scrolled_window").unwrap(),
            zoom_button: builder.object("zoom_button").unwrap(),
            zoom_label: builder.object("zoom_label").unwrap(),
            zoom_fit: Cell::new(true),
            zoom_factor: Cell::new(1.0),
            image_bgra: Arc::new(Mutex::new(None)),
            image_surface: RefCell::new(None),
            render_generation: Arc::new(AtomicU64::new(0)),
            mouse_pos: Cell::new((0.0, 0.0)),
            action_export: SimpleAction::new("export", None),
            action_render: SimpleAction::new("render", None),
            info_button: builder.object("info_button").unwrap(),
            info_sidebar: builder.object("info_sidebar").unwrap(),
            measurements_button: builder.object("measurements_button").unwrap(),
            measurements_sidebar: builder.object("measurements_sidebar").unwrap(),
            draw_measurements: Rc::new(Cell::new(true)),
            info_split_view: builder.object("info_split_view").unwrap(),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
            thermogram: RefCell::new(None),
            dir_files: RefCell::new(Vec::new()),
            dir_idx: Cell::new(0),
            min_temp: Cell::new(0.0),
            max_temp: Cell::new(0.0),
            palette: RefCell::new(PALETTES[3].to_vec()), // grayscale until thermogram loaded
        };

        let this = Rc::new(RefCell::new(state));
        *this.borrow().self_ref.borrow_mut() = Rc::downgrade(&this);

        // Remove the trough margin on the touching sides so the two scales appear as one.
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            "scale.range-min trough { margin-right: 0; border-top-right-radius: 0; border-bottom-right-radius: 0; }
             scale.range-max trough { margin-left:  0; border-top-left-radius:  0; border-bottom-left-radius:  0; }
             box.osd { border-radius: 9px; }
",
        );
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().unwrap(),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        this.borrow().min_scale.add_css_class("range-min");
        this.borrow().max_scale.add_css_class("range-max");

        {
            let s = this.borrow();
            let pic = Picture::for_resource("/eu/nimmerfort/blackbody/resources/placeholder.svg");
            pic.set_can_shrink(false);
            let btn = Button::with_label("Open thermogram…");
            btn.set_action_name(Some("win.open"));
            btn.add_css_class("suggested-action");
            btn.set_halign(gtk4::Align::Center);
            s.placeholder.append(&pic);
            s.placeholder.append(&btn);
            s.placeholder.set_halign(gtk4::Align::Center);
            s.placeholder.set_valign(gtk4::Align::Center);
            s.canvas_overlay.add_overlay(&s.placeholder);
        }

        {
            let s = this.borrow();
            s.osd_container.set_opacity(0.0);
            s.osd_container.set_can_target(false);
            let container = s.osd_container.downgrade();
            s.osd_hide_anim.connect_done(move |_| {
                if let Some(c) = container.upgrade() {
                    c.set_can_target(false);
                }
            });
        }

        AppState::setup_palette_popover(&this);
        AppState::connect_signals(&this, application);
        // We're inside connect_activate, so GTK is ready — present immediately
        let app = application.as_ref();
        app.set_accels_for_action("app.new-window", &["<Control>n"]);
        app.set_accels_for_action("win.open",       &["<Control>o"]);
        app.set_accels_for_action("win.export",     &["<Control>e"]);
        app.set_accels_for_action("win.render",     &["<Control>s"]);
        app.add_window(&this.borrow().window);
        this.borrow().window.present();
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
                    let has_info = thermogram.capture_params().is_some();
                    let has_optical = thermogram.has_optical();
                    let has_pip = thermogram.has_pip();
                    let has_measurements = !thermogram.measurements().is_empty();
                    self.populate_info_sidebar(&thermogram);
                    self.populate_measurements_sidebar(&thermogram);
                    let embedded_palette = thermogram.palette();
                    *self.thermogram.borrow_mut() = Some(Arc::new(thermogram));
                    *self.palette.borrow_mut() = PALETTES[self.palette_idx.get()].to_vec();
                    if let Some(this) = self.self_ref.borrow().upgrade() {
                        Self::update_embedded_palette(&this, embedded_palette);
                    }
                    // min_scale is inverted and stores -actual_min_temp, so:
                    //   lower (right end) = -(current max),  upper (left end) = -(min - 20)
                    self.min_scale.adjustment().set_lower(-(max as f64));
                    self.min_scale.adjustment().set_upper((20.0 - min) as f64);
                    self.max_scale.adjustment().set_lower(min as f64);
                    self.max_scale.adjustment().set_upper((max + 20.0) as f64);
                    self.min_scale.set_value(-(min as f64));
                    self.max_scale.set_value(max as f64);
                    self.min_label.set_text(&format!("{:.1} °C", min));
                    self.max_label.set_text(&format!("{:.1} °C", max));
                    self.placeholder.set_visible(false);
                    self.zoom_button.set_sensitive(true);
                    self.action_export.set_enabled(true);
                    self.action_render.set_enabled(true);
                    self.info_button.set_sensitive(has_info);
                    self.measurements_button.set_sensitive(has_measurements);
                    if !has_measurements && self.measurements_button.is_active() {
                        self.measurements_button.set_active(false);
                    }
                    self.mode_optical.set_sensitive(has_optical);
                    self.mode_pip.set_sensitive(has_pip);
                    if (!has_optical && self.mode_optical.is_active())
                        || (!has_pip && self.mode_pip.is_active())
                    {
                        self.mode_thermal.set_active(true);
                    }
                    let files = scan_dir_files(path);
                    let idx = files.iter().position(|p| p == path).unwrap_or(0);
                    *self.dir_files.borrow_mut() = files;
                    self.dir_idx.set(idx);
                    self.show_osd();
                    self.draw_render_threaded();
                    self.color_bar.queue_draw();
                }
                None => {
                    let p = path.to_str().unwrap_or("<invalid path>");
                    self.show_error_dialog("Could not open file", &format!(
                        "Failed to open file. The file may be corrupted or the camera \
                         unsupported.\n\nFile: {p}"
                    ));
                }
            }
        }
    }

    /// The latest rendered frame as a cairo surface. Drains the cross-thread
    /// slot at most once per frame; the returned clone is a refcount bump,
    /// not a pixel copy.
    fn current_surface(&self) -> Option<cairo::ImageSurface> {
        if let Some((bgra, w, h)) = self.image_bgra.lock().unwrap().take() {
            let stride = w * 4;
            if let Ok(surface) =
                cairo::ImageSurface::create_for_data(bgra, cairo::Format::Rgb24, w, h, stride)
            {
                *self.image_surface.borrow_mut() = Some(surface);
            }
        }
        self.image_surface.borrow().clone()
    }

    fn apply_zoom(&self) {
        if self.zoom_fit.get() {
            self.image.set_halign(gtk4::Align::Fill);
            self.image.set_valign(gtk4::Align::Fill);
            self.image.set_size_request(-1, -1);
            self.zoom_label.set_text("Fit");
        } else {
            let factor = self.zoom_factor.get();
            if let Some(surface) = self.current_surface() {
                let w = (surface.width() as f64 * factor) as i32;
                let h = (surface.height() as f64 * factor) as i32;
                // Center within the viewport so the image doesn't stretch to fill it.
                // The viewport always allocates max(natural, viewport_size); halign=center
                // keeps the DrawingArea at its natural (= size_request) size within that.
                self.image.set_halign(gtk4::Align::Center);
                self.image.set_valign(gtk4::Align::Center);
                self.image.set_size_request(w, h);
            }
            self.zoom_label.set_text(&format!("{}%", (factor * 100.0).round() as u32));
        }
    }

    fn draw_color_bar(context: &cairo::Context, width: f64, height: f64, palette: &[[f32; 3]]) {
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
    }

    fn draw_render_threaded(&self) {
        let min = self.min_temp.get();
        let max = self.max_temp.get();
        let palette: Vec<[f32; 3]> = self.palette.borrow().clone();
        let thermal_mode = self.is_thermal_mode();
        let pip_mode = self.mode_pip.is_active();
        let img_ref = SendWeakRef::from(self.image.downgrade());
        let surface_arc = self.image_bgra.clone();

        // Render threads finish in arbitrary order (a slider drag spawns many);
        // only the thread matching the latest generation may publish its result,
        // otherwise a slow older render would overwrite a newer one.
        let generation = self.render_generation.clone();
        let my_gen = generation.fetch_add(1, Ordering::Relaxed) + 1;

        if let Some(thermogram) = self.thermogram.borrow().clone() {
            std::thread::spawn(move || {
                if generation.load(Ordering::Relaxed) != my_gen {
                    return; // superseded while queued; skip the expensive render
                }
                let image = if pip_mode {
                    thermogram
                        .picture_in_picture(min, max, &palette)
                        .unwrap_or_else(|| thermogram.render(min, max, &palette))
                } else if !thermal_mode {
                    thermogram.optical().unwrap_or_else(|| thermogram.render(min, max, &palette))
                } else {
                    thermogram.render(min, max, &palette)
                };
                if let Some(bytes) = image.as_slice() {
                    let h = image.shape()[0] as i32;
                    let w = image.shape()[1] as i32;
                    // Convert RGB → Cairo Rgb24 (4 bytes/pixel: BGRX on little-endian)
                    let stride = w * 4;
                    let mut bgra = vec![0u8; (h * stride) as usize];
                    for (i, pixel) in bytes.chunks_exact(3).enumerate() {
                        let j = i * 4;
                        bgra[j]     = pixel[2]; // B
                        bgra[j + 1] = pixel[1]; // G
                        bgra[j + 2] = pixel[0]; // R
                    }
                    MainContext::default().invoke(move || {
                        if generation.load(Ordering::Relaxed) != my_gen {
                            return; // a newer render already published
                        }
                        *surface_arc.lock().unwrap() = Some((bgra, w, h));
                        if let Some(img) = img_ref.upgrade() {
                            img.queue_draw();
                        }
                    });
                }
            });
        }
    }

    fn query_tooltip(&self, x: i32, y: i32, tooltip: &Tooltip) -> bool {
        // Temperature readout only makes sense on the thermal render: the
        // optical and PIP images have different dimensions and geometry.
        if !self.is_thermal_mode() {
            return false;
        }
        let thermogram = self.thermogram.borrow();
        let Some(thermogram) = thermogram.as_ref() else { return false };

        let shape = thermogram.thermal_shape(); // [height, width]
        let Some((ix, iy)) = widget_to_image(
            x as f64,
            y as f64,
            shape[1],
            shape[0],
            self.image.width() as f64,
            self.image.height() as f64,
        ) else {
            return false;
        };

        let temp = thermogram.thermal()[[iy, ix]];
        tooltip.set_text(Some(&format!("{:.1} °C", temp)));
        true
    }

    fn update_embedded_palette(this: &Rc<RefCell<Self>>, palette: Option<Vec<[f32; 3]>>) {
        let embedded_section = this.borrow().embedded_section.clone();
        while let Some(child) = embedded_section.first_child() {
            embedded_section.remove(&child);
        }
        *this.borrow().embedded_swatch.borrow_mut() = None;

        let Some(palette_data) = palette else {
            embedded_section.set_visible(false);
            return;
        };

        let heading = Label::builder().label("Embedded").xalign(0.0).build();
        heading.add_css_class("heading");

        let swatch = DrawingArea::builder().width_request(80).height_request(16).build();
        {
            let pd = palette_data.clone();
            swatch.set_draw_func(move |_, ctx, w, h| {
                let g = LinearGradient::new(0.0, 0.0, w as f64, 0.0);
                let step = 1.0 / (pd.len() - 1) as f64;
                for (i, c) in pd.iter().enumerate() {
                    g.add_color_stop_rgb(i as f64 * step, c[0] as f64, c[1] as f64, c[2] as f64);
                }
                ctx.rectangle(0.0, 0.0, w as f64, h as f64);
                let _ = ctx.set_source(&g);
                let _ = ctx.fill();
            });
        }

        let name_label = Label::new(Some("Camera palette"));
        name_label.add_css_class("caption");

        let vbox = gtk4::Box::new(Orientation::Vertical, 2);
        vbox.append(&swatch);
        vbox.append(&name_label);

        let btn = Button::builder().child(&vbox).build();
        btn.add_css_class("flat");

        // Default to embedded palette: apply now and mark selected
        *this.borrow().palette.borrow_mut() = palette_data.clone();

        let all = this.borrow().all_swatches.clone();
        let that = this.clone();
        let btn_clone = btn.clone();
        btn.connect_clicked(move |_| {
            *that.borrow().palette.borrow_mut() = palette_data.clone();
            for b in all.borrow().iter() {
                b.remove_css_class("suggested-action");
            }
            btn_clone.add_css_class("suggested-action");
            that.borrow().draw_render_threaded();
            that.borrow().color_bar.queue_draw();
        });
        for b in this.borrow().all_swatches.borrow().iter() {
            b.remove_css_class("suggested-action");
        }
        btn.add_css_class("suggested-action");

        let flow = FlowBox::builder()
            .selection_mode(SelectionMode::None)
            .homogeneous(true)
            .max_children_per_line(3)
            .min_children_per_line(2)
            .build();
        flow.insert(&btn, -1);

        *this.borrow().embedded_swatch.borrow_mut() = Some(btn);
        embedded_section.append(&heading);
        embedded_section.append(&flow);
        embedded_section.set_visible(true);
    }

    fn setup_palette_popover(this: &Rc<RefCell<Self>>) {
        const GROUPS: &[(&str, &[(&str, usize)])] = &[
            ("Perceptually uniform", &[
                ("Turbo", 0),
                ("Cividis", 1),
                ("Inferno", 5),
                ("Magma", 8),
                ("Viridis", 9),
            ]),
            ("Classic", &[
                ("Grayscale", 3),
                ("Hot", 4),
                ("Rainbow", 6),
                ("Copper", 2),
            ]),
            ("Diverging", &[("Coolwarm", 7)]),
        ];

        let all_swatches = this.borrow().all_swatches.clone();
        let palette_box = this.borrow().palette_box.clone();

        let embedded_section = this.borrow().embedded_section.clone();
        embedded_section.set_visible(false);
        palette_box.prepend(&embedded_section);

        for (group_name, palettes) in GROUPS {
            let heading = Label::builder()
                .label(*group_name)
                .xalign(0.0)
                .build();
            heading.add_css_class("heading");
            palette_box.append(&heading);

            let flow = FlowBox::builder()
                .selection_mode(SelectionMode::None)
                .homogeneous(true)
                .max_children_per_line(3)
                .min_children_per_line(2)
                .build();
            palette_box.append(&flow);

            for (name, idx) in *palettes {
                let idx = *idx;
                let palette_data: Vec<[f32; 3]> = PALETTES[idx].to_vec();

                // Gradient swatch DrawingArea
                let swatch = DrawingArea::builder()
                    .width_request(80)
                    .height_request(16)
                    .build();
                {
                    let pd = palette_data.clone();
                    swatch.set_draw_func(move |_, ctx, w, h| {
                        let g = LinearGradient::new(0.0, 0.0, w as f64, 0.0);
                        let step = 1.0 / (pd.len() - 1) as f64;
                        for (i, c) in pd.iter().enumerate() {
                            g.add_color_stop_rgb(i as f64 * step, c[0] as f64, c[1] as f64, c[2] as f64);
                        }
                        ctx.rectangle(0.0, 0.0, w as f64, h as f64);
                        let _ = ctx.set_source(&g);
                        let _ = ctx.fill();
                    });
                }

                let label = Label::new(Some(name));
                label.add_css_class("caption");

                let vbox = gtk4::Box::new(Orientation::Vertical, 2);
                vbox.append(&swatch);
                vbox.append(&label);

                let btn = Button::builder().child(&vbox).build();
                btn.add_css_class("flat");

                // Mark first palette (Turbo, idx=0) as initially selected
                if idx == 0 {
                    btn.add_css_class("suggested-action");
                }

                let that = this.clone();
                let all = all_swatches.clone();
                let btn_clone = btn.clone();
                btn.connect_clicked(move |_| {
                    // Update palette in AppState
                    {
                        let s = that.borrow();
                        s.palette_idx.set(idx);
                        *s.palette.borrow_mut() = PALETTES[idx].to_vec();
                    }
                    // Update selection highlight
                    for b in all.borrow().iter() {
                        b.remove_css_class("suggested-action");
                    }
                    if let Some(b) = that.borrow().embedded_swatch.borrow().as_ref() {
                        b.remove_css_class("suggested-action");
                    }
                    btn_clone.add_css_class("suggested-action");
                    // Re-render
                    let s = that.borrow();
                    s.draw_render_threaded();
                    s.color_bar.queue_draw();
                });

                all_swatches.borrow_mut().push(btn.clone());
                flow.insert(&btn, -1);
            }
        }
    }

    fn is_thermal_mode(&self) -> bool {
        self.mode_thermal.is_active()
    }

    fn show_osd(&self) {
        if self.current_surface().is_none() { return; }
        if let Some(id) = self.osd_hide_source.replace(None) {
            id.remove();
        }
        self.osd_hide_anim.pause();
        self.osd_show_anim.set_value_from(self.osd_container.opacity());
        self.osd_container.set_can_target(true);
        self.osd_show_anim.play();
        self.schedule_osd_hide(std::time::Duration::from_secs(3));
    }

    fn schedule_osd_hide(&self, delay: std::time::Duration) {
        if let Some(id) = self.osd_hide_source.replace(None) {
            id.remove();
        }
        let shared = Rc::clone(&self.osd_hide_source);
        let show_anim = self.osd_show_anim.clone();
        let hide_anim = self.osd_hide_anim.clone();
        let container = self.osd_container.downgrade();
        let id = glib::timeout_add_local(delay, move || {
            shared.replace(None);
            show_anim.pause();
            hide_anim.set_value_from(container.upgrade().map(|c| c.opacity()).unwrap_or(1.0));
            hide_anim.play();
            glib::ControlFlow::Break
        });
        self.osd_hide_source.set(Some(id));
    }

    /// Called when a mode button becomes active. The T/O/P buttons are grouped
    /// in the .ui, so GTK enforces exactly one active (and prevents untoggling).
    fn apply_mode(this: &Rc<RefCell<Self>>) {
        let s = this.borrow();
        // PIP renders thermal data too, so palette and range still apply there.
        let uses_palette = s.is_thermal_mode() || s.mode_pip.is_active();
        s.color_bar.set_sensitive(uses_palette);
        s.range_bar.set_visible(uses_palette);
        s.palette_button.set_sensitive(uses_palette);

        // Re-render with the appropriate image
        s.draw_render_threaded();
        if uses_palette {
            s.color_bar.queue_draw();
        }
    }

    #[allow(deprecated)]
    fn show_export_dialog(this: &Rc<RefCell<Self>>) {
        #[allow(deprecated)]
        use gtk4::{FileChooserAction, FileChooserNative, ResponseType};
        let window = this.borrow().window.clone();
        let that = this.clone();
        let tiff_filter = FileFilter::new();
        tiff_filter.add_mime_type("image/tiff");
        tiff_filter.set_name(Some("TIFF (32-bit float)"));
        let png_filter = FileFilter::new();
        png_filter.add_mime_type("image/png");
        png_filter.set_name(Some("PNG (16-bit)"));
        let initial_name = that.borrow().thermogram.borrow().as_ref()
            .map(|t| Path::new(t.identifier()).file_stem()
                .and_then(|s| s.to_str()).unwrap_or("thermogram").to_string())
            .unwrap_or_else(|| "thermogram".into());
        let dialog = FileChooserNative::new(
            Some("Export thermogram…"),
            Some(&window),
            FileChooserAction::Save,
            Some("Export"),
            Some("Cancel"),
        );
        dialog.add_filter(&tiff_filter);
        dialog.add_filter(&png_filter);
        dialog.set_current_name(&initial_name);
        dialog.connect_response(move |dlg, response| {
            if response == ResponseType::Accept {
                if let Some(path) = dlg.file().and_then(|f| f.path()) {
                    let ext = dlg.filter().and_then(|f| f.name())
                        .map(|n| if n.contains("PNG") { "png" } else { "tiff" })
                        .unwrap_or("tiff");
                    let path = if path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case(ext))
                        .unwrap_or(false)
                    {
                        path
                    } else {
                        let mut s = path.into_os_string();
                        s.push(format!(".{ext}"));
                        PathBuf::from(s)
                    };
                    let thermogram = that.borrow().thermogram.borrow().clone();
                    if let Some(thermogram) = thermogram {
                        let ok = if ext == "png" {
                            thermogram.export_thermal_png(&path)
                        } else {
                            thermogram.export_thermal(&path)
                        };
                        if ok.is_none() {
                            let p = path.to_str().unwrap_or("<invalid path>");
                            that.borrow().show_error_dialog("Export failed", &format!("Failed to export to {p}"));
                        }
                    }
                }
            }
        });
        dialog.show();
    }

    fn show_render_dialog(this: &Rc<RefCell<Self>>) {
        let window = this.borrow().window.clone();
        let that = this.clone();
        let png_filter = FileFilter::new();
        png_filter.add_mime_type("image/png");
        png_filter.set_name(Some("PNG"));
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&png_filter);
        let initial_name = that.borrow().thermogram.borrow().as_ref()
            .map(|t| {
                let mut p = PathBuf::from(t.identifier());
                p.set_extension("png");
                p.file_name().unwrap_or_default().to_string_lossy().into_owned()
            })
            .unwrap_or_else(|| "render.png".into());
        let dialog = gtk4::FileDialog::builder()
            .title("Save render")
            .filters(&filters)
            .initial_name(&initial_name)
            .build();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(mut path) = file.path() {
                    path.set_extension("png");
                    let s = that.borrow();
                    let thermogram = s.thermogram.borrow().clone();
                    if let Some(thermogram) = thermogram {
                        let min = s.min_temp.get();
                        let max = s.max_temp.get();
                        let palette = s.palette.borrow().clone();
                        drop(s);
                        if thermogram.save_render(path.clone(), min, max, &palette).is_none() {
                            let p = path.to_str().unwrap_or("<invalid path>");
                            that.borrow().show_error_dialog("Save failed", &format!("Failed to save to {p}"));
                        }
                    }
                }
            }
        });
    }

    fn show_about_dialog(&self) {
        adw::AboutDialog::builder()
            .application_name("Blackbody")
            .version(crate::config::VERSION)
            .developer_name("Arthur Nieuwland")
            .website("https://bitbucket.org/nimmerwoner/blackbody/")
            .license("EUPL-1.2")
            .build()
            .present(Some(&self.window));
    }

    fn populate_info_sidebar(&self, thermogram: &Thermogram) {
        while let Some(child) = self.info_sidebar.first_child() {
            self.info_sidebar.remove(&child);
        }

        // value is title (bold), label is subtitle (dim)
        let add_row = |group: &PreferencesGroup, label: &str, value: &str| {
            group.add(&ActionRow::builder().title(value).subtitle(label).build());
        };

        // File entry — clicking opens parent directory
        if let Some(path) = thermogram.path() {
            let parent_str = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("").to_string();
            let dir_uri = gio::File::for_path(path.parent().unwrap_or(path)).uri().to_string();

            let open_btn = gtk4::Button::builder()
                .icon_name("folder-open-symbolic")
                .valign(gtk4::Align::Center)
                .css_classes(["flat"])
                .build();
            open_btn.connect_clicked(move |_| {
                gtk4::UriLauncher::new(&dir_uri).launch(None::<&gtk4::Window>, gio::Cancellable::NONE, |_| {});
            });

            let file_group = PreferencesGroup::new();
            let file_row = ActionRow::builder().title(&parent_str).subtitle("Directory").build();
            file_row.add_suffix(&open_btn);
            file_row.set_activatable_widget(Some(&open_btn));
            file_group.add(&file_row);
            self.info_sidebar.append(&file_group);
        }

        let image_group = PreferencesGroup::new();
        let shape = thermogram.thermal_shape();
        add_row(&image_group, "Dimensions", &format!("{} × {}", shape[1], shape[0]));
        let format_str = match thermogram {
            Thermogram::Flir(_) => "FLIR JPEG",
            Thermogram::Tiff(_) => "TIFF",
            Thermogram::Png(_) => "PNG (16-bit)",
        };
        add_row(&image_group, "Format", format_str);
        if let Some(path) = thermogram.path() {
            if let Ok(meta) = std::fs::metadata(path) {
                add_row(&image_group, "File size", &format_file_size(meta.len()));
                if let Ok(t) = meta.created() {
                    if let Some(s) = format_system_time(t) { add_row(&image_group, "Created", &s); }
                }
                if let Ok(t) = meta.modified() {
                    if let Some(s) = format_system_time(t) { add_row(&image_group, "Modified", &s); }
                }
            }
        }
        self.info_sidebar.append(&image_group);

        let camera_group = PreferencesGroup::new();
        if let Some(meta) = thermogram.camera_metadata() {
            if let Some(v) = &meta.make { add_row(&camera_group, "Make", v); }
            if let Some(v) = &meta.model { add_row(&camera_group, "Model", v); }
            if let Some(v) = meta.focal_length { add_row(&camera_group, "Focal length", &format!("{v:.1} mm")); }
            if let Some(v) = &meta.date_time { add_row(&camera_group, "Photographed", &format_exif_datetime(v)); }
        }
        self.info_sidebar.append(&camera_group);

        let capture_group = PreferencesGroup::new();
        if let Some(cp) = thermogram.capture_params() {
            add_row(&capture_group, "Emissivity", &format!("{:.2}", cp.emissivity));
            add_row(&capture_group, "Object distance", &format!("{:.2} m", cp.object_distance_m));
            add_row(&capture_group, "Reflected temperature", &format!("{:.1} °C", cp.reflected_temp_k - 273.15));
            add_row(&capture_group, "Relative humidity", &format!("{:.0}%", cp.relative_humidity * 100.0));
        }
        self.info_sidebar.append(&capture_group);
    }

    fn populate_measurements_sidebar(&self, thermogram: &Thermogram) {
        while let Some(child) = self.measurements_sidebar.first_child() {
            self.measurements_sidebar.remove(&child);
        }

        let measurements = thermogram.measurements();
        if measurements.is_empty() {
            return;
        }

        let switch = adw::SwitchRow::builder()
            .title("Show in image")
            .active(self.draw_measurements.get())
            .build();
        let flag = self.draw_measurements.clone();
        let image = self.image.clone();
        switch.connect_active_notify(move |sw| {
            flag.set(sw.is_active());
            image.queue_draw();
        });
        let switch_group = PreferencesGroup::new();
        switch_group.add(&switch);
        self.measurements_sidebar.append(&switch_group);

        let group = PreferencesGroup::new();
        for m in measurements {
            let (kind, label, coords) = describe_measurement(m);
            let subtitle = match label {
                "" => format!("{kind} {coords}"),
                l => format!("{kind} ‘{l}’ {coords}"),
            };
            let value = match thermogram.measurement_stats(m) {
                Some(s) if s.min == s.max => format!("{:.1} °C", s.avg),
                Some(s) => format!("avg {:.1} °C · {:.1} – {:.1} °C", s.avg, s.min, s.max),
                None => "—".into(),
            };
            group.add(&ActionRow::builder().title(&value).subtitle(subtitle.trim_end()).build());
        }
        self.measurements_sidebar.append(&group);
    }

    /// One of the two sidebar toggles changed: keep them mutually exclusive and
    /// show the sidebar when either is active.
    fn apply_sidebar(this: &Rc<RefCell<Self>>, button: &ToggleButton) {
        let s = this.borrow();
        if button.is_active() {
            for tb in [&s.info_button, &s.measurements_button] {
                if *tb != *button {
                    tb.set_active(false);
                }
            }
        }
        s.info_sidebar.set_visible(s.info_button.is_active());
        s.measurements_sidebar.set_visible(s.measurements_button.is_active());
        s.info_split_view
            .set_show_sidebar(s.info_button.is_active() || s.measurements_button.is_active());
    }

}

/// (kind, user-assigned label, coordinate string) for a measurement's sidebar row.
fn describe_measurement(m: &Measurement) -> (&'static str, &str, String) {
    match m {
        Measurement::Spot { label, x, y } => ("Spot", label, format!("({x}, {y})")),
        Measurement::Endpoint { label, x, y } => ("Endpoint", label, format!("({x}, {y})")),
        // Area params are x, y, width, height (flyr 0.7 misnames w/h as x2/y2)
        Measurement::Area { label, x1, y1, x2: w, y2: h } => {
            ("Area", label, format!("({x1}, {y1}) {w} × {h} px"))
        }
        Measurement::Line { label, x1, y1, x2, y2 } => {
            ("Line", label, format!("({x1}, {y1}) – ({x2}, {y2})"))
        }
        Measurement::Ellipse { label, params } if params.len() >= 6 => {
            let (xc, yc) = (params[0] as f64, params[1] as f64);
            let ru = (params[2] as f64 - xc).hypot(params[3] as f64 - yc);
            let rv = (params[4] as f64 - xc).hypot(params[5] as f64 - yc);
            ("Ellipse", label, format!("({}, {}) r {ru:.0} × {rv:.0} px", params[0], params[1]))
        }
        Measurement::Ellipse { label, .. } => ("Ellipse", label, String::new()),
        Measurement::Alarm { label, .. } => ("Alarm", label, String::new()),
        Measurement::Difference { label, .. } => ("Difference", label, String::new()),
    }
}

/// Scale and top-left offset of an img_w×img_h image fitted into a widget
/// (content-fit = contain: centred, scaled to fit, aspect ratio kept).
fn fit_transform(img_w: f64, img_h: f64, widget_w: f64, widget_h: f64) -> (f64, f64, f64) {
    let scale = (widget_w / img_w).min(widget_h / img_h);
    let off_x = (widget_w - img_w * scale) / 2.0;
    let off_y = (widget_h - img_h * scale) / 2.0;
    (scale, off_x, off_y)
}

/// Map a widget-space position to image pixel coordinates, or `None` when the
/// position falls in the letterbox margins around the painted image.
fn widget_to_image(
    x: f64,
    y: f64,
    img_w: usize,
    img_h: usize,
    widget_w: f64,
    widget_h: f64,
) -> Option<(usize, usize)> {
    let (scale, off_x, off_y) = fit_transform(img_w as f64, img_h as f64, widget_w, widget_h);
    let ix = (x - off_x) / scale;
    let iy = (y - off_y) / scale;
    // The < 0 check must happen in floating point: a negative value cast to
    // usize saturates to 0, silently mapping margins onto row/column 0.
    if ix < 0.0 || iy < 0.0 {
        return None;
    }
    let (ix, iy) = (ix as usize, iy as usize);
    (ix < img_w && iy < img_h).then_some((ix, iy))
}

fn scan_dir_files(path: &Path) -> Vec<PathBuf> {
    let Some(dir) = path.parent() else { return vec![] };
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            matches!(ext.as_str(), "jpg" | "jpeg" | "tif" | "tiff" | "png")
        })
        .collect();
    files.sort();
    files
}

fn format_file_size(bytes: u64) -> String {
    if bytes >= 1_000_000 { format!("{:.1} MB", bytes as f64 / 1_000_000.0) }
    else if bytes >= 1_000 { format!("{:.0} kB", bytes as f64 / 1_000.0) }
    else { format!("{bytes} B") }
}

fn format_system_time(t: std::time::SystemTime) -> Option<String> {
    let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    glib::DateTime::from_unix_local(secs).ok()?.format("%Y-%m-%d %H:%M").map(|s| s.to_string()).ok()
}

fn format_exif_datetime(s: &str) -> String {
    // EXIF stores "YYYY:MM:DD HH:MM:SS" — reformat to "YYYY-MM-DD HH:MM"
    let b = s.as_bytes();
    if b.len() >= 16 && b[4] == b':' && b[7] == b':' {
        format!("{}-{}-{} {}", &s[0..4], &s[5..7], &s[8..10], &s[11..16])
    } else {
        s.to_string()
    }
}

impl AppState {
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

    fn show_error_dialog(&self, title: &str, msg: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(msg));
        dialog.add_response("close", "Close");
        dialog.present(Some(&self.window));
    }

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &impl IsA<adw::Application>) {
        let application = application.as_ref();
        let window = this.borrow().window.clone();
        {
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| Self::show_open_dialog(&that));
            window.add_action(&open);
        }
        {
            let action_export = this.borrow().action_export.clone();
            action_export.set_enabled(false);
            let that = this.clone();
            action_export.connect_activate(move |_, _| Self::show_export_dialog(&that));
            window.add_action(&action_export);
        }
        {
            let action_render = this.borrow().action_render.clone();
            action_render.set_enabled(false);
            let that = this.clone();
            action_render.connect_activate(move |_, _| Self::show_render_dialog(&that));
            window.add_action(&action_render);
        }
        {
            let that = this.clone();
            let about = SimpleAction::new("about", None);
            about.connect_activate(move |_, _| that.borrow().show_about_dialog());
            application.add_action(&about);
        }
        {
            let app = application.upcast_ref::<adw::Application>().clone();
            let new_window = SimpleAction::new("new-window", None);
            new_window.connect_activate(move |_, _| { AppState::new(&app); });
            application.add_action(&new_window);
        }
        {
            let that = this.clone();
            this.borrow().image.set_has_tooltip(true);
            this.borrow().image.connect_query_tooltip(move |_, x, y, _, tooltip| {
                that.borrow().query_tooltip(x, y, tooltip)
            });
        }
        {
            // Mode toggles: T / O / P (grouped in the .ui — radio semantics)
            let s = this.borrow();
            for tb in [&s.mode_thermal, &s.mode_optical, &s.mode_pip] {
                let that = this.clone();
                tb.connect_toggled(move |btn| {
                    if btn.is_active() { Self::apply_mode(&that); }
                });
            }
        }
        {
            // Sidebar toggles: measurements / info share one panel
            let that = this.clone();
            this.borrow().info_button.connect_toggled(move |btn| {
                Self::apply_sidebar(&that, btn);
            });
            let that = this.clone();
            this.borrow().measurements_button.connect_toggled(move |btn| {
                Self::apply_sidebar(&that, btn);
            });
            // Sidebar dismissed some other way (e.g. tap outside in overlay mode):
            // untoggle both buttons so they stay in sync.
            let that = this.clone();
            this.borrow().info_split_view.connect_show_sidebar_notify(move |sv| {
                if !sv.shows_sidebar() {
                    that.borrow().info_button.set_active(false);
                    that.borrow().measurements_button.set_active(false);
                }
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
                Self::draw_color_bar(ctx, w, h, &palette);
            });
        }
        {
            // Colour bar tooltip: map y-position to temperature
            let that = this.clone();
            this.borrow().color_bar.set_has_tooltip(true);
            this.borrow().color_bar.connect_query_tooltip(move |widget, _, y, _, tooltip| {
                let s = that.borrow();
                let h = widget.height();
                if h == 0 { return false; }
                let position = 1.0 - y as f32 / h as f32;
                let temp = s.min_temp.get() + position * (s.max_temp.get() - s.min_temp.get());
                tooltip.set_text(Some(&format!("{:.1} °C", temp)));
                true
            });
        }
        {
            // Image draw function: scales the rendered image to fill the DrawingArea.
            // In fit mode the area fills the viewport (hexpand/vexpand=true), so the
            // image is scaled to fit. In zoom mode size_request sets the area to exactly
            // the desired pixel size, so the image fills it 1:1.
            let that = this.clone();
            this.borrow().image.set_draw_func(move |_, ctx, width, height| {
                let s = that.borrow();
                let Some(surface) = s.current_surface() else { return };
                let (img_w, img_h) = (surface.width(), surface.height());
                let (scale, off_x, off_y) =
                    fit_transform(img_w as f64, img_h as f64, width as f64, height as f64);
                let _ = ctx.save();
                ctx.translate(off_x, off_y);
                ctx.scale(scale, scale);
                let _ = ctx.set_source_surface(&surface, 0.0, 0.0);
                let _ = ctx.paint();
                let _ = ctx.restore();

                // ponytail: measurement coords are thermal pixels, which map 1:1 onto the
                // thermal render only; add the PIP transform if overlays are wanted there.
                if !s.draw_measurements.get() || !s.is_thermal_mode() {
                    return;
                }
                let thermogram = s.thermogram.borrow();
                let Some(thermogram) = thermogram.as_ref() else { return };
                let px = |v: u16| off_x + (v as f64 + 0.5) * scale;
                let py = |v: u16| off_y + (v as f64 + 0.5) * scale;
                let arm = 6.0f64.max(0.5 * scale);
                for m in thermogram.measurements() {
                    match m {
                        Measurement::Spot { x, y, .. } | Measurement::Endpoint { x, y, .. } => {
                            let (cx, cy) = (px(*x), py(*y));
                            ctx.move_to(cx - arm, cy);
                            ctx.line_to(cx + arm, cy);
                            ctx.move_to(cx, cy - arm);
                            ctx.line_to(cx, cy + arm);
                        }
                        // Area params are x, y, width, height (flyr 0.7 misnames w/h as x2/y2)
                        Measurement::Area { x1, y1, x2: w, y2: h, .. } => {
                            ctx.rectangle(px(*x1), py(*y1), *w as f64 * scale, *h as f64 * scale);
                        }
                        Measurement::Line { x1, y1, x2, y2, .. } => {
                            ctx.move_to(px(*x1), py(*y1));
                            ctx.line_to(px(*x2), py(*y2));
                        }
                        // Ellipse params: centre, then the two semi-axis endpoints
                        Measurement::Ellipse { params, .. } if params.len() >= 6 => {
                            let (xc, yc) = (params[0] as f64, params[1] as f64);
                            let (ux, uy) = (params[2] as f64 - xc, params[3] as f64 - yc);
                            let (vx, vy) = (params[4] as f64 - xc, params[5] as f64 - yc);
                            let (ru, rv) = (ux.hypot(uy), vx.hypot(vy));
                            if ru > 0.0 && rv > 0.0 {
                                // Build the path under a warped CTM, restore before stroking
                                // so the line width stays uniform.
                                let _ = ctx.save();
                                ctx.translate(px(params[0]), py(params[1]));
                                ctx.rotate(uy.atan2(ux));
                                ctx.scale(ru * scale, rv * scale);
                                ctx.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                                let _ = ctx.restore();
                            }
                        }
                        Measurement::Ellipse { .. }
                        | Measurement::Alarm { .. }
                        | Measurement::Difference { .. } => {}
                    }
                }
                // Dark casing under a white core keeps markers visible on any palette.
                ctx.set_source_rgba(0.0, 0.0, 0.0, 0.8);
                ctx.set_line_width(3.0);
                let _ = ctx.stroke_preserve();
                ctx.set_source_rgb(1.0, 1.0, 1.0);
                ctx.set_line_width(1.5);
                let _ = ctx.stroke();
            });
        }
        {
            // min_scale stores -actual_min; negate to recover temperature.
            let that = this.clone();
            this.borrow().min_scale.connect_value_changed(move |scale| {
                let actual = -(scale.value() as f32);
                let s = that.borrow();
                s.min_temp.set(actual);
                s.min_label.set_text(&format!("{:.1} °C", actual));
                s.max_scale.adjustment().set_lower(actual as f64);
                s.draw_render_threaded();
                s.color_bar.queue_draw();
            });
        }
        {
            // max_scale stores actual_max; update min_scale's lower (= -actual_max).
            let that = this.clone();
            this.borrow().max_scale.connect_value_changed(move |scale| {
                let actual = scale.value() as f32;
                let s = that.borrow();
                s.max_temp.set(actual);
                s.max_label.set_text(&format!("{:.1} °C", actual));
                s.min_scale.adjustment().set_lower(-(actual as f64));
                s.draw_render_threaded();
                s.color_bar.queue_draw();
            });
        }
        {
            // Ctrl+scroll → zoom; plain scroll → pan (handled by ScrolledWindow)
            let that = this.clone();
            let ctrl = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
            ctrl.connect_scroll(move |_controller, _dx, dy| {
                let s = that.borrow();

                let (mx, my) = s.mouse_pos.get();

                // Effective zoom factor before this step; fit mode needs its ratio computed.
                let old_factor = if s.zoom_fit.get() {
                    s.current_surface().map(|surface| {
                        let vw = s.scrolled_window.width() as f64;
                        let vh = s.scrolled_window.height() as f64;
                        (vw / surface.width() as f64).min(vh / surface.height() as f64)
                    }).unwrap_or(1.0)
                } else {
                    s.zoom_factor.get()
                };

                let new_factor = (old_factor * 1.05_f64.powf(-dy)).clamp(0.1, 10.0);
                s.zoom_fit.set(false);
                s.zoom_factor.set(new_factor);

                // Image-space point under the cursor before zoom.
                let hadj = s.scrolled_window.hadjustment();
                let vadj = s.scrolled_window.vadjustment();
                let img_x = hadj.value() + mx;
                let img_y = vadj.value() + my;
                let ratio = new_factor / old_factor;

                // Pre-expand the adjustment bounds to the new image size so that
                // set_value below is not clamped to the old (smaller) upper.
                // The layout pass will confirm the same values from set_size_request.
                if let Some(surface) = s.current_surface() {
                    hadj.set_upper((surface.width() as f64 * new_factor).ceil());
                    vadj.set_upper((surface.height() as f64 * new_factor).ceil());
                }
                hadj.set_value(img_x * ratio - mx);
                vadj.set_value(img_y * ratio - my);

                s.apply_zoom();

                glib::Propagation::Stop
            });
            this.borrow().scrolled_window.add_controller(ctrl);

            let that = this.clone();
            let motion = EventControllerMotion::new();
            motion.connect_motion(move |_, x, y| {
                that.borrow().mouse_pos.set((x, y));
            });
            this.borrow().scrolled_window.add_controller(motion);
        }
        {
            // OSD fade: show on mouse motion, hide after idle
            let that = this.clone();
            let osd_motion = EventControllerMotion::new();
            osd_motion.connect_motion(move |_, _, _| {
                that.borrow().show_osd();
            });
            this.borrow().canvas_overlay.add_controller(osd_motion);
        }
        {
            let that = this.clone();
            let set_zoom = SimpleAction::new("set-zoom", Some(glib::VariantTy::new("i").unwrap()));
            set_zoom.connect_activate(move |_, param| {
                let pct = param.and_then(|v| v.get::<i32>()).unwrap_or(100);
                let s = that.borrow();
                s.zoom_fit.set(false);
                s.zoom_factor.set(pct as f64 / 100.0);
                s.apply_zoom();
            });
            window.add_action(&set_zoom);

            let that = this.clone();
            let zoom_fit = SimpleAction::new("zoom-fit", None);
            zoom_fit.connect_activate(move |_, _| {
                let s = that.borrow();
                s.zoom_fit.set(true);
                s.apply_zoom();
            });
            window.add_action(&zoom_fit);
        }
        {
            let that = this.clone();
            let key_ctrl = EventControllerKey::new();
            key_ctrl.connect_key_pressed(move |_, key, _, _| {
                let delta: i32 = match key {
                    gtk4::gdk::Key::Left => -1,
                    gtk4::gdk::Key::Right => 1,
                    _ => return glib::Propagation::Proceed,
                };
                let path = {
                    let s = that.borrow();
                    let files = s.dir_files.borrow();
                    if files.is_empty() { return glib::Propagation::Stop; }
                    let new_idx = (s.dir_idx.get() as i32 + delta)
                        .clamp(0, files.len() as i32 - 1) as usize;
                    s.dir_idx.set(new_idx);
                    files[new_idx].clone()
                };
                that.borrow().set_thermogram_from_path(Some(&path));
                glib::Propagation::Stop
            });
            this.borrow().window.add_controller(key_ctrl);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::widget_to_image;

    #[test]
    fn maps_painted_area_and_rejects_margins() {
        // 100×50 image in a 200×200 widget: scale 2, painted 200×100, y-offset 50.
        assert_eq!(widget_to_image(0.0, 50.0, 100, 50, 200.0, 200.0), Some((0, 0)));
        assert_eq!(widget_to_image(0.0, 100.0, 100, 50, 200.0, 200.0), Some((0, 25)));
        assert_eq!(widget_to_image(199.0, 149.0, 100, 50, 200.0, 200.0), Some((99, 49)));

        // Letterbox margins above/below the image previously saturated the
        // negative offset to 0 and reported row 0 temperatures.
        assert_eq!(widget_to_image(10.0, 40.0, 100, 50, 200.0, 200.0), None);
        assert_eq!(widget_to_image(10.0, 151.0, 100, 50, 200.0, 200.0), None);

        // 50×100 image in the same widget: x margins instead.
        assert_eq!(widget_to_image(40.0, 10.0, 50, 100, 200.0, 200.0), None);
        assert_eq!(widget_to_image(160.0, 10.0, 50, 100, 200.0, 200.0), None);
    }
}
