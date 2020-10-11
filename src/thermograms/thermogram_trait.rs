use std::collections::HashMap;

use ndarray::*;
use ndarray_stats::*;

pub trait ThermogramTrait {
    fn thermal(&self) -> &Array<f32, Ix2>;
    fn optical(&self) -> Option<&Array<u8, Ix3>>;
    fn identifier(&self) -> String;

    fn render_defaults(&self) -> Array<u8, Ix3> {
        self.render(1.8f32, 8.2f32)
    }

    fn render_clip_percentiles(&self, _min_p: u8, _max_p: u8) -> Array<u8, Ix3> {
        self.render(self.min_temp(), self.max_temp())
    }

    fn render(&self, min_temp: f32, max_temp: f32) -> Array<u8, Ix3> {
        let clipped = self.thermal().mapv(|v| {
            if v < min_temp {
                0
            } else if v > max_temp {
                255
            } else {
                ((v - min_temp) / (max_temp - min_temp) * 255f32) as u8
            }
        });

        let grayscale = clipped.insert_axis(Axis(2));

        stack(
            Axis(2),
            &[grayscale.view(), grayscale.view(), grayscale.view()],
        )
        .unwrap()
    }

    fn thermal_shape(&self) -> [usize; 2] {
        // FIXME copies whole array
        let thermal = self.thermal();
        [thermal.nrows(), thermal.ncols()]
    }

    fn metadata(&self) -> HashMap<String, String> {
        HashMap::new() // TODO
    }

    fn as_base64(&self) -> String {
        "".to_string() // TODO
    }

    fn positionally_annotated(&self) -> bool {
        false // TODO
    }

    fn position(&self) -> [f32; 2] {
        [0.0, 0.0] // TODO
    }

    fn direction(&self) -> f32 {
        0.0 // TODO
    }

    fn angle(&self) -> f32 {
        0.0 // TODO
    }

    fn time_stamp(&self) -> u8 {
        0 // TODO
    }

    fn path(&self) -> Option<String> {
        None // TODO
    }

    fn has_optical(&self) -> bool {
        self.optical() == None // TODO
    }

    fn thermal_preprocessed_simple(&self) -> Array<f32, Ix2> {
        //let mut thermal = self.normalized();
        //thermal.mapv_inplace(|v| (v - 0.5) * 255f32);
        let thermal = self.normalized();
        let thermal = (thermal - 0.5) * 255f32;
        thermal
    }

    fn min_temp(&self) -> f32 {
        *self.thermal().min_skipnan()
    }

    fn max_temp(&self) -> f32 {
        *self.thermal().max_skipnan()
    }

    fn normalized(&self) -> Array<f32, Ix2> {
        let thermal = self.thermal();
        (thermal - self.min_temp()) / (self.max_temp() + 0.001)
    }
}
