//! A single on-screen widget for choosing the rendered temperature range:
//! two full-width `GtkScale`s overlaid on one trough, so both handles ride
//! the same track and push each other along when they meet. The absolute
//! extremes of the track are shown as editable labels on either side, and
//! the value of a handle appears in a bubble above it only while dragging.
//!
//! Pointer routing: only one of the overlaid scales is targetable at a time;
//! a motion controller on the overlay retargets whichever handle is nearest
//! to the pointer. (A touch press without prior motion goes to the most
//! recently targeted handle.)

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{
    EditableLabel, EventControllerLegacy, EventControllerMotion, Label, Orientation,
    PropagationPhase, Scale,
};

use crate::domain::units::TempUnit;

/// Extra draggable room beyond the thermogram's own range, in celsius.
/// The user can widen it further by editing the extreme labels.
const RANGE_MARGIN: f32 = 20.0;
/// Gap between the bubble's bottom edge and the top of the scales, in pixels.
const BUBBLE_GAP: f64 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    Min,
    Max,
}

pub(super) struct RangeSlider {
    root: gtk4::Box,
    overlay: gtk4::Overlay,
    min_scale: Scale,
    max_scale: Scale,
    min_edit: EditableLabel,
    max_edit: EditableLabel,
    bubble: Label,
    /// External overlay the bubble floats in (the widget itself is too short
    /// to show a bubble above the trough). Set via `attach_bubble`.
    bubble_host: RefCell<Option<gtk4::Overlay>>,
    unit: Cell<TempUnit>,
    dragging: Cell<Option<Handle>>,
    /// Silences `on_changed` while `configure` moves the values around.
    muted: Cell<bool>,
    on_changed: RefCell<Option<Box<dyn Fn(f32, f32)>>>,
}

impl RangeSlider {
    pub(super) fn new() -> Rc<RangeSlider> {
        install_css();

        let min_scale = Scale::with_range(Orientation::Horizontal, -20.0, 40.0, 0.5);
        let max_scale = Scale::with_range(Orientation::Horizontal, -20.0, 40.0, 0.5);
        for scale in [&min_scale, &max_scale] {
            scale.set_draw_value(false);
            scale.set_hexpand(true);
        }
        // The top scale's trough is transparent (CSS) so only its handle shows.
        min_scale.add_css_class("bb-range-bottom");
        max_scale.add_css_class("bb-range-top");
        min_scale.set_value(0.0);
        max_scale.set_value(20.0);

        let overlay = gtk4::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_child(Some(&min_scale));
        overlay.add_overlay(&max_scale);

        let min_edit = EditableLabel::new("");
        let max_edit = EditableLabel::new("");
        min_edit.set_width_chars(8);
        max_edit.set_width_chars(8);
        min_edit.set_alignment(0.0);
        max_edit.set_alignment(1.0);
        min_edit.set_margin_start(8);
        max_edit.set_margin_end(8);
        for edit in [&min_edit, &max_edit] {
            edit.set_valign(gtk4::Align::Center);
            edit.set_tooltip_text(Some(&gettextrs::gettext("Click to edit the slider's extreme")));
        }

        let bubble = Label::new(None);
        bubble.add_css_class("bb-range-bubble");
        bubble.set_halign(gtk4::Align::Start);
        bubble.set_valign(gtk4::Align::End);
        bubble.set_visible(false);
        bubble.set_can_target(false);

        let root = gtk4::Box::new(Orientation::Horizontal, 6);
        root.set_hexpand(true);
        root.append(&min_edit);
        root.append(&overlay);
        root.append(&max_edit);

        let this = Rc::new(RangeSlider {
            root,
            overlay,
            min_scale,
            max_scale,
            min_edit,
            max_edit,
            bubble,
            bubble_host: RefCell::new(None),
            unit: Cell::new(TempUnit::default()),
            dragging: Cell::new(None),
            muted: Cell::new(false),
            on_changed: RefCell::new(None),
        });
        this.refresh_bound_labels();
        Self::connect_scales(&this);
        Self::connect_retargeting(&this);
        Self::connect_edits(&this);
        this
    }

    /// The root widget, to be packed into the OSD's range bar.
    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    /// The two overlaid scales, for app-level tweaks like focus handling.
    pub(super) fn scales(&self) -> [&Scale; 2] {
        [&self.min_scale, &self.max_scale]
    }

    /// The overlay the drag bubble floats in — typically the canvas overlay,
    /// which provides room above the OSD. Call once during setup.
    pub(super) fn attach_bubble(&self, host: &gtk4::Overlay) {
        host.add_overlay(&self.bubble);
        *self.bubble_host.borrow_mut() = Some(host.clone());
    }

    pub(super) fn connect_changed(&self, f: impl Fn(f32, f32) + 'static) {
        *self.on_changed.borrow_mut() = Some(Box::new(f));
    }

    /// Reset the track to the thermogram's range plus a margin on both ends
    /// and put the handles at the range's min and max. Does not notify.
    pub(super) fn configure(&self, min: f32, max: f32) {
        self.muted.set(true);
        for scale in [&self.min_scale, &self.max_scale] {
            let adj = scale.adjustment();
            adj.set_lower((min - RANGE_MARGIN) as f64);
            adj.set_upper((max + RANGE_MARGIN) as f64);
        }
        self.min_scale.set_value(min as f64);
        self.max_scale.set_value(max as f64);
        self.muted.set(false);
        self.refresh_bound_labels();
    }

    /// Change the display unit and reformat the extreme labels. Temperatures
    /// stay in celsius internally.
    pub(super) fn set_unit(&self, unit: TempUnit) {
        self.unit.set(unit);
        self.refresh_bound_labels();
    }

    /// Track extremes in celsius, shared by both adjustments.
    fn bounds(&self) -> (f64, f64) {
        let adj = self.min_scale.adjustment();
        (adj.lower(), adj.upper())
    }

    fn refresh_bound_labels(&self) {
        let (lo, hi) = self.bounds();
        let unit = self.unit.get();
        self.min_edit.set_text(&unit.format(lo as f32));
        self.max_edit.set_text(&unit.format(hi as f32));
    }

    fn emit_changed(&self) {
        if self.muted.get() {
            return;
        }
        if let Some(cb) = self.on_changed.borrow().as_ref() {
            cb(self.min_scale.value() as f32, self.max_scale.value() as f32);
        }
    }

    fn connect_scales(this: &Rc<Self>) {
        let pairs = [(this.min_scale.clone(), Handle::Min), (this.max_scale.clone(), Handle::Max)];
        for (scale, handle) in pairs {
            let that = this.clone();
            scale.connect_value_changed(move |scale| that.on_value_changed(scale, handle));

            // Show the bubble only while the pointer holds a handle. A raw
            // event observer, because GtkRange's internal drag gesture claims
            // the sequence and would cancel an observing GestureClick.
            let that = this.clone();
            let observed = scale.clone();
            let press = EventControllerLegacy::new();
            press.set_propagation_phase(PropagationPhase::Capture);
            press.connect_event(move |_, event| {
                use gtk4::gdk::EventType::*;
                match event.event_type() {
                    ButtonPress | TouchBegin => that.begin_drag(handle, &observed),
                    ButtonRelease | TouchEnd => that.end_drag(),
                    _ => {}
                }
                glib::Propagation::Proceed
            });
            scale.add_controller(press);
        }
    }

    fn on_value_changed(&self, scale: &Scale, handle: Handle) {
        // Push the other handle along when crossing it, so min <= max always.
        match handle {
            Handle::Min if scale.value() > self.max_scale.value() => {
                self.max_scale.set_value(scale.value());
            }
            Handle::Max if scale.value() < self.min_scale.value() => {
                self.min_scale.set_value(scale.value());
            }
            _ => {}
        }
        if self.dragging.get() == Some(handle) {
            self.update_bubble(scale);
        }
        self.emit_changed();
    }

    fn begin_drag(&self, handle: Handle, scale: &Scale) {
        self.dragging.set(Some(handle));
        self.bubble.set_visible(true);
        self.update_bubble(scale);
    }

    fn end_drag(&self) {
        self.dragging.set(None);
        self.bubble.set_visible(false);
    }

    /// Place the bubble in the host overlay, centered above the handle.
    fn update_bubble(&self, scale: &Scale) {
        let host = self.bubble_host.borrow().clone();
        let Some(host) = host else { return };
        let x_in_scale = handle_center_x(scale);
        let point = gtk4::graphene::Point::new(x_in_scale as f32, 0.0);
        let Some(point) = scale.compute_point(&host, &point) else { return };

        self.bubble.set_text(&self.unit.get().format(scale.value() as f32));
        let (_, natural) = self.bubble.preferred_size();
        let width = natural.width() as f64;
        let left =
            (point.x() as f64 - width / 2.0).clamp(0.0, (host.width() as f64 - width).max(0.0));
        self.bubble.set_margin_start(left as i32);
        self.bubble
            .set_margin_bottom((host.height() as f64 - point.y() as f64 + BUBBLE_GAP) as i32);
    }

    /// Route pointer input to whichever scale's handle is nearest, since the
    /// top scale would otherwise swallow every event. Frozen during a drag:
    /// GTK's implicit grab keeps events flowing to the grabbed scale anyway.
    fn connect_retargeting(this: &Rc<Self>) {
        let that = this.clone();
        let motion = EventControllerMotion::new();
        motion.set_propagation_phase(PropagationPhase::Capture);
        motion.connect_motion(move |_, x, _| {
            if that.dragging.get().is_some() {
                return;
            }
            let min_x = handle_center_x(&that.min_scale);
            let max_x = handle_center_x(&that.max_scale);
            let (d_min, d_max) = ((x - min_x).abs(), (x - max_x).abs());
            // Equidistant (e.g. coinciding handles): the side approached
            // decides, so a coinciding pair can always be pulled apart.
            let pick_max = if d_max != d_min { d_max < d_min } else { x > min_x };
            that.min_scale.set_can_target(!pick_max);
            that.max_scale.set_can_target(pick_max);
        });
        this.overlay.add_controller(motion);
    }

    fn connect_edits(this: &Rc<Self>) {
        let pairs = [(this.min_edit.clone(), Handle::Min), (this.max_edit.clone(), Handle::Max)];
        for (edit, handle) in pairs {
            let that = this.clone();
            edit.connect_editing_notify(move |edit| {
                if edit.is_editing() {
                    return;
                }
                that.commit_bound(handle, &edit.text());
            });
        }
    }

    /// Apply an edited extreme. Invalid input (unparseable, or on the wrong
    /// side of the opposite extreme) reverts the label. Handles outside the
    /// new extreme are clamped by the adjustment, which notifies normally.
    fn commit_bound(&self, handle: Handle, text: &str) {
        let (lo, hi) = self.bounds();
        if let Some(value) = parse_temp(text, self.unit.get()) {
            let value = value as f64;
            match handle {
                Handle::Min if value < hi => {
                    for scale in [&self.min_scale, &self.max_scale] {
                        scale.adjustment().set_lower(value);
                    }
                }
                Handle::Max if value > lo => {
                    for scale in [&self.min_scale, &self.max_scale] {
                        scale.adjustment().set_upper(value);
                    }
                }
                _ => {}
            }
        }
        self.refresh_bound_labels();
    }
}

/// The handle's center x in the scale's own coordinates (the overlay's too:
/// the scales fill it exactly).
fn handle_center_x(scale: &Scale) -> f64 {
    let adj = scale.adjustment();
    let rect = scale.range_rect();
    let range = adj.upper() - adj.lower();
    if range <= 0.0 {
        return rect.x() as f64 + rect.width() as f64 / 2.0;
    }
    let fraction = (adj.value() - adj.lower()) / range;
    rect.x() as f64 + fraction * rect.width() as f64
}

/// Parse user input like "12.5", "-40", "20,5 °C" or "296 K" in the given
/// display unit; returns celsius.
fn parse_temp(text: &str, unit: TempUnit) -> Option<f32> {
    let text = text.trim().trim_end_matches(|c: char| c.is_alphabetic() || c == '°').trim();
    text.replace(',', ".").parse::<f32>().ok().map(|v| unit.to_celsius(v))
}

/// Hide the top scale's trough (only its handle shows) and both highlights:
/// a left-to-handle fill makes no sense on a two-handle track.
fn install_css() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            "scale.bb-range-top trough {
                 background-color: transparent;
                 background-image: none;
                 border-color: transparent;
                 box-shadow: none;
             }
             scale.bb-range-top highlight,
             scale.bb-range-bottom highlight {
                 background-color: transparent;
                 background-image: none;
             }
             label.bb-range-bubble {
                 background-color: rgba(0, 0, 0, 0.75);
                 color: white;
                 border-radius: 6px;
                 padding: 2px 8px;
             }",
        );
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().unwrap(),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

#[cfg(test)]
mod tests {
    use crate::domain::units::TempUnit;

    use super::parse_temp;

    #[test]
    fn parses_plain_and_suffixed_input() {
        assert_eq!(parse_temp("12.5", TempUnit::Celsius), Some(12.5));
        assert_eq!(parse_temp(" -40 ", TempUnit::Celsius), Some(-40.0));
        assert_eq!(parse_temp("20,5 °C", TempUnit::Celsius), Some(20.5));
        assert_eq!(parse_temp("273.15 K", TempUnit::Kelvin), Some(0.0));
        assert_eq!(parse_temp("32 °F", TempUnit::Fahrenheit), Some(0.0));
        assert_eq!(parse_temp("garbage", TempUnit::Celsius), None);
        assert_eq!(parse_temp("", TempUnit::Celsius), None);
    }
}
