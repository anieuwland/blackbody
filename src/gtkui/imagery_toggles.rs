use std::cell::RefCell;
use std::rc::Rc;

use glib::{ObjectExt, SignalHandlerId};
use gtk::prelude::{BuilderExtManual, ToggleButtonExt};
use gtk::{Builder, Image, SpinButton, ToggleButton};
use libblackbody::Thermogram;

#[derive(Clone)]
pub struct ImageryToggles {
    pub tool_showable_thermal: ToggleButton,
    pub tool_showable_optical: ToggleButton,

    // TODO Automatically zet zoom such that optical&thermal have same dimensions when they are toggled between using a reference to thermogram, zoom spinner and image
    // thermogram: &'a RefCell<Option<Thermogram>>,
    image: Image,
    zoom_spinner: SpinButton,

    thermal_handler_id: Rc<RefCell<Option<SignalHandlerId>>>,
    optical_handler_id: Rc<RefCell<Option<SignalHandlerId>>>,
    on_change: Rc<RefCell<Option<Box<dyn Fn()>>>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum ImageryKind {
    Thermal,
    Optical,
    PictureInPicture,
}

impl ImageryToggles {
    pub fn from_builder(
        builder: Builder,
        _thermogram: &RefCell<Option<Thermogram>>,
    ) -> Rc<RefCell<ImageryToggles>> {
        let tool_showable_thermal = builder.object("imagery_thermal_toggle").unwrap();
        let tool_showable_optical = builder.object("imagery_optical_toggle").unwrap();

        let toggles = ImageryToggles {
            tool_showable_thermal,
            tool_showable_optical,
            // thermogram,
            image: builder.object("viewed_image").unwrap(),
            zoom_spinner: builder.object("zoom_spinner").unwrap(),

            thermal_handler_id: Rc::new(RefCell::new(None)),
            optical_handler_id: Rc::new(RefCell::new(None)),
            on_change: Rc::new(RefCell::new(None)),
        };
        let this = Rc::new(RefCell::new(toggles));
        Self::connect_signals(&this);

        this
    }

    pub fn kind(&self) -> ImageryKind {
        if self.tool_showable_optical.is_active() {
            ImageryKind::Optical
        } else {
            ImageryKind::Thermal
        }
    }

    pub fn set_on_change<F: Fn() + 'static>(&self, f: F) {
        *self.on_change.borrow_mut() = Some(Box::new(f));
    }

    fn connect_signals(this: &Rc<RefCell<Self>>) {
        {
            let that = this.clone();
            let handler_id = this.borrow().tool_showable_thermal.connect_toggled(move |button| {
                that.borrow().activate_toggle(button);
            });
            this.borrow().thermal_handler_id.replace(Some(handler_id));
        }
        {
            let that = this.clone();
            let handler_id = this.borrow().tool_showable_optical.connect_toggled(move |button| {
                that.borrow().activate_toggle(button);
            });
            this.borrow().optical_handler_id.replace(Some(handler_id));
        }
    }

    pub fn activate_toggle(&self, button: &ToggleButton) {
        if let Some(handler_id) = self.thermal_handler_id.borrow().as_ref() {
            self.tool_showable_thermal.block_signal(handler_id);
        };
        if let Some(handler_id) = self.optical_handler_id.borrow().as_ref() {
            self.tool_showable_optical.block_signal(handler_id);
        }

        self.tool_showable_thermal.set_active(false);
        self.tool_showable_optical.set_active(false);
        button.set_active(true);

        if let Some(handler_id) = self.thermal_handler_id.borrow().as_ref() {
            self.tool_showable_thermal.unblock_signal(handler_id);
        };

        if let Some(handler_id) = self.optical_handler_id.borrow().as_ref() {
            self.tool_showable_optical.unblock_signal(handler_id);
        };

        if let Some(f) = self.on_change.borrow().as_ref() {
            f();
        }
    }
}
