//! The on-screen-display fade: the control bar over the canvas appears on
//! mouse motion and fades out after a few seconds of inactivity.

use std::rc::Rc;

use gtk4::EventControllerMotion;
use gtk4::prelude::*;
use libadwaita::prelude::*;

use super::app_window::AppState;

impl AppState {
    pub(super) fn connect_osd(this: &Rc<Self>) {
        // Start hidden and click-through until a thermogram is shown.
        let osd = &this.ui.osd;
        osd.container.set_opacity(0.0);
        osd.container.set_can_target(false);
        let container = osd.container.downgrade();
        osd.hide_anim.connect_done(move |_| {
            if let Some(c) = container.upgrade() {
                c.set_can_target(false);
            }
        });

        // Show on mouse motion, hide after idle.
        let that = this.clone();
        let osd_motion = EventControllerMotion::new();
        osd_motion.connect_motion(move |_, _, _| {
            that.show_osd();
        });
        this.ui.canvas.overlay.add_controller(osd_motion);
    }

    pub(super) fn show_osd(&self) {
        if self.current_surface().is_none() {
            return;
        }
        let osd = &self.ui.osd;
        if let Some(id) = osd.hide_source.replace(None) {
            id.remove();
        }
        osd.hide_anim.pause();
        osd.show_anim.set_value_from(osd.container.opacity());
        osd.container.set_can_target(true);
        osd.show_anim.play();
        self.schedule_osd_hide(std::time::Duration::from_secs(3));
    }

    fn schedule_osd_hide(&self, delay: std::time::Duration) {
        let osd = &self.ui.osd;
        if let Some(id) = osd.hide_source.replace(None) {
            id.remove();
        }
        let shared = Rc::clone(&osd.hide_source);
        let show_anim = osd.show_anim.clone();
        let hide_anim = osd.hide_anim.clone();
        let container = osd.container.downgrade();
        let id = glib::timeout_add_local(delay, move || {
            shared.replace(None);
            show_anim.pause();
            hide_anim.set_value_from(container.upgrade().map(|c| c.opacity()).unwrap_or(1.0));
            hide_anim.play();
            glib::ControlFlow::Break
        });
        osd.hide_source.set(Some(id));
    }
}
