//! The thermogram canvas: threaded rendering, the image draw function
//! (including measurement overlays), zoom, display modes (thermal, visible,
//! overlay), the temperature range scales, and the per-pixel temperature
//! tooltip.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use gettextrs::gettext;
use gio::SimpleAction;
use glib::object::SendWeakRef;
use glib::MainContext;
use gtk4::prelude::*;
use gtk4::{EventControllerMotion, EventControllerScroll, EventControllerScrollFlags, Tooltip};
use imgref::ImgVec;
use rgb::RGB8;

use super::app_window::AppState;
use libblackbody::{Measurement, Thermogram, ThermogramTrait};

impl AppState {
    pub(super) fn connect_canvas(this: &Rc<Self>) {
        Self::connect_pixel_tooltip(this);
        Self::connect_mode_toggles(this);
        Self::connect_draw(this);
        Self::connect_range_scales(this);
        Self::connect_zoom(this);
        Self::connect_zoom_actions(this);
        Self::connect_pan(this);
    }

    /// The latest rendered frame as a cairo surface. Drains the cross-thread
    /// slot at most once per frame; the returned clone is a refcount bump,
    /// not a pixel copy.
    pub(super) fn current_surface(&self) -> Option<cairo::ImageSurface> {
        if let Some((bgra, w, h)) = self.image_bgra.lock().unwrap().take() {
            let stride = w * 4;
            if let Ok(surface) =
                cairo::ImageSurface::create_for_data(bgra, cairo::Format::Rgb24, w, h, stride)
            {
                *self.image_surface.borrow_mut() = Some(surface);
            }
        }
        self.image_surface.borrow().clone()
    }

    pub(super) fn is_thermal_mode(&self) -> bool {
        self.ui.header.mode_group.active_name().as_deref() == Some("thermal")
    }

    pub(super) fn is_overlay_mode(&self) -> bool {
        self.ui.header.mode_group.active_name().as_deref() == Some("overlay")
    }

    /// Drop the displayed frame. Bumping the generation orphans any in-flight
    /// render thread, so a slow render of the previous file can't republish
    /// its frame afterwards.
    pub(super) fn clear_canvas(&self) {
        self.render_generation.fetch_add(1, Ordering::Relaxed);
        *self.image_bgra.lock().unwrap() = None;
        *self.image_surface.borrow_mut() = None;
        self.ui.canvas.image.queue_draw();
    }

    /// Render the current mode's image on a worker thread and publish the
    /// result to `image_bgra`. Render threads finish in arbitrary order (a
    /// slider drag spawns many); only the thread matching the latest
    /// generation may publish, otherwise a slow older render would overwrite
    /// a newer one.
    pub(super) fn draw_render_threaded(&self) {
        let Some(thermogram) = self.thermogram.borrow().clone() else { return };
        let min = self.min_temp.get();
        let max = self.max_temp.get();
        let palette: Vec<[f32; 3]> = self.active_palette.borrow().clone();
        let thermal_mode = self.is_thermal_mode();
        let pip_mode = self.is_overlay_mode();
        let img_ref = SendWeakRef::from(self.ui.canvas.image.downgrade());
        let surface_arc = self.image_bgra.clone();
        let generation = self.render_generation.clone();
        let my_gen = generation.fetch_add(1, Ordering::Relaxed) + 1;

        std::thread::spawn(move || {
            if generation.load(Ordering::Relaxed) != my_gen {
                return; // superseded while queued; skip the expensive render
            }
            let image = render_for_mode(&thermogram, min, max, &palette, thermal_mode, pip_mode);
            let (w, h) = (image.width() as i32, image.height() as i32);
            let bgra = rgb_to_bgra(image);
            MainContext::default().invoke(move || {
                if generation.load(Ordering::Relaxed) != my_gen {
                    return; // a newer render already published
                }
                *surface_arc.lock().unwrap() = Some((bgra, w, h));
                if let Some(img) = img_ref.upgrade() {
                    img.queue_draw();
                }
            });
        });
    }

    fn connect_pixel_tooltip(this: &Rc<Self>) {
        let that = this.clone();
        this.ui.canvas.image.set_has_tooltip(true);
        this.ui.canvas.image.connect_query_tooltip(move |_, x, y, _, tooltip| {
            that.query_tooltip(x, y, tooltip)
        });
    }

    fn query_tooltip(&self, x: i32, y: i32, tooltip: &Tooltip) -> bool {
        // Temperature readout only makes sense on the thermal render: the
        // visual and PiP images have different dimensions and geometry.
        if !self.is_thermal_mode() {
            return false;
        }
        let thermogram = self.thermogram.borrow();
        let Some(thermogram) = thermogram.as_ref() else { return false };

        let shape = thermogram.thermal_shape(); // [height, width]
        let image = &self.ui.canvas.image;
        let Some((ix, iy)) = widget_to_image(
            x as f64,
            y as f64,
            shape[1],
            shape[0],
            image.width() as f64,
            image.height() as f64,
        ) else {
            return false;
        };

        let temp = thermogram.thermal()[(ix, iy)];
        tooltip.set_text(Some(&self.temp_unit.get().format(temp)));
        true
    }

    fn connect_mode_toggles(this: &Rc<Self>) {
        let that = this.clone();
        this.ui.header.mode_group.connect_active_notify(move |_| that.apply_mode());
    }

    /// Called when the active display mode changes.
    fn apply_mode(&self) {
        // The overlay renders thermal data too, so palette and range still apply.
        let uses_palette = self.is_thermal_mode() || self.is_overlay_mode();
        self.ui.palette.color_bar.set_sensitive(uses_palette);
        self.ui.osd.range_bar.set_visible(uses_palette);
        self.ui.header.palette_button.set_sensitive(uses_palette);

        // Re-render with the appropriate image
        self.draw_render_threaded();
        if uses_palette {
            self.ui.palette.color_bar.queue_draw();
        }
    }

    fn connect_draw(this: &Rc<Self>) {
        let that = this.clone();
        this.ui.canvas.image.set_draw_func(move |_, ctx, width, height| {
            that.draw_canvas(ctx, width, height);
        });
    }

    /// Scales the rendered image to fill the DrawingArea. In fit mode the
    /// area fills the viewport (hexpand/vexpand=true), so the image is scaled
    /// to fit. In zoom mode size_request sets the area to exactly the desired
    /// pixel size, so the image fills it 1:1.
    fn draw_canvas(&self, ctx: &cairo::Context, width: i32, height: i32) {
        let Some(surface) = self.current_surface() else { return };
        let (img_w, img_h) = (surface.width() as f64, surface.height() as f64);
        let (scale, off_x, off_y) = fit_transform(img_w, img_h, width as f64, height as f64);
        let _ = ctx.save();
        ctx.translate(off_x, off_y);
        ctx.scale(scale, scale);
        let _ = ctx.set_source_surface(&surface, 0.0, 0.0);
        let _ = ctx.paint();
        let _ = ctx.restore();

        // ponytail: measurement coords are thermal pixels, which map 1:1 onto the
        // thermal render only; add the PiP transform if overlays are wanted there.
        if !self.draw_measurements.get() || !self.is_thermal_mode() {
            return;
        }
        let thermogram = self.thermogram.borrow();
        let Some(thermogram) = thermogram.as_ref() else { return };
        draw_measurement_overlay(ctx, &thermogram.measurements(), scale, off_x, off_y);
    }

    fn connect_range_scales(this: &Rc<Self>) {
        let slider = &this.ui.osd.range_slider;
        slider.attach_bubble(&this.ui.canvas.overlay);
        let that = this.clone();
        slider.connect_changed(move |min, max| {
            that.min_temp.set(min);
            that.max_temp.set(max);
            that.draw_render_threaded();
            that.ui.palette.color_bar.queue_draw();
        });
    }

    /// Ctrl+scroll → zoom; plain scroll → pan (handled by ScrolledWindow)
    fn connect_zoom(this: &Rc<Self>) {
        let that = this.clone();
        let ctrl = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
        ctrl.connect_scroll(move |_, _dx, dy| {
            that.zoom_step(dy);
            glib::Propagation::Stop
        });
        this.ui.canvas.scrolled_window.add_controller(ctrl);

        // Track the pointer for cursor-anchored zooming.
        let that = this.clone();
        let motion = EventControllerMotion::new();
        motion.connect_motion(move |_, x, y| {
            that.mouse_pos.set((x, y));
        });
        this.ui.canvas.scrolled_window.add_controller(motion);
    }

    /// Drag the image to pan, as an alternative to the scrollbars. The
    /// adjustments clamp to their bounds, so in fit mode (or when the image
    /// is smaller than the viewport) the drag is a harmless no-op.
    fn connect_pan(this: &Rc<Self>) {
        let drag = gtk4::GestureDrag::new();
        let start = Rc::new(Cell::new((0.0, 0.0)));

        let that = this.clone();
        let start_pos = start.clone();
        drag.connect_drag_begin(move |_, _, _| {
            let sw = &that.ui.canvas.scrolled_window;
            start_pos.set((sw.hadjustment().value(), sw.vadjustment().value()));
        });

        let that = this.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            let sw = &that.ui.canvas.scrolled_window;
            let (h0, v0) = start.get();
            sw.hadjustment().set_value(h0 - dx);
            sw.vadjustment().set_value(v0 - dy);
        });

        // Show the open hand whenever the image can be dragged. The bounds
        // of the adjustments change on zoom, image load and window resize,
        // all of which emit "changed" after the layout pass — unlike calling
        // this directly from apply_zoom, which would read stale bounds.
        let sw = &this.ui.canvas.scrolled_window;
        for adj in [sw.hadjustment(), sw.vadjustment()] {
            let that = this.clone();
            adj.connect_changed(move |_| that.update_pan_cursor());
        }

        sw.add_controller(drag);
    }

    /// The open-hand cursor when panning is possible, the default otherwise.
    /// Set on the drawing area as well as the viewport: during a drag the
    /// implicit pointer grab resolves the cursor from the widget that took
    /// the press (the drawing area), so the hand must live there too or it
    /// reverts to the arrow mid-drag.
    fn update_pan_cursor(&self) {
        let cursor = if self.can_pan() { grab_cursor() } else { None };
        self.ui.canvas.scrolled_window.set_cursor(cursor.as_ref());
        self.ui.canvas.image.set_cursor(cursor.as_ref());
    }

    /// Whether the image overflows the viewport in either direction.
    fn can_pan(&self) -> bool {
        let sw = &self.ui.canvas.scrolled_window;
        let overflows =
            |adj: gtk4::Adjustment| adj.upper() - adj.lower() > adj.page_size() + 0.5;
        overflows(sw.hadjustment()) || overflows(sw.vadjustment())
    }

    /// One scroll notch: 5% zoom per unit, keeping the image point under the
    /// cursor stationary.
    fn zoom_step(&self, dy: f64) {
        let old_factor = self.effective_zoom_factor();
        let new_factor = (old_factor * 1.05_f64.powf(-dy)).clamp(0.1, 10.0);
        self.zoom_fit.set(false);
        self.zoom_factor.set(new_factor);
        self.keep_cursor_anchored(old_factor, new_factor);
        self.apply_zoom();
    }

    /// Effective zoom factor before a zoom step; fit mode needs its ratio computed.
    fn effective_zoom_factor(&self) -> f64 {
        if !self.zoom_fit.get() {
            return self.zoom_factor.get();
        }
        self.current_surface().map(|surface| {
            let vw = self.ui.canvas.scrolled_window.width() as f64;
            let vh = self.ui.canvas.scrolled_window.height() as f64;
            (vw / surface.width() as f64).min(vh / surface.height() as f64)
        }).unwrap_or(1.0)
    }

    /// Adjust the scroll position so the image point under the cursor stays
    /// under the cursor across the zoom change.
    fn keep_cursor_anchored(&self, old_factor: f64, new_factor: f64) {
        let (mx, my) = self.mouse_pos.get();
        let hadj = self.ui.canvas.scrolled_window.hadjustment();
        let vadj = self.ui.canvas.scrolled_window.vadjustment();
        // Image-space point under the cursor before zoom.
        let img_x = hadj.value() + mx;
        let img_y = vadj.value() + my;
        let ratio = new_factor / old_factor;

        // Pre-expand the adjustment bounds to the new image size so that
        // set_value below is not clamped to the old (smaller) upper.
        // The layout pass will confirm the same values from set_size_request.
        if let Some(surface) = self.current_surface() {
            hadj.set_upper((surface.width() as f64 * new_factor).ceil());
            vadj.set_upper((surface.height() as f64 * new_factor).ceil());
        }
        hadj.set_value(img_x * ratio - mx);
        vadj.set_value(img_y * ratio - my);
    }

    fn apply_zoom(&self) {
        let image = &self.ui.canvas.image;
        if self.zoom_fit.get() {
            image.set_halign(gtk4::Align::Fill);
            image.set_valign(gtk4::Align::Fill);
            image.set_size_request(-1, -1);
            self.ui.osd.zoom_label.set_text(&gettext("Fit"));
        } else {
            let factor = self.zoom_factor.get();
            if let Some(surface) = self.current_surface() {
                let w = (surface.width() as f64 * factor) as i32;
                let h = (surface.height() as f64 * factor) as i32;
                // Center within the viewport so the image doesn't stretch to fill it.
                // The viewport always allocates max(natural, viewport_size); halign=center
                // keeps the DrawingArea at its natural (= size_request) size within that.
                image.set_halign(gtk4::Align::Center);
                image.set_valign(gtk4::Align::Center);
                image.set_size_request(w, h);
            }
            self.ui.osd.zoom_label.set_text(&format!("{}%", (factor * 100.0).round() as u32));
        }
    }

    fn connect_zoom_actions(this: &Rc<Self>) {
        let that = this.clone();
        let set_zoom = SimpleAction::new("set-zoom", Some(glib::VariantTy::new("i").unwrap()));
        set_zoom.connect_activate(move |_, param| {
            let pct = param.and_then(|v| v.get::<i32>()).unwrap_or(100);
            that.zoom_fit.set(false);
            that.zoom_factor.set(pct as f64 / 100.0);
            that.apply_zoom();
        });
        this.ui.window.add_action(&set_zoom);

        let that = this.clone();
        let zoom_fit = SimpleAction::new("zoom-fit", None);
        zoom_fit.connect_activate(move |_, _| {
            that.zoom_fit.set(true);
            that.apply_zoom();
        });
        this.ui.window.add_action(&zoom_fit);
    }
}

/// The pixels for the given display mode, falling back to the thermal render
/// when the file lacks the visual/PiP data.
/// The open-hand cursor, falling back to the legacy X11 name "openhand" for
/// themes that don't ship the CSS name. `set_cursor_from_name` alone would
/// fall back to the arrow, which reads as "dragging is not possible".
fn grab_cursor() -> Option<gtk4::gdk::Cursor> {
    use gtk4::gdk::Cursor;
    let fallback = Cursor::from_name("openhand", None);
    Cursor::from_name("grab", fallback.as_ref()).or(fallback)
}

fn render_for_mode(
    thermogram: &Thermogram,
    min: f32,
    max: f32,
    palette: &[[f32; 3]],
    thermal_mode: bool,
    pip_mode: bool,
) -> ImgVec<RGB8> {
    if pip_mode {
        thermogram
            .picture_in_picture(min, max, palette)
            .unwrap_or_else(|| thermogram.render(min, max, palette))
    } else if !thermal_mode {
        thermogram.visual().unwrap_or_else(|| thermogram.render(min, max, palette))
    } else {
        thermogram.render(min, max, palette)
    }
}

/// Convert an RGB image to Cairo Rgb24 pixels (4 bytes/pixel: BGRX on little-endian).
/// Iterating pixels (not the raw buffer) keeps this correct for non-contiguous images.
fn rgb_to_bgra(image: ImgVec<RGB8>) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(image.width() * image.height() * 4);
    bgra.extend(image.pixels().flat_map(|p| [p.b, p.g, p.r, 0]));
    bgra
}

/// Trace all measurement shapes into one path, then stroke it twice: a dark
/// casing under a white core keeps markers visible on any palette.
fn draw_measurement_overlay(
    ctx: &cairo::Context,
    measurements: &[Measurement],
    scale: f64,
    off_x: f64,
    off_y: f64,
) {
    for m in measurements {
        trace_measurement(ctx, m, scale, off_x, off_y);
    }
    ctx.set_source_rgba(0.0, 0.0, 0.0, 0.8);
    ctx.set_line_width(3.0);
    let _ = ctx.stroke_preserve();
    ctx.set_source_rgb(1.0, 1.0, 1.0);
    ctx.set_line_width(1.5);
    let _ = ctx.stroke();
}

/// Add one measurement's outline to the current path. Coordinates are thermal
/// pixels; `scale`/`off_*` map them into widget space, targeting pixel centres.
fn trace_measurement(ctx: &cairo::Context, m: &Measurement, scale: f64, off_x: f64, off_y: f64) {
    let px = |v: u32| off_x + (v as f64 + 0.5) * scale;
    let py = |v: u32| off_y + (v as f64 + 0.5) * scale;
    let arm = 6.0f64.max(0.5 * scale);
    match m {
        Measurement::Spot { x, y, .. } | Measurement::Endpoint { x, y, .. } => {
            let (cx, cy) = (px(*x), py(*y));
            ctx.move_to(cx - arm, cy);
            ctx.line_to(cx + arm, cy);
            ctx.move_to(cx, cy - arm);
            ctx.line_to(cx, cy + arm);
        }
        // Area params are x, y, width, height (flyr 0.7 misnames w/h as x2/y2)
        Measurement::Area { x: x1, y: y1, width: w, height: h, .. } => {
            ctx.rectangle(px(*x1), py(*y1), *w as f64 * scale, *h as f64 * scale);
        }
        Measurement::Line { x1, y1, x2, y2, .. } => {
            ctx.move_to(px(*x1), py(*y1));
            ctx.line_to(px(*x2), py(*y2));
        }
        // Ellipse params: centre, then the two semi-axis endpoints
        Measurement::Ellipse { params, .. } if params.len() >= 6 => {
            let (xc, yc) = (params[0] as f64, params[1] as f64);
            let (ux, uy) = (params[2] as f64 - xc, params[3] as f64 - yc);
            let (vx, vy) = (params[4] as f64 - xc, params[5] as f64 - yc);
            let (ru, rv) = (ux.hypot(uy), vx.hypot(vy));
            if ru > 0.0 && rv > 0.0 {
                // Build the path under a warped CTM, restore before stroking
                // so the line width stays uniform.
                let _ = ctx.save();
                ctx.translate(px(params[0]), py(params[1]));
                ctx.rotate(uy.atan2(ux));
                ctx.scale(ru * scale, rv * scale);
                ctx.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                let _ = ctx.restore();
            }
        }
        Measurement::Ellipse { .. }
        | Measurement::Alarm { .. }
        | Measurement::Difference { .. } => {}
    }
}

/// Scale and top-left offset of an img_w×img_h image fitted into a widget
/// (content-fit = contain: centred, scaled to fit, aspect ratio kept).
fn fit_transform(img_w: f64, img_h: f64, widget_w: f64, widget_h: f64) -> (f64, f64, f64) {
    let scale = (widget_w / img_w).min(widget_h / img_h);
    let off_x = (widget_w - img_w * scale) / 2.0;
    let off_y = (widget_h - img_h * scale) / 2.0;
    (scale, off_x, off_y)
}

/// Map a widget-space position to image pixel coordinates, or `None` when the
/// position falls in the letterbox margins around the painted image.
fn widget_to_image(
    x: f64,
    y: f64,
    img_w: usize,
    img_h: usize,
    widget_w: f64,
    widget_h: f64,
) -> Option<(usize, usize)> {
    let (scale, off_x, off_y) = fit_transform(img_w as f64, img_h as f64, widget_w, widget_h);
    let ix = (x - off_x) / scale;
    let iy = (y - off_y) / scale;
    // The < 0 check must happen in floating point: a negative value cast to
    // usize saturates to 0, silently mapping margins onto row/column 0.
    if ix < 0.0 || iy < 0.0 {
        return None;
    }
    let (ix, iy) = (ix as usize, iy as usize);
    (ix < img_w && iy < img_h).then_some((ix, iy))
}

#[cfg(test)]
mod tests {
    use super::widget_to_image;

    #[test]
    fn maps_painted_area_and_rejects_margins() {
        // 100×50 image in a 200×200 widget: scale 2, painted 200×100, y-offset 50.
        assert_eq!(widget_to_image(0.0, 50.0, 100, 50, 200.0, 200.0), Some((0, 0)));
        assert_eq!(widget_to_image(0.0, 100.0, 100, 50, 200.0, 200.0), Some((0, 25)));
        assert_eq!(widget_to_image(199.0, 149.0, 100, 50, 200.0, 200.0), Some((99, 49)));

        // Letterbox margins above/below the image previously saturated the
        // negative offset to 0 and reported row 0 temperatures.
        assert_eq!(widget_to_image(10.0, 40.0, 100, 50, 200.0, 200.0), None);
        assert_eq!(widget_to_image(10.0, 151.0, 100, 50, 200.0, 200.0), None);

        // 50×100 image in the same widget: x margins instead.
        assert_eq!(widget_to_image(40.0, 10.0, 50, 100, 200.0, 200.0), None);
        assert_eq!(widget_to_image(160.0, 10.0, 50, 100, 200.0, 200.0), None);
    }
}
