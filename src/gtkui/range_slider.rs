//! A single on-screen widget for choosing the rendered temperature range:
//! a `DoubleScale` — one Adwaita-styled trough carrying two handles, with
//! the span between them drawn in the accent colour. The absolute extremes
//! of the track are shown as editable labels on either side, and the value
//! of a handle appears in a bubble above it only while dragging.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{EditableLabel, Label, Orientation};
use uom::si::f32::{TemperatureInterval, ThermodynamicTemperature};
use uom::si::temperature_interval;
use uom::si::thermodynamic_temperature::kelvin;

use super::double_scale::{DoubleScale, Handle};
use crate::domain::units::TempUnit;

/// Extra draggable room beyond the thermogram's own range. An interval, not
/// a temperature: it widens the track on both ends. The user can widen it
/// further by editing the extreme labels.
fn range_margin() -> TemperatureInterval {
    TemperatureInterval::new::<temperature_interval::kelvin>(20.0)
}
/// Gap between the bubble's bottom edge and the top of the slider, in pixels.
const BUBBLE_GAP: f64 = 8.0;

/// Callback invoked when either handle moves, with the new (min, max).
type ChangedCallback = Box<dyn Fn(ThermodynamicTemperature, ThermodynamicTemperature)>;

pub(super) struct RangeSlider {
    root: gtk4::Box,
    scale: DoubleScale,
    min_edit: EditableLabel,
    max_edit: EditableLabel,
    bubble: Label,
    /// External overlay the bubble floats in (the widget itself is too short
    /// to show a bubble above the trough). Set via `attach_bubble`.
    bubble_host: RefCell<Option<gtk4::Overlay>>,
    unit: Cell<TempUnit>,
    on_changed: RefCell<Option<ChangedCallback>>,
}

/// The `DoubleScale` carries plain `f64`s; fix their meaning as kelvin at
/// this boundary so no other code needs to know.
fn to_scale(t: ThermodynamicTemperature) -> f64 {
    t.get::<kelvin>() as f64
}

fn from_scale(v: f64) -> ThermodynamicTemperature {
    ThermodynamicTemperature::new::<kelvin>(v as f32)
}

impl RangeSlider {
    pub(super) fn new() -> Rc<RangeSlider> {
        install_css();

        let scale = DoubleScale::new();
        scale.set_hexpand(true);
        scale.add_css_class("bb-range");

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
        root.append(&scale);
        root.append(&max_edit);

        let this = Rc::new(RangeSlider {
            root,
            scale,
            min_edit,
            max_edit,
            bubble,
            bubble_host: RefCell::new(None),
            unit: Cell::new(TempUnit::default()),
            on_changed: RefCell::new(None),
        });
        this.refresh_bound_labels();
        Self::connect_scale(&this);
        Self::connect_edits(&this);
        this
    }

    /// The root widget, to be packed into the OSD's range bar.
    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    /// The slider itself, for app-level tweaks like focus handling.
    pub(super) fn scale_widget(&self) -> &gtk4::Widget {
        self.scale.upcast_ref()
    }

    /// The overlay the drag bubble floats in — typically the canvas overlay,
    /// which provides room above the OSD. Call once during setup.
    pub(super) fn attach_bubble(&self, host: &gtk4::Overlay) {
        host.add_overlay(&self.bubble);
        *self.bubble_host.borrow_mut() = Some(host.clone());
    }

    pub(super) fn connect_changed(
        &self,
        f: impl Fn(ThermodynamicTemperature, ThermodynamicTemperature) + 'static,
    ) {
        *self.on_changed.borrow_mut() = Some(Box::new(f));
    }

    /// Reset the track to the thermogram's range plus a margin on both ends
    /// and put the handles at the range's min and max. Does not notify.
    pub(super) fn configure(&self, min: ThermodynamicTemperature, max: ThermodynamicTemperature) {
        self.scale.set_bounds(to_scale(min - range_margin()), to_scale(max + range_margin()));
        self.scale.set_values(to_scale(min), to_scale(max));
        self.refresh_bound_labels();
    }

    /// Change the display unit and reformat the extreme labels. Temperatures
    /// stay typed internally.
    pub(super) fn set_unit(&self, unit: TempUnit) {
        self.unit.set(unit);
        self.refresh_bound_labels();
    }

    /// Track extremes.
    fn bounds(&self) -> (ThermodynamicTemperature, ThermodynamicTemperature) {
        (from_scale(self.scale.lower()), from_scale(self.scale.upper()))
    }

    fn refresh_bound_labels(&self) {
        let (lo, hi) = self.bounds();
        let unit = self.unit.get();
        self.min_edit.set_text(&unit.format(lo));
        self.max_edit.set_text(&unit.format(hi));
    }

    fn emit_changed(&self) {
        if let Some(cb) = self.on_changed.borrow().as_ref() {
            cb(from_scale(self.scale.min_value()), from_scale(self.scale.max_value()));
        }
    }

    fn connect_scale(this: &Rc<Self>) {
        // The scale only notifies on user interaction, so every change must
        // reach the app; the bubble follows the handle while it's dragged.
        let that = this.clone();
        this.scale.connect_value_changed(move |_| {
            that.update_bubble();
            that.emit_changed();
        });
        let that = this.clone();
        this.scale.connect_drag_changed(move |scale| {
            that.bubble.set_visible(scale.dragging().is_some());
            that.update_bubble();
        });
    }

    /// Place the bubble in the host overlay, centered above the dragged
    /// handle.
    fn update_bubble(&self) {
        let Some(handle) = self.scale.dragging() else { return };
        let host = self.bubble_host.borrow().clone();
        let Some(host) = host else { return };
        let Some(x) = self.scale.handle_center(handle) else { return };
        let point = gtk4::graphene::Point::new(x as f32, 0.0);
        let Some(point) = self.scale.compute_point(&host, &point) else { return };

        let value = match handle {
            Handle::Min => self.scale.min_value(),
            Handle::Max => self.scale.max_value(),
        };
        self.bubble.set_text(&self.unit.get().format(from_scale(value)));
        let (_, natural) = self.bubble.preferred_size();
        let width = natural.width() as f64;
        let left =
            (point.x() as f64 - width / 2.0).clamp(0.0, (host.width() as f64 - width).max(0.0));
        self.bubble.set_margin_start(left as i32);
        self.bubble
            .set_margin_bottom((host.height() as f64 - point.y() as f64 + BUBBLE_GAP) as i32);
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
    /// new extreme are clamped by the scale; that changes the rendered
    /// range, so it notifies.
    fn commit_bound(&self, handle: Handle, text: &str) {
        let (lo, hi) = (self.scale.lower(), self.scale.upper());
        if let Some(value) = parse_temp(text, self.unit.get()) {
            let value = to_scale(value);
            let before = (self.scale.min_value(), self.scale.max_value());
            match handle {
                Handle::Min if value < hi => self.scale.set_bounds(value, hi),
                Handle::Max if value > lo => self.scale.set_bounds(lo, value),
                _ => {}
            }
            if (self.scale.min_value(), self.scale.max_value()) != before {
                self.emit_changed();
            }
        }
        self.refresh_bound_labels();
    }
}

/// Parse user input like "12.5", "-40", "20,5 °C" or "296 K" in the given
/// display unit.
fn parse_temp(text: &str, unit: TempUnit) -> Option<ThermodynamicTemperature> {
    let text = text.trim().trim_end_matches(|c: char| c.is_alphabetic() || c == '°').trim();
    text.replace(',', ".").parse::<f32>().ok().map(|v| unit.to_temperature(v))
}

/// The drag bubble's look, plus a keyboard focus ring around a handle: the
/// stock stylesheet only draws one for a focusable `scale`, while here the
/// handle nodes themselves take focus.
fn install_css() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let css = gtk4::CssProvider::new();
        css.load_from_string(
            "scale.bb-range > trough > slider {
                 outline: 2px solid transparent;
                 outline-offset: 0px;
                 transition: outline-color 200ms ease;
             }
             scale.bb-range > trough > slider:focus-visible {
                 outline-color: color-mix(in srgb, var(--accent-color) 50%, transparent);
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
    use uom::si::thermodynamic_temperature::degree_celsius;

    use crate::domain::units::TempUnit;

    use super::parse_temp;

    /// Compare in celsius with a small tolerance: unit conversions (e.g.
    /// fahrenheit) are not exact in f32.
    fn assert_celsius(text: &str, unit: TempUnit, expected: f32) {
        let parsed = parse_temp(text, unit).expect(text).get::<degree_celsius>();
        assert!((parsed - expected).abs() < 1e-3, "{text}: {parsed} != {expected}");
    }

    #[test]
    fn parses_plain_and_suffixed_input() {
        assert_celsius("12.5", TempUnit::Celsius, 12.5);
        assert_celsius(" -40 ", TempUnit::Celsius, -40.0);
        assert_celsius("20,5 °C", TempUnit::Celsius, 20.5);
        assert_celsius("273.15 K", TempUnit::Kelvin, 0.0);
        assert_celsius("32 °F", TempUnit::Fahrenheit, 0.0);
        assert_eq!(parse_temp("garbage", TempUnit::Celsius), None);
        assert_eq!(parse_temp("", TempUnit::Celsius), None);
    }
}
