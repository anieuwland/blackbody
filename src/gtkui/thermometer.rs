use std::cell::RefCell;
use std::rc::Rc;

use cairo::{Context, LinearGradient};
use gio::prelude::ObjectExt;
use gtk::prelude::WidgetExt;
use gtk::{DrawingArea, Inhibit, Tooltip};

#[derive(Clone)]
pub struct Thermometer {
    pub thermometer: DrawingArea,
    palette: Vec<[f32; 3]>,
    min: f32,
    max: f32,
}

impl Thermometer {
    pub fn new(thermometer: DrawingArea, palette: Vec<[f32; 3]>) -> Rc<RefCell<Thermometer>> {
        let thermometer = Thermometer { thermometer, palette: palette, min: 0.0, max: 0.0 };

        let this = Rc::new(RefCell::new(thermometer));
        {
            // Tell the drawing area how to draw the thermometer
            let that = this.clone();
            this.borrow()
                .thermometer
                .connect_draw(move |_, context| that.borrow().render_thermometer(context));
        }
        {
            // Let the tooltip show the temperature of that color
            let that = this.clone();
            this.borrow().thermometer.set_property("has-tooltip", &true).ok().map(|_| {
                this.borrow().thermometer.connect_query_tooltip(move |_, _, y, _, tooltip| {
                    that.borrow().set_tooltip_text(tooltip, y)
                });
            });
        }

        this
    }

    pub fn set_palette(&mut self, palette: Vec<[f32; 3]>) {
        self.palette = palette;
    }

    pub fn set_minimum(&mut self, min: f32) {
        self.min = min;
    }

    pub fn set_maximum(&mut self, max: f32) {
        self.max = max;
    }

    pub fn queue_draw(&self) {
        self.thermometer.queue_draw();
    }

    pub fn allocated_height(&self) -> i32 {
        self.thermometer.allocated_height()
    }

    pub fn allocated_width(&self) -> i32 {
        self.thermometer.allocated_width()
    }

    fn render_thermometer(&self, context: &Context) -> Inhibit {
        // Define the area and direction of the gradient
        let width = self.allocated_width() as f64;
        let height = self.allocated_height() as f64;
        let pattern = LinearGradient::new(0.0, 0.0, 0.0, height);

        // Set the color transition points
        let step = 1.0 / 256.0;
        for (i, v) in self.palette.iter().enumerate() {
            let i_f = i as f64;
            let (r, g, b) = (v[0].into(), v[1].into(), v[2].into());
            pattern.add_color_stop_rgb(1.0 - i_f * step, r, g, b);
        }

        // Actually draw the gradient in the size of the widget
        context.rectangle(0.0, 0.0, width, height);
        let _ = context.set_source(&pattern);
        let _ = context.fill();
        Inhibit(false)
    }

    fn set_tooltip_text(&self, tooltip: &Tooltip, y: i32) -> bool {
        // Calculate the position of the mouse on the thermometer
        // Return early if it would result in division by 0
        let max_y = self.allocated_height() - 1;
        if max_y == 0 {
            return false;
        }
        let position = (max_y - y) as f32 / max_y as f32;

        // Convert position to temperature in the min/max range and
        // update the tooltip
        let temperature = (self.max - self.min) * position + self.min;
        let temp = format!("{:.2}°C", temperature);
        tooltip.set_text(Some(temp.as_str()));

        true
    }
}
