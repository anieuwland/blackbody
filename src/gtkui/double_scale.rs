//! `DoubleScale`: an Adwaita-styled slider with a single trough and two
//! handles, for choosing a closed value range.
//!
//! The widget borrows GtkScale's CSS node names (`scale > trough >
//! highlight/slider`), so the platform stylesheet paints it exactly like a
//! stock slider — same trough, same round handles, same hover and active
//! shades — including the accent-coloured highlight, which here spans the
//! gap between the two handles instead of running from the track's origin.
//!
//! Interaction mirrors GtkScale where a second handle makes sense: press or
//! drag anywhere on the track to move the nearest handle, scroll to nudge
//! it, and Tab reaches each handle so arrow keys can adjust it. Handles push
//! each other along when they collide, so min <= max always holds.
//! Programmatic setters are silent; only user interaction notifies.

use std::cell::{Cell, OnceCell, RefCell};

use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{Allocation, Orientation};

/// Pointer presses closer than this to a handle's center pick the handle up
/// where it is; farther presses warp it to the pointer first. Half the
/// handle's 20px visual diameter.
const GRAB_RADIUS: f64 = 10.0;
/// Pointer drags round values to this quantum, mirroring the 0.1 rounding a
/// GtkScale derives from a 0.5 step.
const DRAG_QUANTUM: f64 = 0.1;
/// Keyboard arrows and the scroll wheel move a handle by this much.
const STEP: f64 = 0.5;
/// PageUp/PageDown move a handle by this much.
const PAGE: f64 = 5.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Handle {
    Min,
    Max,
}

mod imp {
    use super::*;

    pub(in crate::gtkui) struct DoubleScale {
        pub(super) trough: OnceCell<super::Trough>,
        pub(super) lower: Cell<f64>,
        pub(super) upper: Cell<f64>,
        pub(super) min_value: Cell<f64>,
        pub(super) max_value: Cell<f64>,
        pub(super) dragging: Cell<Option<Handle>>,
        /// Pointer distance from the grabbed handle's center at press time,
        /// so a grabbed handle doesn't jump under the pointer.
        pub(super) grab_offset: Cell<f64>,
        /// Last pointer x, for aiming scroll-wheel nudges. NaN before the
        /// first motion event.
        pub(super) pointer_x: Cell<f64>,
        pub(super) on_value_changed: RefCell<Option<Box<dyn Fn(&super::DoubleScale)>>>,
        pub(super) on_drag_changed: RefCell<Option<Box<dyn Fn(&super::DoubleScale)>>>,
    }

    impl Default for DoubleScale {
        fn default() -> Self {
            DoubleScale {
                trough: OnceCell::new(),
                lower: Cell::new(-20.0),
                upper: Cell::new(40.0),
                min_value: Cell::new(0.0),
                max_value: Cell::new(20.0),
                dragging: Cell::new(None),
                grab_offset: Cell::new(0.0),
                pointer_x: Cell::new(f64::NAN),
                on_value_changed: RefCell::new(None),
                on_drag_changed: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DoubleScale {
        const NAME: &'static str = "BbDoubleScale";
        type Type = super::DoubleScale;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            // Borrow GtkScale's node name so the platform stylesheet applies.
            klass.set_css_name("scale");
            klass.set_accessible_role(gtk4::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for DoubleScale {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("horizontal");

            let trough = super::Trough::new();
            trough.set_parent(&*obj);
            // Pointer events belong to the scale itself; the internal nodes
            // are visuals only.
            trough.set_can_target(false);
            self.trough.set(trough).unwrap();

            obj.setup_controllers();
            obj.update_accessible_values();
        }

        fn dispose(&self) {
            if let Some(trough) = self.trough.get() {
                trough.unparent();
            }
        }
    }

    impl WidgetImpl for DoubleScale {
        fn measure(&self, orientation: Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match self.trough.get() {
                Some(trough) => {
                    let (min, nat, _, _) = trough.measure(orientation, for_size);
                    (min, nat, -1, -1)
                }
                None => (0, 0, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let Some(trough) = self.trough.get() else { return };
            // Center the trough vertically at its natural height, like GtkRange.
            let (trough_height, _, _, _) = trough.measure(Orientation::Vertical, -1);
            let y = (height - trough_height) / 2;
            trough.size_allocate(&Allocation::new(0, y, width, trough_height), -1);
        }
    }
}

glib::wrapper! {
    pub(super) struct DoubleScale(ObjectSubclass<imp::DoubleScale>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl DoubleScale {
    pub(super) fn new() -> DoubleScale {
        glib::Object::new()
    }

    /// Reset the track's extremes; values are clamped into them. Silent.
    pub(super) fn set_bounds(&self, lower: f64, upper: f64) {
        if !lower.is_finite() || !upper.is_finite() || upper <= lower {
            return;
        }
        let imp = self.imp();
        imp.lower.set(lower);
        imp.upper.set(upper);
        let min = imp.min_value.get().clamp(lower, upper);
        let max = imp.max_value.get().clamp(lower, upper).max(min);
        imp.min_value.set(min);
        imp.max_value.set(max);
        self.refresh();
    }

    /// Move both handles; clamped into the bounds and ordered. Silent.
    pub(super) fn set_values(&self, min: f64, max: f64) {
        let imp = self.imp();
        let (lower, upper) = (imp.lower.get(), imp.upper.get());
        let min = min.clamp(lower, upper);
        let max = max.clamp(lower, upper).max(min);
        imp.min_value.set(min);
        imp.max_value.set(max);
        self.refresh();
    }

    pub(super) fn lower(&self) -> f64 {
        self.imp().lower.get()
    }

    pub(super) fn upper(&self) -> f64 {
        self.imp().upper.get()
    }

    pub(super) fn min_value(&self) -> f64 {
        self.imp().min_value.get()
    }

    pub(super) fn max_value(&self) -> f64 {
        self.imp().max_value.get()
    }

    /// The handle currently held by a pointer drag, if any.
    pub(super) fn dragging(&self) -> Option<Handle> {
        self.imp().dragging.get()
    }

    /// A handle's center x in the widget's own coordinates, once allocated.
    pub(super) fn handle_center(&self, handle: Handle) -> Option<f64> {
        let (track_x, usable, half_slider) = self.track_geometry()?;
        Some(track_x + half_slider + self.fraction(self.value(handle)) * usable)
    }

    /// `f` runs on every user-initiated value change (drag, keys, scroll).
    pub(super) fn connect_value_changed(&self, f: impl Fn(&DoubleScale) + 'static) {
        *self.imp().on_value_changed.borrow_mut() = Some(Box::new(f));
    }

    /// `f` runs when a pointer drag starts or ends; query `dragging`.
    pub(super) fn connect_drag_changed(&self, f: impl Fn(&DoubleScale) + 'static) {
        *self.imp().on_drag_changed.borrow_mut() = Some(Box::new(f));
    }

    fn value(&self, handle: Handle) -> f64 {
        match handle {
            Handle::Min => self.imp().min_value.get(),
            Handle::Max => self.imp().max_value.get(),
        }
    }

    /// A value's position on the track as a 0..=1 fraction, flipped for RTL.
    fn fraction(&self, value: f64) -> f64 {
        let (lower, upper) = (self.lower(), self.upper());
        let range = upper - lower;
        if range <= 0.0 {
            return 0.0;
        }
        let f = ((value - lower) / range).clamp(0.0, 1.0);
        if self.direction() == gtk4::TextDirection::Rtl { 1.0 - f } else { f }
    }

    /// The track in widget coordinates: the trough's left edge, the distance
    /// a handle's center can travel, and half the handle's layout width.
    fn track_geometry(&self) -> Option<(f64, f64, f64)> {
        let trough = self.imp().trough.get()?;
        let bounds = trough.compute_bounds(self)?;
        let slider = trough.slider(Handle::Min).measure(Orientation::Horizontal, -1).0 as f64;
        let usable = (bounds.width() as f64 - slider).max(0.0);
        Some((bounds.x() as f64, usable, slider / 2.0))
    }

    /// The value whose handle center would sit at x (widget coordinates).
    fn value_at(&self, x: f64) -> f64 {
        let (lower, upper) = (self.lower(), self.upper());
        let Some((track_x, usable, half_slider)) = self.track_geometry() else { return lower };
        if usable <= 0.0 {
            return lower;
        }
        let mut f = ((x - track_x - half_slider) / usable).clamp(0.0, 1.0);
        if self.direction() == gtk4::TextDirection::Rtl {
            f = 1.0 - f;
        }
        lower + f * (upper - lower)
    }

    /// Equidistant (e.g. coinciding handles): the side approached decides,
    /// so a coinciding pair can always be pulled apart.
    fn nearest_handle(&self, x: f64) -> Handle {
        let (Some(min_x), Some(max_x)) =
            (self.handle_center(Handle::Min), self.handle_center(Handle::Max))
        else {
            return Handle::Max;
        };
        let (d_min, d_max) = ((x - min_x).abs(), (x - max_x).abs());
        if d_max != d_min {
            if d_max < d_min { Handle::Max } else { Handle::Min }
        } else if x > min_x {
            Handle::Max
        } else {
            Handle::Min
        }
    }

    fn setup_controllers(&self) {
        let drag = gtk4::GestureDrag::new();
        drag.connect_drag_begin(|gesture, x, _| {
            if let Some(scale) = scale_of(gesture) {
                scale.begin_drag(gesture, x);
            }
        });
        drag.connect_drag_update(|gesture, offset_x, _| {
            if let (Some(scale), Some((start_x, _))) = (scale_of(gesture), gesture.start_point()) {
                scale.continue_drag(start_x + offset_x);
            }
        });
        drag.connect_drag_end(|gesture, _, _| {
            if let Some(scale) = scale_of(gesture) {
                scale.end_drag();
            }
        });
        self.add_controller(drag);

        // Scroll targets whichever handle is nearest the pointer, so keep
        // track of where the pointer is.
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion(|controller, x, _| {
            if let Some(scale) = scale_of(controller) {
                scale.imp().pointer_x.set(x);
            }
        });
        self.add_controller(motion);

        let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::BOTH_AXES);
        scroll.connect_scroll(|controller, dx, dy| match scale_of(controller) {
            Some(scale) => scale.scroll_by(if dx.abs() > dy.abs() { dx } else { -dy }),
            None => glib::Propagation::Proceed,
        });
        self.add_controller(scroll);

        for handle in [Handle::Min, Handle::Max] {
            let key = gtk4::EventControllerKey::new();
            key.connect_key_pressed(move |controller, keyval, _, _| {
                let scale = controller
                    .widget()
                    .and_then(|w| w.ancestor(DoubleScale::static_type()))
                    .and_downcast::<DoubleScale>();
                match scale {
                    Some(scale) => scale.key_adjust(handle, keyval),
                    None => glib::Propagation::Proceed,
                }
            });
            if let Some(trough) = self.imp().trough.get() {
                trough.slider(handle).add_controller(key);
            }
        }
    }

    fn begin_drag(&self, gesture: &gtk4::GestureDrag, x: f64) {
        gesture.set_state(gtk4::EventSequenceState::Claimed);
        let imp = self.imp();
        let handle = self.nearest_handle(x);
        imp.dragging.set(Some(handle));
        let center = self.handle_center(handle).unwrap_or(x);
        if (x - center).abs() <= GRAB_RADIUS {
            imp.grab_offset.set(x - center);
        } else {
            imp.grab_offset.set(0.0);
            self.set_value_interactive(handle, quantize(self.value_at(x)));
        }
        self.set_state_flags(gtk4::StateFlags::ACTIVE, false);
        if self.gets_focus_on_click() {
            if let Some(trough) = imp.trough.get() {
                trough.slider(handle).grab_focus();
            }
        }
        self.notify_drag();
    }

    fn continue_drag(&self, x: f64) {
        let imp = self.imp();
        let Some(handle) = imp.dragging.get() else { return };
        let value = self.value_at(x - imp.grab_offset.get());
        self.set_value_interactive(handle, quantize(value));
    }

    fn end_drag(&self) {
        self.imp().dragging.set(None);
        self.unset_state_flags(gtk4::StateFlags::ACTIVE);
        self.notify_drag();
    }

    fn scroll_by(&self, direction: f64) -> glib::Propagation {
        if direction == 0.0 {
            return glib::Propagation::Proceed;
        }
        let x = self.imp().pointer_x.get();
        let handle = if x.is_nan() { Handle::Max } else { self.nearest_handle(x) };
        self.set_value_interactive(handle, self.value(handle) + direction * STEP);
        glib::Propagation::Stop
    }

    fn key_adjust(&self, handle: Handle, keyval: Key) -> glib::Propagation {
        let current = self.value(handle);
        let target = match keyval {
            Key::Left | Key::KP_Left | Key::Down | Key::KP_Down => current - STEP,
            Key::Right | Key::KP_Right | Key::Up | Key::KP_Up => current + STEP,
            Key::Page_Down | Key::KP_Page_Down => current - PAGE,
            Key::Page_Up | Key::KP_Page_Up => current + PAGE,
            Key::Home | Key::KP_Home => self.lower(),
            Key::End | Key::KP_End => self.upper(),
            _ => return glib::Propagation::Proceed,
        };
        self.set_value_interactive(handle, target);
        glib::Propagation::Stop
    }

    /// Apply a user-initiated value: clamp into the bounds, push the other
    /// handle along when crossing it, notify when anything moved.
    fn set_value_interactive(&self, handle: Handle, value: f64) {
        let imp = self.imp();
        let value = value.clamp(imp.lower.get(), imp.upper.get());
        let (old_min, old_max) = (imp.min_value.get(), imp.max_value.get());
        match handle {
            Handle::Min => {
                imp.min_value.set(value);
                imp.max_value.set(old_max.max(value));
            }
            Handle::Max => {
                imp.max_value.set(value);
                imp.min_value.set(old_min.min(value));
            }
        }
        if imp.min_value.get() != old_min || imp.max_value.get() != old_max {
            self.refresh();
            if let Some(cb) = imp.on_value_changed.borrow().as_ref() {
                cb(self);
            }
        }
    }

    fn refresh(&self) {
        if let Some(trough) = self.imp().trough.get() {
            trough.queue_allocate();
        }
        self.update_accessible_values();
    }

    fn update_accessible_values(&self) {
        let Some(trough) = self.imp().trough.get() else { return };
        for handle in [Handle::Min, Handle::Max] {
            trough.slider(handle).update_property(&[
                gtk4::accessible::Property::ValueMin(self.lower()),
                gtk4::accessible::Property::ValueMax(self.upper()),
                gtk4::accessible::Property::ValueNow(self.value(handle)),
            ]);
        }
    }

    fn notify_drag(&self) {
        if let Some(cb) = self.imp().on_drag_changed.borrow().as_ref() {
            cb(self);
        }
    }
}

/// The widget a controller is attached to, as a `DoubleScale`.
fn scale_of(controller: &impl IsA<gtk4::EventController>) -> Option<DoubleScale> {
    controller.as_ref().widget().and_downcast::<DoubleScale>()
}

fn quantize(value: f64) -> f64 {
    (value / DRAG_QUANTUM).round() * DRAG_QUANTUM
}

mod trough_imp {
    use super::*;

    #[derive(Default)]
    pub(in crate::gtkui) struct Trough {
        pub(super) highlight: OnceCell<super::Highlight>,
        pub(super) slider_min: OnceCell<super::SliderHandle>,
        pub(super) slider_max: OnceCell<super::SliderHandle>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Trough {
        const NAME: &'static str = "BbDoubleScaleTrough";
        type Type = super::Trough;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("trough");
            klass.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Trough {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            let highlight = super::Highlight::new();
            let slider_min = super::SliderHandle::new();
            let slider_max = super::SliderHandle::new();
            // Paint order is child order: the highlight goes below the handles.
            for widget in [
                highlight.upcast_ref::<gtk4::Widget>(),
                slider_min.upcast_ref(),
                slider_max.upcast_ref(),
            ] {
                widget.set_parent(&*obj);
                widget.set_can_target(false);
            }
            for slider in [&slider_min, &slider_max] {
                slider.set_focusable(true);
            }
            self.highlight.set(highlight).unwrap();
            self.slider_min.set(slider_min).unwrap();
            self.slider_max.set(slider_max).unwrap();
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Trough {
        fn measure(&self, orientation: Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let mut min = 0;
            let mut nat = 0;
            let mut child = self.obj().first_child();
            while let Some(c) = child {
                let (child_min, child_nat, _, _) = c.measure(orientation, -1);
                min = min.max(child_min);
                nat = nat.max(child_nat);
                child = c.next_sibling();
            }
            if orientation == Orientation::Horizontal {
                // At the very least, room for the two handles side by side.
                min *= 2;
            }
            (min, nat.max(min), -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let Some(scale) = obj.parent().and_downcast::<super::DoubleScale>() else { return };
            let (Some(highlight), Some(slider_min), Some(slider_max)) =
                (self.highlight.get(), self.slider_min.get(), self.slider_max.get())
            else {
                return;
            };

            let slider_w = slider_min.measure(Orientation::Horizontal, -1).0;
            let slider_h = slider_min.measure(Orientation::Vertical, -1).0;
            let usable = (width - slider_w).max(0) as f64;
            let y = (height - slider_h) / 2;

            let mut centers = [0; 2];
            for (i, (slider, handle)) in
                [(slider_min, Handle::Min), (slider_max, Handle::Max)].into_iter().enumerate()
            {
                let x = (scale.fraction(scale.value(handle)) * usable).round() as i32;
                slider.size_allocate(&Allocation::new(x, y, slider_w, slider_h), -1);
                centers[i] = x + slider_w / 2;
            }

            // The accent-coloured span between the two handle centers.
            let (left, right) = (centers[0].min(centers[1]), centers[0].max(centers[1]));
            highlight.size_allocate(&Allocation::new(left, 0, (right - left).max(1), height), -1);
        }
    }
}

glib::wrapper! {
    pub(super) struct Trough(ObjectSubclass<trough_imp::Trough>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Trough {
    fn new() -> Trough {
        glib::Object::new()
    }

    fn slider(&self, handle: Handle) -> &SliderHandle {
        let imp = self.imp();
        let cell = match handle {
            Handle::Min => &imp.slider_min,
            Handle::Max => &imp.slider_max,
        };
        cell.get().expect("sliders are created in constructed")
    }
}

mod slider_imp {
    use super::*;

    #[derive(Default)]
    pub(in crate::gtkui) struct SliderHandle;

    #[glib::object_subclass]
    impl ObjectSubclass for SliderHandle {
        const NAME: &'static str = "BbDoubleScaleSlider";
        type Type = super::SliderHandle;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("slider");
            klass.set_accessible_role(gtk4::AccessibleRole::Slider);
        }
    }

    impl ObjectImpl for SliderHandle {}

    impl WidgetImpl for SliderHandle {
        fn measure(&self, _orientation: Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            // The stylesheet's min-width/min-height provide the real size.
            (0, 0, -1, -1)
        }
    }
}

glib::wrapper! {
    pub(super) struct SliderHandle(ObjectSubclass<slider_imp::SliderHandle>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SliderHandle {
    fn new() -> SliderHandle {
        glib::Object::new()
    }
}

mod highlight_imp {
    use super::*;

    #[derive(Default)]
    pub(in crate::gtkui) struct Highlight;

    #[glib::object_subclass]
    impl ObjectSubclass for Highlight {
        const NAME: &'static str = "BbDoubleScaleHighlight";
        type Type = super::Highlight;
        type ParentType = gtk4::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("highlight");
            klass.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for Highlight {}

    impl WidgetImpl for Highlight {
        fn measure(&self, _orientation: Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }
    }
}

glib::wrapper! {
    pub(super) struct Highlight(ObjectSubclass<highlight_imp::Highlight>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Highlight {
    fn new() -> Highlight {
        glib::Object::new()
    }
}
