//! Palette selection: the popover with standard palette groups, the
//! per-file embedded camera palette entry, and the colour bar.

use std::rc::Rc;

use cairo::LinearGradient;
use gettextrs::gettext;
use gtk4::prelude::*;
use gtk4::{Button, DrawingArea, FlowBox, Label, Orientation, SelectionMode};

use super::app_window::AppState;
use super::palettes::PALETTES;

impl AppState {
    pub(super) fn connect_palette_ui(this: &Rc<Self>) {
        Self::setup_palette_popover(this);
        Self::connect_color_bar(this);
    }

    fn setup_palette_popover(this: &Rc<Self>) {
        let palette = &this.ui.palette;
        palette.embedded_section.set_visible(false);
        palette.palette_box.prepend(&palette.embedded_section);
        for (group_name, palettes) in palette_groups() {
            Self::add_palette_group(this, &group_name, &palettes);
        }
    }

    fn add_palette_group(this: &Rc<Self>, name: &str, palettes: &[(String, usize)]) {
        let heading = Label::builder().label(name).xalign(0.0).build();
        heading.add_css_class("heading");
        let flow = swatch_flow_box();

        let palette_box = &this.ui.palette.palette_box;
        palette_box.append(&heading);
        palette_box.append(&flow);
        for (name, idx) in palettes {
            flow.insert(&Self::standard_swatch(this, name, *idx), -1);
        }
    }

    /// A swatch button for `PALETTES[idx]` that applies that palette on click.
    /// `name` is already translated.
    fn standard_swatch(this: &Rc<Self>, name: &str, idx: usize) -> Button {
        let btn = make_swatch_button(PALETTES[idx].to_vec(), name);
        // Mark first palette (Turbo, idx=0) as initially selected
        if idx == 0 {
            btn.add_css_class("suggested-action");
        }

        let that = this.clone();
        let btn_clone = btn.clone();
        btn.connect_clicked(move |_| {
            that.apply_standard_palette(idx);
            that.highlight_swatch(&btn_clone);
        });

        this.ui.palette.all_swatches.borrow_mut().push((idx, btn.clone()));
        btn
    }

    /// Rebuild the popover's "Embedded" section for the palette found in the
    /// current file (or hide the section when there is none).
    pub(super) fn update_embedded_palette(this: &Rc<Self>, palette: Option<Vec<[f32; 3]>>) {
        let section = this.ui.palette.embedded_section.clone();
        while let Some(child) = section.first_child() {
            section.remove(&child);
        }
        *this.ui.palette.embedded_swatch.borrow_mut() = None;

        let Some(palette_data) = palette else {
            // No embedded palette to honour: the standard palette (already
            // applied by load_thermogram) is in effect — make sure its swatch
            // carries the highlight.
            section.set_visible(false);
            this.highlight_standard_swatch();
            return;
        };

        let btn = make_swatch_button(palette_data.clone(), &gettext("Camera palette"));
        // Only apply the embedded palette if the user's current choice is
        // "camera palette"; an explicit standard palette stays selected
        // (and highlighted) when browsing between files.
        if this.use_embedded_palette.get() {
            *this.active_palette.borrow_mut() = palette_data.clone();
            this.highlight_swatch(&btn);
        }
        let that = this.clone();
        let btn_clone = btn.clone();
        btn.connect_clicked(move |_| {
            that.apply_embedded_palette(&palette_data);
            that.highlight_swatch(&btn_clone);
        });

        let heading = Label::builder().label(gettext("Embedded")).xalign(0.0).build();
        heading.add_css_class("heading");
        let flow = swatch_flow_box();
        flow.insert(&btn, -1);

        *this.ui.palette.embedded_swatch.borrow_mut() = Some(btn);
        section.append(&heading);
        section.append(&flow);
        section.set_visible(true);
    }

    /// Make `PALETTES[idx]` the active palette and re-render.
    fn apply_standard_palette(&self, idx: usize) {
        self.palette_idx.set(idx);
        self.use_embedded_palette.set(false);
        *self.active_palette.borrow_mut() = PALETTES[idx].to_vec();
        self.draw_render_threaded();
        self.ui.palette.color_bar.queue_draw();
    }

    /// Make the file's embedded camera palette the active palette and re-render.
    fn apply_embedded_palette(&self, palette: &[[f32; 3]]) {
        self.use_embedded_palette.set(true);
        *self.active_palette.borrow_mut() = palette.to_vec();
        self.draw_render_threaded();
        self.ui.palette.color_bar.queue_draw();
    }

    /// Highlight `btn` as the selected palette, clearing every other swatch.
    fn highlight_swatch(&self, btn: &Button) {
        let palette = &self.ui.palette;
        for (_, b) in palette.all_swatches.borrow().iter() {
            b.remove_css_class("suggested-action");
        }
        if let Some(b) = palette.embedded_swatch.borrow().as_ref() {
            b.remove_css_class("suggested-action");
        }
        btn.add_css_class("suggested-action");
    }

    /// Highlight the swatch of the active standard palette, clearing the rest.
    fn highlight_standard_swatch(&self) {
        for (idx, b) in self.ui.palette.all_swatches.borrow().iter() {
            if *idx == self.palette_idx.get() {
                b.add_css_class("suggested-action");
            } else {
                b.remove_css_class("suggested-action");
            }
        }
    }

    fn connect_color_bar(this: &Rc<Self>) {
        let that = this.clone();
        this.ui.palette.color_bar.set_draw_func(move |_, ctx, w, h| {
            draw_color_bar(ctx, w as f64, h as f64, &that.active_palette.borrow());
        });

        // Colour bar tooltip: map y-position to temperature
        let that = this.clone();
        this.ui.palette.color_bar.set_has_tooltip(true);
        this.ui.palette.color_bar.connect_query_tooltip(move |widget, _, y, _, tooltip| {
            let h = widget.height();
            if h == 0 {
                return false;
            }
            let position = 1.0 - y as f32 / h as f32;
            let temp = that.min_temp.get() + position * (that.max_temp.get() - that.min_temp.get());
            tooltip.set_text(Some(&that.temp_unit.get().format(temp)));
            true
        });
    }
}

/// Popover groups: (translated heading, [(translated name, `PALETTES` index)]).
/// Built at runtime so xgettext sees each name as a literal.
fn palette_groups() -> [(String, Vec<(String, usize)>); 3] {
    [
        (
            gettext("Perceptually uniform"),
            vec![
                (gettext("Turbo"), 0),
                (gettext("Cividis"), 1),
                (gettext("Inferno"), 5),
                (gettext("Magma"), 8),
                (gettext("Viridis"), 9),
            ],
        ),
        (
            gettext("Classic"),
            vec![
                (gettext("Grayscale"), 3),
                (gettext("Hot"), 4),
                (gettext("Rainbow"), 6),
                (gettext("Copper"), 2),
            ],
        ),
        (gettext("Diverging"), vec![(gettext("Coolwarm"), 7)]),
    ]
}

fn draw_color_bar(context: &cairo::Context, width: f64, height: f64, palette: &[[f32; 3]]) {
    let gradient = LinearGradient::new(0.0, 0.0, 0.0, height);
    let step = 1.0 / (palette.len() - 1) as f64;
    for (i, color) in palette.iter().enumerate() {
        // Palette index 0 = min (bottom of bar), last = max (top of bar)
        gradient.add_color_stop_rgb(
            1.0 - i as f64 * step,
            color[0] as f64,
            color[1] as f64,
            color[2] as f64,
        );
    }
    context.rectangle(0.0, 0.0, width, height);
    let _ = context.set_source(&gradient);
    let _ = context.fill();
}

/// Flat button showing a horizontal gradient swatch of `palette` above a
/// caption with the (already translated) palette name.
fn make_swatch_button(palette: Vec<[f32; 3]>, name: &str) -> Button {
    let swatch = DrawingArea::builder().width_request(80).height_request(16).build();
    swatch.set_draw_func(move |_, ctx, w, h| {
        let g = LinearGradient::new(0.0, 0.0, w as f64, 0.0);
        let step = 1.0 / (palette.len() - 1) as f64;
        for (i, c) in palette.iter().enumerate() {
            g.add_color_stop_rgb(i as f64 * step, c[0] as f64, c[1] as f64, c[2] as f64);
        }
        ctx.rectangle(0.0, 0.0, w as f64, h as f64);
        let _ = ctx.set_source(&g);
        let _ = ctx.fill();
    });

    let label = Label::new(Some(name));
    label.add_css_class("caption");

    let vbox = gtk4::Box::new(Orientation::Vertical, 2);
    vbox.append(&swatch);
    vbox.append(&label);

    let btn = Button::builder().child(&vbox).build();
    btn.add_css_class("flat");
    btn
}

fn swatch_flow_box() -> FlowBox {
    FlowBox::builder()
        .selection_mode(SelectionMode::None)
        .homogeneous(true)
        .max_children_per_line(3)
        .min_children_per_line(2)
        .build()
}
