//! The info and measurements sidebars: populating their content on file load
//! and keeping the two toggle buttons and the split view in sync.

use std::rc::Rc;

use gettextrs::gettext;
use gtk4::prelude::*;
use gtk4::ToggleButton;
use libadwaita as adw;
use libadwaita::prelude::*;
use libadwaita::{ActionRow, PreferencesGroup};

use super::app_window::AppState;
use super::dialogs::tr;
use super::units::TempUnit;
use libblackbody::{Measurement, Thermogram, ThermogramTrait};

impl AppState {
    pub(super) fn connect_sidebar(this: &Rc<Self>) {
        // Sidebar toggles: measurements / info share one panel
        let that = this.clone();
        this.ui.header.info_button.connect_toggled(move |btn| {
            that.apply_sidebar(btn);
        });
        let that = this.clone();
        this.ui.header.measurements_button.connect_toggled(move |btn| {
            that.apply_sidebar(btn);
        });
        // Sidebar dismissed some other way (e.g. tap outside in overlay mode):
        // untoggle both buttons so they stay in sync.
        let that = this.clone();
        this.ui.sidebar.split_view.connect_show_sidebar_notify(move |sv| {
            if !sv.shows_sidebar() {
                that.ui.header.info_button.set_active(false);
                that.ui.header.measurements_button.set_active(false);
            }
        });
    }

    /// One of the two sidebar toggles changed: keep them mutually exclusive and
    /// show the sidebar when either is active.
    fn apply_sidebar(&self, button: &ToggleButton) {
        let header = &self.ui.header;
        if button.is_active() {
            for tb in [&header.info_button, &header.measurements_button] {
                if *tb != *button {
                    tb.set_active(false);
                }
            }
        }
        let sidebar = &self.ui.sidebar;
        sidebar.info.set_visible(header.info_button.is_active());
        sidebar.measurements.set_visible(header.measurements_button.is_active());
        sidebar.split_view.set_show_sidebar(
            header.info_button.is_active() || header.measurements_button.is_active(),
        );
    }

    pub(super) fn populate_info_sidebar(&self, thermogram: Option<&Thermogram>) {
        let sidebar = &self.ui.sidebar.info;
        while let Some(child) = sidebar.first_child() {
            sidebar.remove(&child);
        }

        if let Some(t) = thermogram {
            if let Some(group) = file_group(t) {
                sidebar.append(&group);
            }
            sidebar.append(&image_group(t));
            sidebar.append(&camera_group(t));
            sidebar.append(&capture_group(&t, self.temp_unit.get()));
        }
        else {
            sidebar.append(&no_info_page());
            return;
        }
    }

    pub(super) fn populate_measurements_sidebar(&self, thermogram: Option<&Thermogram>) {
        let sidebar = &self.ui.sidebar.measurements;
        while let Some(child) = sidebar.first_child() {
            sidebar.remove(&child);
        }

        if let Some(t) = thermogram {
            let measurements = t.measurements();
            if measurements.is_empty() {
                sidebar.append(&no_measurements_page());
                return;
            }
            sidebar.append(&self.overlay_switch_group());

            let group = PreferencesGroup::new();
            for m in measurements {
                group.add(&measurement_row(t, &m, self.temp_unit.get()));
            }
            sidebar.append(&group);
        }
        else {
            sidebar.append(&no_measurements_page());
            return;
        }
    }

    /// The "Show in image" switch controlling the measurement overlay.
    fn overlay_switch_group(&self) -> PreferencesGroup {
        let switch = adw::SwitchRow::builder()
            .title(gettext("Show in image"))
            .active(self.draw_measurements.get())
            .build();
        let flag = self.draw_measurements.clone();
        let image = self.ui.canvas.image.clone();
        switch.connect_active_notify(move |sw| {
            flag.set(sw.is_active());
            image.queue_draw();
        });
        let group = PreferencesGroup::new();
        group.add(&switch);
        group
    }
}

/// Dimmed placeholder shown when the file contains no measurements.
fn no_measurements_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("find-location-symbolic")
        .title(gettext("No measurements"))
        .vexpand(true)
        .css_classes(["compact"])
        .build()
}

/// Dimmed placeholder shown when no thermogram is loaded.
fn no_info_page() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("info-outline-symbolic")
        .title(gettext("No information"))
        .vexpand(true)
        .css_classes(["compact"])
        .build()
}

// value is title (bold), label is subtitle (dim)
fn add_row(group: &PreferencesGroup, label: &str, value: &str) {
    group.add(&ActionRow::builder().title(value).subtitle(label).build());
}

/// File entry — clicking opens the parent directory.
fn file_group(thermogram: &Thermogram) -> Option<PreferencesGroup> {
    let path = thermogram.path()?;
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

    let group = PreferencesGroup::new();
    let row = ActionRow::builder().title(&parent_str).subtitle(gettext("Folder")).build();
    row.add_suffix(&open_btn);
    row.set_activatable_widget(Some(&open_btn));
    group.add(&row);
    Some(group)
}

fn image_group(thermogram: &Thermogram) -> PreferencesGroup {
    let group = PreferencesGroup::new();
    let shape = thermogram.thermal_shape();
    add_row(&group, &gettext("Dimensions"), &format!("{} × {}", shape[1], shape[0]));
    let format_str = match thermogram {
        Thermogram::Flir(_) => "FLIR JPEG",
        Thermogram::Tiff(_) => "TIFF",
        Thermogram::Png(_) => "PNG (16-bit)",
        Thermogram::Fluke(_) => "Fluke is2",
    };
    add_row(&group, &gettext("Format"), format_str);
    if let Some(path) = thermogram.path() {
        if let Ok(meta) = std::fs::metadata(path) {
            add_row(&group, &gettext("File size"), &format_file_size(meta.len()));
            if let Ok(t) = meta.created() {
                if let Some(s) = format_system_time(t) { add_row(&group, &gettext("Created"), &s); }
            }
            if let Ok(t) = meta.modified() {
                if let Some(s) = format_system_time(t) { add_row(&group, &gettext("Modified"), &s); }
            }
        }
    }
    group
}

fn camera_group(thermogram: &Thermogram) -> PreferencesGroup {
    let group = PreferencesGroup::new();
    if let Some(meta) = thermogram.camera_metadata() {
        if let Some(v) = &meta.make { add_row(&group, &gettext("Make"), v); }
        if let Some(v) = &meta.model { add_row(&group, &gettext("Model"), v); }
        if let Some(v) = meta.focal_length { add_row(&group, &gettext("Focal length"), &format!("{v:.1} mm")); }
        if let Some(v) = &meta.date_time { add_row(&group, &gettext("Photographed"), &format_exif_datetime(v)); }
        if let (Some(lat), Some(lon)) = (meta.gps_latitude, meta.gps_longitude) {
            group.add(&location_row(lat, lon));
        }
    }
    group
}

/// GPS location row — the pin button opens the position in the user's maps
/// app (RFC 5870 `geo:` URI), falling back to OpenStreetMap in the browser
/// when no handler is installed.
fn location_row(lat: f64, lon: f64) -> ActionRow {
    let pin_btn = gtk4::Button::builder()
        .icon_name("mark-location-symbolic")
        .valign(gtk4::Align::Center)
        .css_classes(["flat"])
        .tooltip_text(gettext("Show on map"))
        .build();
    pin_btn.connect_clicked(move |_| {
        let geo = format!("geo:{lat},{lon}");
        let osm = format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}#map=16/{lat}/{lon}");
        gtk4::UriLauncher::new(&geo).launch(None::<&gtk4::Window>, gio::Cancellable::NONE, move |result| {
            if result.is_err() {
                gtk4::UriLauncher::new(&osm).launch(None::<&gtk4::Window>, gio::Cancellable::NONE, |_| {});
            }
        });
    });

    let row = ActionRow::builder()
        .title(format!("{lat:.5}°, {lon:.5}°"))
        .subtitle(gettext("Location"))
        .build();
    row.add_suffix(&pin_btn);
    row.set_activatable_widget(Some(&pin_btn));
    row
}

fn capture_group(thermogram: &Thermogram, unit: TempUnit) -> PreferencesGroup {
    let group = PreferencesGroup::new();
    if let Some(cp) = thermogram.capture_params() {
        add_row(&group, &gettext("Emissivity"), &format!("{:.2}", cp.emissivity));
        add_row(&group, &gettext("Object distance"), &format!("{:.2} m", cp.object_distance_m));
        add_row(&group, &gettext("Reflected temperature"), &unit.format(cp.reflected_temp_k - 273.15));
        add_row(&group, &gettext("Relative humidity"), &format!("{:.0}%", cp.relative_humidity * 100.0));
    }
    group
}

fn measurement_row(thermogram: &Thermogram, m: &Measurement, unit: TempUnit) -> ActionRow {
    let (kind, label, coords) = describe_measurement(m);
    let subtitle = match label {
        "" => format!("{kind} {coords}"),
        l => format!("{kind} ‘{l}’ {coords}"),
    };
    let value = match thermogram.measurement_stats(m) {
        Some(s) if s.min == s.max => unit.format(s.avg),
        Some(s) => tr(
            "avg {} · {} – {}",
            &[&unit.format(s.avg), &format!("{:.1}", unit.convert(s.min)), &unit.format(s.max)],
        ),
        None => "—".into(),
    };
    ActionRow::builder().title(&value).subtitle(subtitle.trim_end()).build()
}

/// (translated kind, user-assigned label, coordinate string) for a
/// measurement's sidebar row.
fn describe_measurement(m: &Measurement) -> (String, &str, String) {
    match m {
        Measurement::Spot { label, x, y } => (gettext("Spot"), label, format!("({x}, {y})")),
        Measurement::Endpoint { label, x, y } => {
            (gettext("Endpoint"), label, format!("({x}, {y})"))
        }
        // Area params are x, y, width, height (flyr 0.7 misnames w/h as x2/y2)
        Measurement::Area { label, x: x1, y: y1, width: w, height: h } => {
            (gettext("Area"), label, format!("({x1}, {y1}) {w} × {h} px"))
        }
        Measurement::Line { label, x1, y1, x2, y2 } => {
            (gettext("Line"), label, format!("({x1}, {y1}) – ({x2}, {y2})"))
        }
        Measurement::Ellipse { label, params } if params.len() >= 6 => {
            let (xc, yc) = (params[0] as f64, params[1] as f64);
            let ru = (params[2] as f64 - xc).hypot(params[3] as f64 - yc);
            let rv = (params[4] as f64 - xc).hypot(params[5] as f64 - yc);
            (gettext("Ellipse"), label, format!("({}, {}) r {ru:.0} × {rv:.0} px", params[0], params[1]))
        }
        Measurement::Ellipse { label, .. } => (gettext("Ellipse"), label, String::new()),
        Measurement::Alarm { label, .. } => (gettext("Alarm"), label, String::new()),
        Measurement::Difference { label, .. } => (gettext("Difference"), label, String::new()),
    }
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
