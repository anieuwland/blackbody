//! The application window: the `AppState`/`Ui` structs, construction,
//! thermogram loading and directory navigation. Topic-specific behaviour
//! lives in the sibling modules `canvas`, `palette_ui`, `sidebar`, `dialogs`
//! and `osd`, each of which wires its own signals via a `connect_*` function
//! called from `connect_signals`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use gettextrs::gettext;
use gio::SimpleAction;
use gtk4::prelude::*;
use gtk4::{
    Builder, Button, DrawingArea, EventControllerKey, FileFilter, Label, MenuButton, Orientation,
    Overlay, Picture, Scale, ScrolledWindow, ToggleButton,
};
use libadwaita as adw;

use super::dialogs::tr;
use super::palettes::PALETTES;
use libblackbody::{Error, Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

/// BGRA pixels plus width and height, shared with the render thread.
type SharedImage = Arc<Mutex<Option<(Vec<u8>, i32, i32)>>>;

/// The header bar: display mode toggles and the popover/sidebar buttons.
pub(super) struct HeaderUi {
    pub(super) mode_thermal: ToggleButton,
    pub(super) mode_optical: ToggleButton,
    pub(super) mode_pip: ToggleButton,
    pub(super) palette_button: MenuButton,
    pub(super) info_button: ToggleButton,
    pub(super) measurements_button: ToggleButton,
}

/// The thermogram canvas and its scroll viewport.
pub(super) struct CanvasUi {
    pub(super) image: DrawingArea,
    pub(super) scrolled_window: ScrolledWindow,
    pub(super) overlay: Overlay,
    pub(super) placeholder: gtk4::Box,
    /// Shown instead of the image when directory navigation hits an
    /// unloadable file.
    pub(super) error_page: adw::StatusPage,
}

/// The on-screen display floating over the canvas: temperature range scales
/// and the zoom menu, plus the fade animations.
pub(super) struct OsdUi {
    pub(super) container: gtk4::CenterBox,
    pub(super) show_anim: adw::TimedAnimation,
    pub(super) hide_anim: adw::TimedAnimation,
    pub(super) hide_source: Rc<Cell<Option<glib::SourceId>>>,
    pub(super) range_bar: gtk4::Box,
    pub(super) min_scale: Scale,
    pub(super) max_scale: Scale,
    pub(super) min_label: Label,
    pub(super) max_label: Label,
    pub(super) zoom_button: MenuButton,
    pub(super) zoom_label: Label,
}

/// The split view holding the info and measurements sidebars.
pub(super) struct SidebarUi {
    pub(super) split_view: adw::OverlaySplitView,
    pub(super) info: gtk4::Box,
    pub(super) measurements: gtk4::Box,
}

/// The palette popover content and the colour bar beside the canvas.
pub(super) struct PaletteUi {
    pub(super) color_bar: DrawingArea,
    pub(super) palette_box: gtk4::Box,
    pub(super) embedded_section: gtk4::Box,
    pub(super) embedded_swatch: RefCell<Option<Button>>,
    /// Standard-palette swatch buttons, keyed by their `PALETTES` index.
    pub(super) all_swatches: Rc<RefCell<Vec<(usize, Button)>>>,
}

/// All widget handles, grouped by screen area.
pub(super) struct Ui {
    pub(super) window: adw::ApplicationWindow,
    pub(super) header: HeaderUi,
    pub(super) canvas: CanvasUi,
    pub(super) osd: OsdUi,
    pub(super) sidebar: SidebarUi,
    pub(super) palette: PaletteUi,
    pub(super) filter_thermograms: FileFilter,
    pub(super) filter_all_files: FileFilter,
}

impl HeaderUi {
    fn from_builder(builder: &Builder) -> HeaderUi {
        HeaderUi {
            mode_thermal: builder.object("mode_thermal").unwrap(),
            mode_optical: builder.object("mode_optical").unwrap(),
            mode_pip: builder.object("mode_pip").unwrap(),
            palette_button: builder.object("palette_button").unwrap(),
            info_button: builder.object("info_button").unwrap(),
            measurements_button: builder.object("measurements_button").unwrap(),
        }
    }
}

impl CanvasUi {
    fn from_builder(builder: &Builder) -> CanvasUi {
        CanvasUi {
            image: builder.object("viewed_image").unwrap(),
            scrolled_window: builder.object("scrolled_window").unwrap(),
            overlay: builder.object("canvas_overlay").unwrap(),
            placeholder: gtk4::Box::new(Orientation::Vertical, 24),
            error_page: adw::StatusPage::builder()
                .icon_name("image-missing-symbolic")
                .visible(false)
                .build(),
        }
    }
}

impl OsdUi {
    fn from_builder(builder: &Builder) -> OsdUi {
        let container: gtk4::CenterBox = builder.object("osd_container").unwrap();
        let show_target = adw::PropertyAnimationTarget::new(&container, "opacity");
        let hide_target = adw::PropertyAnimationTarget::new(&container, "opacity");
        OsdUi {
            show_anim: adw::TimedAnimation::new(&container, 0.0, 1.0, 200, show_target),
            hide_anim: adw::TimedAnimation::new(&container, 1.0, 0.0, 1000, hide_target),
            hide_source: Rc::new(Cell::new(None)),
            container,
            range_bar: builder.object("range_bar").unwrap(),
            min_scale: builder.object("min_scale").unwrap(),
            max_scale: builder.object("max_scale").unwrap(),
            min_label: builder.object("min_label").unwrap(),
            max_label: builder.object("max_label").unwrap(),
            zoom_button: builder.object("zoom_button").unwrap(),
            zoom_label: builder.object("zoom_label").unwrap(),
        }
    }
}

impl SidebarUi {
    fn from_builder(builder: &Builder) -> SidebarUi {
        SidebarUi {
            split_view: builder.object("info_split_view").unwrap(),
            info: builder.object("info_sidebar").unwrap(),
            measurements: builder.object("measurements_sidebar").unwrap(),
        }
    }
}

impl PaletteUi {
    fn from_builder(builder: &Builder) -> PaletteUi {
        PaletteUi {
            color_bar: builder.object("color_bar").unwrap(),
            palette_box: builder.object("palette_box").unwrap(),
            embedded_section: gtk4::Box::new(Orientation::Vertical, 8),
            embedded_swatch: RefCell::new(None),
            all_swatches: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl Ui {
    fn from_builder(builder: &Builder) -> Ui {
        Ui {
            window: builder.object("blackbody_window").unwrap(),
            header: HeaderUi::from_builder(builder),
            canvas: CanvasUi::from_builder(builder),
            osd: OsdUi::from_builder(builder),
            sidebar: SidebarUi::from_builder(builder),
            palette: PaletteUi::from_builder(builder),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
        }
    }
}

pub struct AppState {
    pub(super) ui: Ui,
    pub(super) palette_idx: Cell<usize>,
    /// Whether the user's current choice is the file's embedded camera palette
    /// (true) or a standard palette from `PALETTES` (false). Sticky across
    /// file loads so browsing a directory doesn't reset an explicit choice.
    pub(super) use_embedded_palette: Cell<bool>,
    pub(super) zoom_fit: Cell<bool>,
    pub(super) zoom_factor: Cell<f64>,
    pub(super) image_bgra: SharedImage,
    /// Cairo surface over the latest frame, built once per frame by
    /// `current_surface` (main thread only — ImageSurface is not Send).
    pub(super) image_surface: RefCell<Option<cairo::ImageSurface>>,
    /// Bumped per render request; stale render threads compare and drop out.
    pub(super) render_generation: Arc<AtomicU64>,
    pub(super) mouse_pos: Cell<(f64, f64)>,
    pub(super) action_export: SimpleAction,
    pub(super) action_render: SimpleAction,
    pub(super) draw_measurements: Rc<Cell<bool>>,
    /// Arc so render threads share the (large) thermogram instead of deep-copying it.
    pub(super) thermogram: RefCell<Option<Arc<Thermogram>>>,
    pub(super) dir_files: RefCell<Vec<PathBuf>>,
    pub(super) dir_idx: Cell<usize>,
    pub(super) min_temp: Cell<f32>,
    pub(super) max_temp: Cell<f32>,
    pub(super) active_palette: RefCell<Vec<[f32; 3]>>,
}

impl AppState {
    pub fn new(application: &impl IsA<adw::Application>) -> Rc<AppState> {
        let builder = Builder::from_resource(UI);
        let this = Rc::new(AppState {
            ui: Ui::from_builder(&builder),
            palette_idx: Cell::new(0),
            use_embedded_palette: Cell::new(true),
            zoom_fit: Cell::new(true),
            zoom_factor: Cell::new(1.0),
            image_bgra: Arc::new(Mutex::new(None)),
            image_surface: RefCell::new(None),
            render_generation: Arc::new(AtomicU64::new(0)),
            mouse_pos: Cell::new((0.0, 0.0)),
            action_export: SimpleAction::new("export", None),
            action_render: SimpleAction::new("render", None),
            draw_measurements: Rc::new(Cell::new(true)),
            thermogram: RefCell::new(None),
            dir_files: RefCell::new(Vec::new()),
            dir_idx: Cell::new(0),
            min_temp: Cell::new(0.0),
            max_temp: Cell::new(0.0),
            active_palette: RefCell::new(PALETTES[3].to_vec()), // grayscale until thermogram loaded
        });

        this.install_css();
        this.setup_placeholder();
        this.prevent_focus_stealing();
        AppState::connect_signals(&this, application);

        // We're inside connect_activate, so GTK is ready — present immediately
        let app = application.as_ref();
        app.set_accels_for_action("app.new-window", &["<Control>n"]);
        app.set_accels_for_action("win.open",       &["<Control>o"]);
        app.set_accels_for_action("win.export",     &["<Control>e"]);
        app.set_accels_for_action("win.render",     &["<Control>s"]);
        app.add_window(&this.ui.window);
        this.ui.window.present();
        this
    }

    /// Remove the trough margin on the touching sides so the two range scales
    /// appear as one.
    fn install_css(&self) {
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
        self.ui.osd.min_scale.add_css_class("range-min");
        self.ui.osd.max_scale.add_css_class("range-max");
    }

    /// "Open thermogram…" prompt shown on the canvas until a file is loaded.
    fn setup_placeholder(&self) {
        let canvas = &self.ui.canvas;
        let pic = Picture::for_resource("/eu/nimmerfort/blackbody/resources/placeholder.svg");
        pic.set_can_shrink(false);
        let btn = Button::with_label(&gettext("Open thermogram…"));
        btn.set_action_name(Some("win.open"));
        btn.add_css_class("suggested-action");
        btn.set_halign(gtk4::Align::Center);
        canvas.placeholder.append(&pic);
        canvas.placeholder.append(&btn);
        canvas.placeholder.set_halign(gtk4::Align::Center);
        canvas.placeholder.set_valign(gtk4::Align::Center);
        canvas.overlay.add_overlay(&canvas.placeholder);
        canvas.overlay.add_overlay(&canvas.error_page);
    }

    /// Clicking the controls must not move keyboard focus onto them: a focused
    /// Scale consumes Left/Right/Home/End itself, breaking the directory
    /// navigation keys after adjusting temperature or zoom with the mouse.
    /// Pointer interaction doesn't need focus, and keyboard users can still
    /// Tab to the sliders.
    fn prevent_focus_stealing(&self) {
        let (header, osd) = (&self.ui.header, &self.ui.osd);
        let controls: [&gtk4::Widget; 9] = [
            osd.min_scale.upcast_ref(),
            osd.max_scale.upcast_ref(),
            osd.zoom_button.upcast_ref(),
            header.palette_button.upcast_ref(),
            header.mode_thermal.upcast_ref(),
            header.mode_optical.upcast_ref(),
            header.mode_pip.upcast_ref(),
            header.info_button.upcast_ref(),
            header.measurements_button.upcast_ref(),
        ];
        for w in controls {
            w.set_focus_on_click(false);
        }

        // And clicking the thermogram itself focuses it, so navigation keys
        // always come back when the user clicks the image.
        let canvas = &self.ui.canvas;
        canvas.image.set_focusable(true);
        let image = canvas.image.clone();
        let click = gtk4::GestureClick::new();
        click.connect_pressed(move |_, _, _, _| {
            image.grab_focus();
        });
        canvas.scrolled_window.add_controller(click);
    }

    pub fn set_thermogram_from_path(self: &Rc<Self>, path: Option<&Path>) {
        let Some(path) = path else { return };
        match Thermogram::from_file(path) {
            Ok(thermogram) => self.load_thermogram(thermogram, path),
            Err(e) => {
                let p = path.to_str().unwrap_or("<invalid path>");
                self.show_error_dialog(
                    &gettext("Could not open file"),
                    &tr(
                        "Failed to open file. The file may be corrupted or the camera \
                         unsupported.\n\nFile: {}\nCause: {}",
                        &[p, &e.to_string()],
                    ),
                );
            }
        }
    }

    fn load_thermogram(self: &Rc<Self>, thermogram: Thermogram, path: &Path) {
        self.ui.window.set_title(Some(thermogram.identifier()));
        let (min, max) = (thermogram.min_temp(), thermogram.max_temp());
        self.min_temp.set(min);
        self.max_temp.set(max);
        self.populate_info_sidebar(&thermogram);
        self.populate_measurements_sidebar(&thermogram);

        let has_info = thermogram.capture_params().is_some();
        let has_optical = thermogram.has_optical();
        let has_pip = thermogram.has_pip();
        let embedded_palette = thermogram.palette();
        *self.thermogram.borrow_mut() = Some(Arc::new(thermogram));

        *self.active_palette.borrow_mut() = PALETTES[self.palette_idx.get()].to_vec();
        Self::update_embedded_palette(self, embedded_palette);
        self.configure_range_scales(min, max);
        self.update_controls(has_info, has_optical, has_pip);
        self.remember_directory(path);
        self.show_osd();
        self.draw_render_threaded();
        self.ui.palette.color_bar.queue_draw();
    }

    /// min_scale is inverted and stores -actual_min_temp, so:
    ///   lower (right end) = -(current max),  upper (left end) = -(min - 20)
    fn configure_range_scales(&self, min: f32, max: f32) {
        let osd = &self.ui.osd;
        osd.min_scale.adjustment().set_lower(-(max as f64));
        osd.min_scale.adjustment().set_upper((20.0 - min) as f64);
        osd.max_scale.adjustment().set_lower(min as f64);
        osd.max_scale.adjustment().set_upper((max + 20.0) as f64);
        osd.min_scale.set_value(-(min as f64));
        osd.max_scale.set_value(max as f64);
        osd.min_label.set_text(&format!("{:.1} °C", min));
        osd.max_label.set_text(&format!("{:.1} °C", max));
    }

    /// Enable the controls the loaded file supports and fall back to thermal
    /// mode if the active mode lost its data. Call after the thermogram is
    /// stored: switching the mode triggers a re-render.
    fn update_controls(&self, has_info: bool, has_optical: bool, has_pip: bool) {
        self.ui.canvas.placeholder.set_visible(false);
        self.ui.canvas.error_page.set_visible(false);
        self.ui.osd.zoom_button.set_sensitive(true);
        self.action_export.set_enabled(true);
        self.action_render.set_enabled(true);

        let header = &self.ui.header;
        header.info_button.set_sensitive(has_info);
        // Always available once a file is open: without measurements the
        // sidebar shows an empty state instead.
        header.measurements_button.set_sensitive(true);
        header.mode_thermal.set_sensitive(true);
        header.mode_optical.set_sensitive(has_optical);
        header.mode_pip.set_sensitive(has_pip);
        if (!has_optical && header.mode_optical.is_active())
            || (!has_pip && header.mode_pip.is_active())
        {
            header.mode_thermal.set_active(true);
        }
    }

    /// Directory navigation: a failed load shows an error state on the canvas
    /// instead of a dialog, so browsing continues past unloadable files.
    fn navigate_to(self: &Rc<Self>, path: &Path) {
        match Thermogram::from_file(path) {
            Ok(thermogram) => self.load_thermogram(thermogram, path),
            Err(e) => self.show_load_error(path, &e),
        }
    }

    fn show_load_error(&self, path: &Path, error: &Error) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        self.ui.window.set_title(Some(&name));
        *self.thermogram.borrow_mut() = None;
        self.clear_canvas();
        self.disable_controls();

        let page = &self.ui.canvas.error_page;
        page.set_title(&name);
        page.set_description(Some(&error.to_string()));
        page.set_visible(true);
    }

    /// Grey out everything that needs a loaded thermogram; `update_controls`
    /// re-enables on the next successful load. Untoggling the sidebar buttons
    /// also closes the sidebar, whose content belongs to the previous file.
    fn disable_controls(&self) {
        self.ui.osd.zoom_button.set_sensitive(false);
        self.action_export.set_enabled(false);
        self.action_render.set_enabled(false);

        let header = &self.ui.header;
        header.info_button.set_active(false);
        header.measurements_button.set_active(false);
        let buttons = [
            &header.info_button,
            &header.measurements_button,
            &header.mode_thermal,
            &header.mode_optical,
            &header.mode_pip,
        ];
        for b in buttons {
            b.set_sensitive(false);
        }
    }

    fn remember_directory(&self, path: &Path) {
        let files = scan_dir_files(path);
        let idx = files.iter().position(|p| p == path).unwrap_or(0);
        *self.dir_files.borrow_mut() = files;
        self.dir_idx.set(idx);
    }

    fn connect_signals(this: &Rc<Self>, application: &impl IsA<adw::Application>) {
        Self::connect_window_actions(this);
        Self::connect_app_actions(this, application.as_ref());
        Self::connect_dir_navigation(this);
        AppState::connect_canvas(this);
        AppState::connect_palette_ui(this);
        AppState::connect_sidebar(this);
        AppState::connect_osd(this);
    }

    fn connect_window_actions(this: &Rc<Self>) {
        let window = &this.ui.window;
        let that = this.clone();
        let open = SimpleAction::new("open", None);
        open.connect_activate(move |_, _| Self::show_open_dialog(&that));
        window.add_action(&open);

        this.action_export.set_enabled(false);
        let that = this.clone();
        this.action_export.connect_activate(move |_, _| Self::show_export_dialog(&that));
        window.add_action(&this.action_export);

        this.action_render.set_enabled(false);
        let that = this.clone();
        this.action_render.connect_activate(move |_, _| Self::show_render_dialog(&that));
        window.add_action(&this.action_render);
    }

    fn connect_app_actions(this: &Rc<Self>, app: &adw::Application) {
        let that = this.clone();
        let about = SimpleAction::new("about", None);
        about.connect_activate(move |_, _| that.show_about_dialog());
        app.add_action(&about);

        let app_clone = app.clone();
        let new_window = SimpleAction::new("new-window", None);
        new_window.connect_activate(move |_, _| { AppState::new(&app_clone); });
        app.add_action(&new_window);
    }

    /// Left/Right/Home/End browse the directory of the open file.
    fn connect_dir_navigation(this: &Rc<Self>) {
        let that = this.clone();
        let key_ctrl = EventControllerKey::new();
        key_ctrl.connect_key_pressed(move |_, key, _, _| {
            use gtk4::gdk::Key;
            if !matches!(key, Key::Left | Key::Right | Key::Home | Key::End) {
                return glib::Propagation::Proceed;
            }
            if let Some(path) = that.step_directory(key) {
                that.navigate_to(&path);
            }
            glib::Propagation::Stop
        });
        this.ui.window.add_controller(key_ctrl);
    }

    /// Move the directory position by one navigation key and return the new
    /// file, or `None` when the position doesn't change.
    fn step_directory(&self, key: gtk4::gdk::Key) -> Option<PathBuf> {
        use gtk4::gdk::Key;
        let files = self.dir_files.borrow();
        let cur = self.dir_idx.get() as i32;
        let last = files.len() as i32 - 1;
        let new_idx = match key {
            Key::Left => (cur - 1).max(0),
            Key::Right => (cur + 1).min(last),
            Key::Home => 0,
            Key::End => last,
            _ => return None,
        };
        if files.is_empty() || new_idx == cur {
            return None;
        }
        self.dir_idx.set(new_idx as usize);
        Some(files[new_idx as usize].clone())
    }
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
