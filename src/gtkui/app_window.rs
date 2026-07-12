use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gio::SimpleAction;
use gtk4::prelude::*;
use gtk4::{Builder, FileFilter, Picture};
use libadwaita as adw;

use libblackbody::{Thermogram, ThermogramTrait};

const UI: &str = "/eu/nimmerfort/blackbody/resources/eu.nimmerfort.blackbody.ui";

pub struct AppState {
    window: adw::ApplicationWindow,
    image: Picture,
    filter_thermograms: FileFilter,
    filter_all_files: FileFilter,
    thermogram: RefCell<Option<Thermogram>>,
}

impl AppState {
    pub fn new(application: &impl IsA<adw::Application>) -> Rc<RefCell<AppState>> {
        let builder = Builder::from_resource(UI);

        let state = AppState {
            window: builder.object("blackbody_window").unwrap(),
            image: builder.object("viewed_image").unwrap(),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
            thermogram: RefCell::new(None),
        };

        let this = Rc::new(RefCell::new(state));
        AppState::connect_signals(&this, application);
        this
    }

    pub fn set_thermogram_from_path(&self, path: Option<&Path>) {
        // Rendering added in step 2; for now just store and set title
        if let Some(path) = path {
            if let Some(thermogram) = Thermogram::from_file(path) {
                self.window.set_title(Some(thermogram.identifier()));
                *self.thermogram.borrow_mut() = Some(thermogram);
            }
        }
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

    fn connect_signals(this: &Rc<RefCell<Self>>, application: &impl IsA<adw::Application>) {
        let application = application.as_ref();
        {
            let that = this.clone();
            application.connect_activate(move |app| {
                app.add_window(&that.borrow().window);
                that.borrow().window.present();
            });
        }
        {
            let that = this.clone();
            let open = SimpleAction::new("open", None);
            open.connect_activate(move |_, _| Self::show_open_dialog(&that));
            application.add_action(&open);
        }
    }
}
