use std::cell::RefCell;
use std::rc::Rc;

use cairo::{Context, LinearGradient};
use gtk4::prelude::{DrawingAreaExtManual, WidgetExt};
use gtk4::{DrawingArea, Tooltip};

#[derive(Clone)]
pub struct Thermometer {
    pub thermometer: DrawingArea,
    palette: Vec<[f32; 3]>,
    min: f32,
    max: f32,
}

impl Thermometer {
    pub fn new(thermometer: DrawingArea, palette: Vec<[f32; 3]>) -> Rc<RefCell<Thermometer>> {
        let thermometer = Thermometer { thermometer, palette, min: 0.0, max: 0.0 };

        let this = Rc::new(RefCell::new(thermometer));
        {
            let that = this.clone();
            this.borrow().thermometer.set_draw_func(move |_, context, _, _| {
                that.borrow().render_thermometer(context);
            });
        }
        {
            let that = this.clone();
            this.borrow().thermometer.set_has_tooltip(true);
            this.borrow().thermometer.connect_query_tooltip(move |_, _, y, _, tooltip| {
                that.borrow().set_tooltip_text(tooltip, y)
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
        self.thermometer.height()
    }

    pub fn allocated_width(&self) -> i32 {
        self.thermometer.width()
    }

    fn render_thermometer(&self, context: &Context) {
        let width = self.allocated_width() as f64;
        let height = self.allocated_height() as f64;
        let pattern = LinearGradient::new(0.0, 0.0, 0.0, height);

        let step = 1.0 / 256.0;
        for (i, v) in self.palette.iter().enumerate() {
            let i_f = i as f64;
            let (r, g, b) = (v[0].into(), v[1].into(), v[2].into());
            pattern.add_color_stop_rgb(1.0 - i_f * step, r, g, b);
        }

        context.rectangle(0.0, 0.0, width, height);
        let _ = context.set_source(&pattern);
        let _ = context.fill();
    }

    fn set_tooltip_text(&self, tooltip: &Tooltip, y: i32) -> bool {
        let max_y = self.allocated_height() - 1;
        if max_y == 0 {
            return false;
        }
        let position = (max_y - y) as f32 / max_y as f32;
        let temperature = (self.max - self.min) * position + self.min;
        let temp = format!("{:.2}°C", temperature);
        tooltip.set_text(Some(temp.as_str()));
        true
    }
}
