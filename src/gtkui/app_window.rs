//! The application window: the `AppState` struct, construction, thermogram
//! loading and directory navigation. The widget handles live in the `ui`
//! module; topic-specific behaviour lives in the sibling modules `canvas`,
//! `palette_ui`, `sidebar`, `dialogs` and `osd`, each of which wires its own
//! signals via a `connect_*` function called from `connect_signals`.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use gettextrs::gettext;
use gio::SimpleAction;
use gtk4::prelude::*;
use gtk4::{Builder, Button, EventControllerKey, Picture};
use libadwaita as adw;

use super::dialogs::tr;
use super::palettes::PALETTES;
use super::ui::Ui;
use crate::domain::units::TempUnit;
use libblackbody::{Error, Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

/// The app's GSettings, or None when the compiled schema isn't installed or
/// predates the key (e.g. plain `cargo run` during development, where only
/// the Meson build installs schemas).
fn app_settings() -> Option<gio::Settings> {
    let schema = gio::SettingsSchemaSource::default()?.lookup("eu.nimmerfort.blackbody", true)?;
    schema
        .has_key("temperature-unit")
        .then(|| gio::Settings::new("eu.nimmerfort.blackbody"))
}

/// BGRA pixels plus width and height, shared with the render thread.
type SharedImage = Arc<Mutex<Option<(Vec<u8>, i32, i32)>>>;

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
    /// Whether the "only this file is accessible" toast was already shown
    /// for the current single-file context, to avoid stacking toasts on
    /// repeated key presses.
    nav_hint_shown: Cell<bool>,
    pub(super) min_temp: Cell<f32>,
    pub(super) max_temp: Cell<f32>,
    /// Display unit for temperatures; the data itself stays in Celsius.
    pub(super) temp_unit: Cell<TempUnit>,
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
            nav_hint_shown: Cell::new(false),
            min_temp: Cell::new(0.0),
            max_temp: Cell::new(0.0),
            temp_unit: Cell::new(TempUnit::default()),
            active_palette: RefCell::new(PALETTES[3].to_vec()), // grayscale until thermogram loaded
        });

        this.install_css();
        this.setup_placeholder();
        this.prevent_focus_stealing();
        AppState::connect_signals(&this, application);

        // We're inside connect_activate, so GTK is ready — present immediately
        let app = application.as_ref();
        app.set_accels_for_action("app.new-window",  &["<Control>n"]);
        app.set_accels_for_action("win.open",        &["<Control>o"]);
        app.set_accels_for_action("win.open-folder", &["<Control><Shift>o"]);
        app.set_accels_for_action("win.export",      &["<Control>e"]);
        app.set_accels_for_action("win.render",      &["<Control>s"]);
        app.add_window(&this.ui.window);
        this.ui.window.present();
        this
    }

    fn install_css(&self) {
        let css = gtk4::CssProvider::new();
        css.load_from_string("box.osd { border-radius: 9px; }");
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().unwrap(),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
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
        let folder_btn = Button::with_label(&gettext("Open folder…"));
        folder_btn.set_action_name(Some("win.open-folder"));
        folder_btn.set_halign(gtk4::Align::Center);
        canvas.placeholder.append(&pic);
        canvas.placeholder.append(&btn);
        canvas.placeholder.append(&folder_btn);
        canvas.placeholder.set_halign(gtk4::Align::Center);
        canvas.placeholder.set_valign(gtk4::Align::Center);
        canvas.overlay.add_overlay(&canvas.placeholder);
        canvas.overlay.add_overlay(&canvas.error_page);
    }

    /// Clicking the controls must not move keyboard focus onto them: a focused
    /// slider handle consumes Left/Right/Home/End itself, breaking the
    /// directory navigation keys after adjusting temperature or zoom with the
    /// mouse. Pointer interaction doesn't need focus, and keyboard users can
    /// still Tab to the slider's handles.
    fn prevent_focus_stealing(&self) {
        let (header, osd) = (&self.ui.header, &self.ui.osd);
        let controls: [&gtk4::Widget; 8] = [
            osd.range_slider.scale_widget(),
            osd.zoom_button.upcast_ref(),
            osd.nav_prev_button.upcast_ref(),
            osd.nav_next_button.upcast_ref(),
            header.palette_button.upcast_ref(),
            header.mode_group.upcast_ref(),
            header.info_button.upcast_ref(),
            header.measurements_button.upcast_ref(),
        ];
        for w in controls {
            w.set_focus_on_click(false);
        }
        // AdwToggleGroup doesn't forward focus-on-click to its internal
        // buttons, so switch it off on every descendant too.
        fn no_focus_on_click(widget: &gtk4::Widget) {
            widget.set_focus_on_click(false);
            let mut child = widget.first_child();
            while let Some(c) = child {
                no_focus_on_click(&c);
                child = c.next_sibling();
            }
        }
        no_focus_on_click(header.mode_group.upcast_ref());

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
        let [min, max] = &thermogram.embedded_render_range().unwrap_or_else(|| [thermogram.min_temp(), thermogram.max_temp()]);
        self.min_temp.set(*min);
        self.max_temp.set(*max);
        self.populate_info_sidebar(Some(&thermogram));
        self.populate_measurements_sidebar(Some(&thermogram));

        let has_optical = thermogram.has_optical();
        let has_pip = thermogram.has_pip();
        let embedded_palette = thermogram.palette();
        *self.thermogram.borrow_mut() = Some(Arc::new(thermogram));

        *self.active_palette.borrow_mut() = PALETTES[self.palette_idx.get()].to_vec();
        Self::update_embedded_palette(self, embedded_palette);
        self.ui.osd.range_slider.configure(*min, *max);
        self.update_controls(has_optical, has_pip);
        self.remember_directory(path);
        self.show_osd();
        self.draw_render_threaded();
        self.ui.palette.color_bar.queue_draw();
    }



    /// Enable the controls the loaded file supports and fall back to thermal
    /// mode if the active mode lost its data. Call after the thermogram is
    /// stored: switching the mode triggers a re-render.
    fn update_controls(&self, has_optical: bool, has_pip: bool) {
        self.ui.canvas.placeholder.set_visible(false);
        self.ui.canvas.error_page.set_visible(false);
        self.ui.osd.zoom_button.set_sensitive(true);
        self.action_export.set_enabled(true);
        self.action_render.set_enabled(true);

        let header = &self.ui.header;
        header.info_button.set_sensitive(true);
        header.measurements_button.set_sensitive(true);
        let modes = &header.mode_group;
        modes.set_sensitive(true);
        modes.toggle_by_name("visible").unwrap().set_enabled(has_optical);
        modes.toggle_by_name("overlay").unwrap().set_enabled(has_pip);
        let active = modes.active_name();
        if (!has_optical && active.as_deref() == Some("visible"))
            || (!has_pip && active.as_deref() == Some("overlay"))
        {
            modes.set_active_name(Some("thermal"));
        }
    }

    /// Open a folder for keyboard browsing: load its first supported file.
    /// Selecting the folder through the portal file dialog grants the sandbox
    /// read access to all its files, so Left/Right/Home/End navigation works
    /// without static filesystem permissions in the Flatpak manifest.
    pub(super) fn open_directory(self: &Rc<Self>, dir: &Path) {
        let files = scan_dir_files(dir);
        let Some(first) = files.first().cloned() else {
            self.show_error_dialog(
                &gettext("No thermograms found"),
                &tr(
                    "The folder contains no supported image files (JPEG, TIFF, PNG, IS2 or FFF).\n\nFolder: {}",
                    &[dir.to_str().unwrap_or("<invalid path>")],
                ),
            );
            return;
        };
        // Populate the browse list before the first load: if that file fails
        // to decode, `show_load_error` keeps navigation alive so the user can
        // step past it. On success `load_thermogram` re-scans via
        // `remember_directory`, which is harmless.
        *self.dir_files.borrow_mut() = files;
        self.dir_idx.set(0);
        self.nav_hint_shown.set(false);
        self.navigate_to(&first);
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
        self.ui.canvas.placeholder.set_visible(false);
        self.populate_info_sidebar(None);
        self.populate_measurements_sidebar(None);

        let page = &self.ui.canvas.error_page;
        page.set_title(&name);
        page.set_description(Some(&error.to_string()));
        page.set_visible(true);
        // dir_files stays valid so browsing continues past the bad file, but
        // the arrow sensitivity must track the new position.
        self.update_nav_bar();
    }

    /// Grey out everything that needs a loaded thermogram; `update_controls`
    /// re-enables on the next successful load. Untoggling the sidebar buttons
    /// also closes the sidebar, whose content belongs to the previous file.
    fn disable_controls(&self) {
        self.ui.header.mode_group.set_sensitive(false);
        self.ui.osd.zoom_button.set_sensitive(false);
        self.action_export.set_enabled(false);
        self.action_render.set_enabled(false);
    }

    fn remember_directory(&self, path: &Path) {
        let files = path.parent().map(scan_dir_files).unwrap_or_default();
        let idx = files.iter().position(|p| p == path).unwrap_or(0);
        *self.dir_files.borrow_mut() = files;
        self.dir_idx.set(idx);
        self.nav_hint_shown.set(false);
        self.update_nav_bar();
    }

    /// Show the OSD navigation arrows only when there are sibling files to
    /// browse to, and grey out the arrow at each end of the directory.
    fn update_nav_bar(&self) {
        let osd = &self.ui.osd;
        let count = self.dir_files.borrow().len();
        let idx = self.dir_idx.get();
        osd.nav_bar.set_visible(count > 1);
        osd.nav_prev_button.set_sensitive(idx > 0);
        osd.nav_next_button.set_sensitive(idx + 1 < count);
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

        let that = this.clone();
        let open_folder = SimpleAction::new("open-folder", None);
        open_folder.connect_activate(move |_, _| Self::show_open_folder_dialog(&that));
        window.add_action(&open_folder);

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

        Self::connect_temperature_unit(this, app);
    }

    /// The app-wide temperature unit: a stateful action driving the menu
    /// radios, persisted to GSettings. The action is shared by all windows;
    /// each window watches its state to refresh the visible temperatures.
    fn connect_temperature_unit(this: &Rc<Self>, app: &adw::Application) {
        let action = match app.lookup_action("temperature-unit") {
            Some(a) => a.downcast::<SimpleAction>().unwrap(),
            None => {
                let settings = app_settings();
                let initial = settings
                    .as_ref()
                    .map(|s| s.string("temperature-unit").to_string())
                    .unwrap_or_else(|| "celsius".into());
                let action = SimpleAction::new_stateful(
                    "temperature-unit",
                    Some(glib::VariantTy::STRING),
                    &initial.to_variant(),
                );
                action.connect_activate(move |action, param| {
                    let Some(param) = param else { return };
                    action.set_state(param);
                    if let Some(s) = &settings {
                        s.set_string("temperature-unit", param.str().unwrap_or("celsius")).ok();
                    }
                });
                app.add_action(&action);
                action
            }
        };

        let unit_of = |a: &SimpleAction| {
            TempUnit::from_key(a.state().as_ref().and_then(|v| v.str()).unwrap_or("celsius"))
        };
        this.temp_unit.set(unit_of(&action));
        this.ui.osd.range_slider.set_unit(this.temp_unit.get());
        let that = this.clone();
        action.connect_notify_local(Some("state"), move |action, _| {
            that.temp_unit.set(unit_of(action));
            that.refresh_temperature_displays();
        });
    }

    /// Re-render every temperature shown as text after a unit change.
    /// Tooltips and the thermal render need nothing: they compute on demand.
    fn refresh_temperature_displays(&self) {
        let unit = self.temp_unit.get();
        self.ui.osd.range_slider.set_unit(unit);
        let thermogram = self.thermogram.borrow();
        self.populate_info_sidebar(thermogram.as_deref());
        self.populate_measurements_sidebar(thermogram.as_deref());
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
            } else {
                that.maybe_show_nav_hint();
            }
            glib::Propagation::Stop
        });
        this.ui.window.add_controller(key_ctrl);

        let that = this.clone();
        this.ui.osd.nav_prev_button.connect_clicked(move |_| {
            if let Some(path) = that.step_directory(gtk4::gdk::Key::Left) {
                that.navigate_to(&path);
            }
        });
        let that = this.clone();
        this.ui.osd.nav_next_button.connect_clicked(move |_| {
            if let Some(path) = that.step_directory(gtk4::gdk::Key::Right) {
                that.navigate_to(&path);
            }
        });
    }

    /// Navigation was attempted but there are no siblings to browse to —
    /// under the Flatpak sandbox opening a single file grants access to only
    /// that file, so the user likely wonders why the arrow keys do nothing.
    /// Explain once per opened file, with a shortcut to the fix. Outside the
    /// sandbox an empty sibling list means the directory really holds no
    /// other supported files, so opening the folder wouldn't help and no
    /// hint is shown.
    fn maybe_show_nav_hint(self: &Rc<Self>) {
        if !running_in_flatpak()
            || self.thermogram.borrow().is_none()
            || self.dir_files.borrow().len() > 1
            || self.nav_hint_shown.get()
        {
            return;
        }
        self.nav_hint_shown.set(true);

        let toast = adw::Toast::builder()
            .title(gettext("Only this file is accessible — open its folder to browse"))
            .button_label(gettext("Open Folder…"))
            .build();
        let that = self.clone();
        toast.connect_button_clicked(move |_| Self::show_open_folder_dialog(&that));
        self.ui.toast_overlay.add_toast(toast);
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

/// Flatpak mounts `/.flatpak-info` inside every sandbox; its presence is the
/// standard way for an app to detect it is running confined.
fn running_in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn scan_dir_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return vec![] };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            matches!(ext.as_str(), "jpg" | "jpeg" | "tif" | "tiff" | "png" | "is2" | "fff")
        })
        .collect();
    files.sort();
    files
}
