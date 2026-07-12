use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use cairo::LinearGradient;
use gdk_pixbuf::Pixbuf;
use gio::SimpleAction;
use glib::object::SendWeakRef;
use glib::{Bytes, MainContext};
use gtk4::prelude::*;
use gtk4::{
    Builder, Button, DrawingArea, EventControllerMotion, EventControllerScroll,
    EventControllerScrollFlags, FileFilter, FlowBox, Label, ListBox, MenuButton, Orientation,
    Picture, Scale, ScrolledWindow, SelectionMode, ToggleButton, Tooltip,
};
use libadwaita::{ActionRow, PreferencesDialog, PreferencesGroup, PreferencesPage};
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
    palette_button: MenuButton,
    palette_box: gtk4::Box,
    palette_idx: Cell<usize>,
    scrolled_window: ScrolledWindow,
    zoom_button: MenuButton,
    zoom_list: ListBox,
    zoom_fit: Cell<bool>,
    zoom_factor: Cell<f64>,
    mouse_pos: Cell<(f64, f64)>,
    action_export: SimpleAction,
    action_render: SimpleAction,
    action_info: SimpleAction,
    info_button: Button,
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
            palette_button: builder.object("palette_button").unwrap(),
            palette_box: builder.object("palette_box").unwrap(),
            palette_idx: Cell::new(0),
            scrolled_window: builder.object("scrolled_window").unwrap(),
            zoom_button: builder.object("zoom_button").unwrap(),
            zoom_list: builder.object("zoom_list").unwrap(),
            zoom_fit: Cell::new(true),
            zoom_factor: Cell::new(1.0),
            mouse_pos: Cell::new((0.0, 0.0)),
            action_export: SimpleAction::new("export", None),
            action_render: SimpleAction::new("render", None),
            action_info: SimpleAction::new("info", None),
            info_button: builder.object("info_button").unwrap(),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
            thermogram: RefCell::new(None),
            min_temp: Cell::new(0.0),
            max_temp: Cell::new(0.0),
            palette: RefCell::new(PALETTES[0].iter().copied().collect()),
        };

        let this = Rc::new(RefCell::new(state));

        // Remove the trough margin on the touching sides so the two scales appear as one.
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            "scale.range-min trough { margin-right: 0; border-top-right-radius: 0; border-bottom-right-radius: 0; }
             scale.range-max trough { margin-left:  0; border-top-left-radius:  0; border-bottom-left-radius:  0; }",
        );
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().unwrap(),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        this.borrow().min_scale.add_css_class("range-min");
        this.borrow().max_scale.add_css_class("range-max");

        AppState::setup_palette_popover(&this);
        AppState::connect_signals(&this, application);
        // We're inside connect_activate, so GTK is ready — present immediately
        let app = application.as_ref();
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
                    *self.thermogram.borrow_mut() = Some(thermogram);
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
                    self.auto_button.set_sensitive(true);
                    self.zoom_button.set_sensitive(true);
                    self.action_export.set_enabled(true);
                    self.action_render.set_enabled(true);
                    self.action_info.set_enabled(has_info);
                    self.info_button.set_sensitive(has_info);
                    self.mode_optical.set_sensitive(has_optical);
                    if !has_optical && !self.mode_thermal.is_active() {
                        self.mode_thermal.set_active(true);
                    }
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

    fn apply_zoom(&self) {
        if self.zoom_fit.get() {
            self.image.set_can_shrink(true);
            self.image.set_size_request(-1, -1);
            self.zoom_button.set_icon_name("zoom-fit-best-symbolic");
        } else {
            let factor = self.zoom_factor.get();
            if let Some(p) = self.image.paintable() {
                let w = (p.intrinsic_width() as f64 * factor) as i32;
                let h = (p.intrinsic_height() as f64 * factor) as i32;
                self.image.set_can_shrink(false);
                self.image.set_size_request(w, h);
            }
            self.zoom_button.set_icon_name("zoom-in-symbolic");
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

        // Collect all swatch buttons so we can manage the selection highlight
        let all_swatches: Rc<RefCell<Vec<Button>>> = Rc::new(RefCell::new(Vec::new()));

        let palette_box = this.borrow().palette_box.clone();

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
                let palette_data: Vec<[f32; 3]> = PALETTES[idx].iter().copied().collect();

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
                        *s.palette.borrow_mut() = PALETTES[idx].iter().copied().collect();
                    }
                    // Update selection highlight
                    for b in all.borrow().iter() {
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

    fn apply_mode(this: &Rc<RefCell<Self>>, button: &ToggleButton) {
        // GTK already set `button` active before emitting toggled; just deactivate the others.
        // Touching `button` here would re-emit toggled → infinite recursion → stack overflow.
        {
            let s = this.borrow();
            for tb in [&s.mode_thermal, &s.mode_optical, &s.mode_pip] {
                if *tb != *button {
                    tb.set_active(false);
                }
            }
        }

        let s = this.borrow();
        let is_thermal = s.is_thermal_mode();
        s.color_bar.set_sensitive(is_thermal);
        s.range_bar.set_sensitive(is_thermal);
        s.palette_button.set_visible(is_thermal);

        // Re-render with the appropriate image
        s.draw_render_threaded();
        if is_thermal {
            s.color_bar.queue_draw();
        }
    }

    fn show_export_dialog(this: &Rc<RefCell<Self>>) {
        let window = this.borrow().window.clone();
        let that = this.clone();
        let tiff_filter = FileFilter::new();
        tiff_filter.add_mime_type("image/tiff");
        tiff_filter.set_name(Some("TIFF"));
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&tiff_filter);
        let initial_name = that.borrow().thermogram.borrow().as_ref()
            .map(|t| {
                let mut p = PathBuf::from(t.identifier());
                p.set_extension("tiff");
                p.file_name().unwrap_or_default().to_string_lossy().into_owned()
            })
            .unwrap_or_else(|| "thermogram.tiff".into());
        let dialog = gtk4::FileDialog::builder()
            .title("Export thermal")
            .filters(&filters)
            .initial_name(&initial_name)
            .build();
        dialog.save(Some(&window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    let thermogram = that.borrow().thermogram.borrow().clone();
                    if let Some(thermogram) = thermogram {
                        if thermogram.export_thermal(&path).is_none() {
                            let p = path.to_str().unwrap_or("<invalid path>");
                            that.borrow().show_error_dialog(&format!("Failed to export to {p}"));
                        }
                    }
                }
            }
        });
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
                            that.borrow().show_error_dialog(&format!("Failed to save to {p}"));
                        }
                    }
                }
            }
        });
    }

    fn show_about_dialog(&self) {
        adw::AboutDialog::builder()
            .application_name("Blackbody")
            .version("2.0.0")
            .developer_name("Arthur Nieuwland")
            .website("https://bitbucket.org/nimmerwoner/blackbody/")
            .license("EUPL-1.2")
            .build()
            .present(Some(&self.window));
    }

    fn show_info_dialog(&self) {
        let thermogram = self.thermogram.borrow();
        let Some(thermogram) = thermogram.as_ref() else { return };

        let dialog = PreferencesDialog::builder()
            .title("Camera Info")
            .build();
        let page = PreferencesPage::new();

        // Camera group — EXIF metadata
        let camera_group = PreferencesGroup::builder().title("Camera").build();
        let add_row = |group: &PreferencesGroup, label: &str, value: &str| {
            group.add(&ActionRow::builder().title(label).subtitle(value).build());
        };
        if let Some(meta) = thermogram.camera_metadata() {
            if let Some(v) = &meta.make { add_row(&camera_group, "Make", v); }
            if let Some(v) = &meta.model { add_row(&camera_group, "Model", v); }
            if let Some(v) = meta.focal_length { add_row(&camera_group, "Focal length", &format!("{v:.1} mm")); }
            if let Some(v) = &meta.date_time { add_row(&camera_group, "Date/time", v); }
        }

        // Capture parameters group
        let capture_group = PreferencesGroup::builder().title("Capture Parameters").build();
        if let Some(cp) = thermogram.capture_params() {
            add_row(&capture_group, "Emissivity", &format!("{:.2}", cp.emissivity));
            add_row(&capture_group, "Object distance", &format!("{:.2} m", cp.object_distance_m));
            add_row(&capture_group, "Reflected temperature", &format!("{:.1} °C", cp.reflected_temp_k - 273.15));
            add_row(&capture_group, "Relative humidity", &format!("{:.0}%", cp.relative_humidity * 100.0));

            // Planck constants group
            let planck_group = PreferencesGroup::builder().title("Planck Constants").build();
            add_row(&planck_group, "R1", &format!("{:.4}", cp.planck_r1));
            add_row(&planck_group, "R2", &format!("{:.8}", cp.planck_r2));
            add_row(&planck_group, "B", &format!("{:.2}", cp.planck_b));
            add_row(&planck_group, "F", &format!("{:.2}", cp.planck_f));
            add_row(&planck_group, "O", &format!("{}", cp.planck_o));
            page.add(&planck_group);
        }

        page.add(&camera_group);
        page.add(&capture_group);
        dialog.add(&page);
        dialog.present(Some(&self.window));
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
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| Self::show_open_dialog(&that));
            application.add_action(&open);
        }
        {
            let action_export = this.borrow().action_export.clone();
            action_export.set_enabled(false);
            let that = this.clone();
            action_export.connect_activate(move |_, _| Self::show_export_dialog(&that));
            application.add_action(&action_export);
        }
        {
            let action_render = this.borrow().action_render.clone();
            action_render.set_enabled(false);
            let that = this.clone();
            action_render.connect_activate(move |_, _| Self::show_render_dialog(&that));
            application.add_action(&action_render);
        }
        {
            let that = this.clone();
            let about = SimpleAction::new("about", None);
            about.connect_activate(move |_, _| that.borrow().show_about_dialog());
            application.add_action(&about);
        }
        {
            let action_info = this.borrow().action_info.clone();
            action_info.set_enabled(false);
            let that = this.clone();
            action_info.connect_activate(move |_, _| that.borrow().show_info_dialog());
            application.add_action(&action_info);
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
            // Auto button: snap range to image min/max
            let that = this.clone();
            this.borrow().auto_button.connect_clicked(move |_| {
                let thermogram = that.borrow().thermogram.borrow().clone();
                if let Some(thermogram) = thermogram {
                    let s = that.borrow();
                    let min = thermogram.min_temp() as f64;
                    let max = thermogram.max_temp() as f64;
                    // Reset inner bounds first so set_value is never clamped.
                    // min_scale stores -actual_min, so its lower (right) = -max.
                    s.min_scale.adjustment().set_lower(-max);
                    s.max_scale.adjustment().set_lower(min);
                    s.min_scale.set_value(-min);
                    s.max_scale.set_value(max);
                }
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
                    s.image.paintable().map(|p| {
                        let vw = s.scrolled_window.width() as f64;
                        let vh = s.scrolled_window.height() as f64;
                        (vw / p.intrinsic_width() as f64)
                            .min(vh / p.intrinsic_height() as f64)
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
                if let Some(p) = s.image.paintable() {
                    hadj.set_upper((p.intrinsic_width() as f64 * new_factor).ceil());
                    vadj.set_upper((p.intrinsic_height() as f64 * new_factor).ceil());
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
            // Zoom list: row activated selects zoom level
            // ponytail: toggle on button click skipped; popover covers the use case
            let that = this.clone();
            this.borrow().zoom_list.connect_row_activated(move |_, row| {
                const FACTORS: &[f64] = &[0.0, 0.25, 0.50, 1.0, 1.5, 2.0];
                let idx = row.index() as usize;
                let s = that.borrow();
                if idx == 0 {
                    s.zoom_fit.set(true);
                } else {
                    s.zoom_fit.set(false);
                    if let Some(&f) = FACTORS.get(idx) {
                        s.zoom_factor.set(f);
                    }
                }
                s.apply_zoom();
                if let Some(popover) = s.zoom_button.popover() {
                    popover.popdown();
                }
            });
        }
    }
}
