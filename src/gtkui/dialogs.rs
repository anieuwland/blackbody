//! File dialogs (open, export, save render), the about dialog, and error
//! reporting.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use gettextrs::gettext;
use gtk4::prelude::*;
use gtk4::FileFilter;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::app_window::AppState;
use libblackbody::ThermogramTrait;

impl AppState {
    pub(super) fn show_open_dialog(this: &Rc<Self>) {
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&this.ui.filter_thermograms);
        filters.append(&this.ui.filter_flir);
        filters.append(&this.ui.filter_fluke);
        filters.append(&this.ui.filter_tiff);
        filters.append(&this.ui.filter_png);
        filters.append(&this.ui.filter_all_files);
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Open thermogram"))
            .filters(&filters)
            .build();
        let that = this.clone();
        dialog.open(Some(&this.ui.window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                that.set_thermogram_from_path(file.path().as_deref());
            }
        });
    }

    /// Folder selection goes through the file chooser portal, which grants
    /// the sandbox read access to the whole folder — unlike opening a single
    /// file, which leaves sibling files unreadable. This is what makes the
    /// keyboard directory navigation work in the Flatpak without static
    /// `--filesystem` permissions.
    pub(super) fn show_open_folder_dialog(this: &Rc<Self>) {
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Open folder"))
            .accept_label(gettext("Open _Folder"))
            .build();
        let that = this.clone();
        dialog.select_folder(Some(&this.ui.window), gio::Cancellable::NONE, move |result| {
            if let Ok(folder) = result {
                if let Some(path) = folder.path() {
                    that.open_directory(&path);
                }
            }
        });
    }

    #[allow(deprecated)]
    pub(super) fn show_export_dialog(this: &Rc<Self>) {
        #[allow(deprecated)]
        use gtk4::{FileChooserAction, FileChooserNative, ResponseType};
        let dialog = FileChooserNative::new(
            Some(gettext("Export thermogram…").as_str()),
            Some(&this.ui.window),
            FileChooserAction::Save,
            Some(gettext("Export").as_str()),
            Some(gettext("Cancel").as_str()),
        );
        dialog.add_filter(&mime_filter("image/tiff", &gettext("TIFF (32-bit float)")));
        dialog.add_filter(&mime_filter("image/png", &gettext("PNG (16-bit)")));
        dialog.set_current_name(&this.export_stem());

        let that = this.clone();
        dialog.connect_response(move |dlg, response| {
            if response != ResponseType::Accept { return }
            let Some(path) = dlg.file().and_then(|f| f.path()) else { return };
            let ext = dlg.filter().and_then(|f| f.name())
                .map(|n| if n.contains("PNG") { "png" } else { "tiff" })
                .unwrap_or("tiff");
            that.export_thermal_to(ensure_extension(path, ext), ext);
        });
        dialog.show();
    }

    /// Default file stem for exports: the open file's name, else "thermogram".
    fn export_stem(&self) -> String {
        self.thermogram.borrow().as_ref()
            .map(|t| Path::new(t.identifier()).file_stem()
                .and_then(|s| s.to_str()).unwrap_or("thermogram").to_string())
            .unwrap_or_else(|| "thermogram".into())
    }

    /// Export the raw thermal data to `path` as 32-bit TIFF or 16-bit PNG.
    fn export_thermal_to(&self, path: PathBuf, ext: &str) {
        let Some(thermogram) = self.thermogram.borrow().clone() else { return };
        let result = if ext == "png" {
            thermogram.export_thermal_png(&path)
        } else {
            thermogram.export_thermal(&path)
        };
        if let Err(e) = result {
            let p = path.to_str().unwrap_or("<invalid path>");
            self.show_error_dialog(
                &gettext("Export failed"),
                &tr("Failed to export to {}\nCause: {}", &[p, &e.to_string()]),
            );
        }
    }

    pub(super) fn show_render_dialog(this: &Rc<Self>) {
        let filters = gio::ListStore::new::<FileFilter>();
        filters.append(&mime_filter("image/png", "PNG"));
        let dialog = gtk4::FileDialog::builder()
            .title(gettext("Save render"))
            .filters(&filters)
            .initial_name(this.render_name())
            .build();

        let that = this.clone();
        dialog.save(Some(&this.ui.window), gio::Cancellable::NONE, move |result| {
            if let Ok(file) = result {
                if let Some(mut path) = file.path() {
                    path.set_extension("png");
                    that.save_render_to(path);
                }
            }
        });
    }

    /// Default name for saved renders: the open file's name as .png, else "render.png".
    fn render_name(&self) -> String {
        self.thermogram.borrow().as_ref()
            .map(|t| {
                let mut p = PathBuf::from(t.identifier());
                p.set_extension("png");
                p.file_name().unwrap_or_default().to_string_lossy().into_owned()
            })
            .unwrap_or_else(|| "render.png".into())
    }

    /// Render with the current palette and range and write a PNG to `path`.
    fn save_render_to(&self, path: PathBuf) {
        let Some(thermogram) = self.thermogram.borrow().clone() else { return };
        let min = self.min_temp.get();
        let max = self.max_temp.get();
        let palette = self.active_palette.borrow().clone();
        if let Err(e) = thermogram.save_render(path.clone(), min, max, &palette) {
            let p = path.to_str().unwrap_or("<invalid path>");
            self.show_error_dialog(
                &gettext("Save failed"),
                &tr("Failed to save to {}\nCause: {}", &[p, &e.to_string()]),
            );
        }
    }

    pub(super) fn show_about_dialog(&self) {
        adw::AboutDialog::builder()
            .application_name("Blackbody")
            .version(crate::config::VERSION)
            .developer_name("Arthur Nieuwland")
            .comments(gettext(
                "A viewer for FLIR thermograms and other thermal images. \
                 Explore temperatures under the cursor, render to different \
                 palettes and ranges, overlay the visible-light photo, and \
                 inspect embedded measurements and camera metadata.",
            ))
            .website("https://github.com/anieuwland/blackbody")
            .license("EUPL-1.2")
            .build()
            .present(Some(&self.ui.window));
    }

    pub(super) fn show_error_dialog(&self, title: &str, msg: &str) {
        let dialog = adw::AlertDialog::new(Some(title), Some(msg));
        dialog.add_response("close", &gettext("Close"));
        dialog.present(Some(&self.ui.window));
    }
}

fn mime_filter(mime: &str, name: &str) -> FileFilter {
    let filter = FileFilter::new();
    filter.add_mime_type(mime);
    filter.set_name(Some(name));
    filter
}

/// gettext with `{}` placeholders substituted in order, so translators see
/// one complete sentence instead of fragments.
pub(super) fn tr(msgid: &str, args: &[&str]) -> String {
    let mut s = gettext(msgid);
    for a in args {
        s = s.replacen("{}", a, 1);
    }
    s
}

/// Append `.{ext}` unless the path already carries a matching extension.
/// "tif" counts as a match for "tiff", so "foo.tif" isn't turned into
/// "foo.tif.tiff".
fn ensure_extension(path: PathBuf, ext: &str) -> PathBuf {
    let already_matches = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(ext) || (ext == "tiff" && e.eq_ignore_ascii_case("tif")))
        .unwrap_or(false);
    if already_matches {
        path
    } else {
        let mut s = path.into_os_string();
        s.push(format!(".{ext}"));
        PathBuf::from(s)
    }
}

#[cfg(test)]
mod tests {
    use super::ensure_extension;
    use std::path::PathBuf;

    #[test]
    fn ensure_extension_accepts_tif_for_tiff() {
        let p = |s: &str| PathBuf::from(s);
        // "foo.tif" previously became "foo.tif.tiff"
        assert_eq!(ensure_extension(p("foo.tif"), "tiff"), p("foo.tif"));
        assert_eq!(ensure_extension(p("foo.TIFF"), "tiff"), p("foo.TIFF"));
        assert_eq!(ensure_extension(p("foo"), "tiff"), p("foo.tiff"));
        assert_eq!(ensure_extension(p("foo.png"), "tiff"), p("foo.png.tiff"));
        assert_eq!(ensure_extension(p("foo.PNG"), "png"), p("foo.PNG"));
        assert_eq!(ensure_extension(p("foo.tif"), "png"), p("foo.tif.png"));
    }
}
