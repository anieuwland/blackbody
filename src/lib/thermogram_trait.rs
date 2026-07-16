use std::cmp::{Ordering, PartialOrd};
use std::fs::File;
use std::path::PathBuf;

/// Capture parameters and Planck constants from the camera info record.
#[derive(Clone, Debug)]
pub struct CaptureParams {
    pub emissivity: f32,
    pub object_distance_m: f32,
    /// Reflected apparent temperature in Kelvin.
    pub reflected_temp_k: f32,
    /// Relative humidity (0.0–1.0).
    pub relative_humidity: f32,
    pub planck_r1: f32,
    pub planck_r2: f32,
    pub planck_b: f32,
    pub planck_f: f32,
    pub planck_o: i32,
}

use image::{save_buffer, ColorType};
use ndarray::*;
use tiff::encoder::*;

use crate::palettes;
use crate::Measurement;

/// Temperature statistics over a measurement tool's pixels, in celsius.
/// For single-pixel tools (spots, endpoints) min, max and avg are equal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempStats {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
}

/// All supported thermogram formats implement this trait.
///
/// ```rust
/// pub trait ThermogramTrait {
///     fn thermal(&self) -> &Array<f32, Ix2>;  // Extract the thermal data
///     fn optical(&self) -> &Array<u8, Ix3>>;  // Extract embedded photos, if present
///     fn identifier(&self) -> &str;  // A uniquely identifying string for this thermogram
///     fn render(&self min_temp: f32, max_temp: f32, palette: [[f32; 3]; 256]) -> Array<u8, Ix3>;  // Thermal data render using the given palette
///     fn render_defaults(&self) -> Array<u8, Ix3>;  // Thermal data rendered using the minimum and maximum thermal value and the `palette::TURBO` palette.
///     fn thermal_shape(&self) -> [usize; 2];  // The [height, width] of the thermal data
/// }
/// ```
pub trait ThermogramTrait {
    /// Returns a reference to the 2D array of thermal data in celsius.
    fn thermal(&self) -> &Array<f32, Ix2>;

    /// Returns reference to the raw RGB values of the thermogram's corresponding optical photo, if
    /// present. Otherwise `None`.
    fn optical(&self) -> Option<Array<u8, Ix3>>;

    /// Provide the identifier for this thermogram, which is typically the file path. It can also be
    /// a randomly generated uuid or similar, however, if there is no path associated with the data.
    fn identifier(&self) -> &str;

    /// Returns the file path, or `None` if not a file.
    fn path(&self) -> Option<&PathBuf>;

    /// Returns the palette this thermogram was originally rendered with, if available
    fn palette(&self) -> Option<Vec<[f32; 3]>>;

    /// Render the thermogram with the given color palette and using the given minimum and maximum
    /// temperature bounds.
    ///
    /// All values are clipped to be between the minimum and maximum value, then put in one of 256
    /// bins. Each bin is mapped to one of the colors in the palette to render an RGB color value.
    ///
    /// # Arguments
    /// * `min_temp` - The temperature value, and all values below it, that needs to be mapped to
    ///     the first color in the palette.
    /// * `max_temp` - The temperature value, and all values above it, that needs to be mapped to
    ///     the last color in the palette.
    /// * `palette` - A collection of 256 colors to which the 256 bins will be mapped.
    ///
    /// # Returns
    /// A three-dimensional RGB array of u8 values between 0 and 255.
    fn render(&self, min_temp: f32, max_temp: f32, palette: &[[f32; 3]]) -> Array<u8, Ix3> {
        let num_bands = 3;
        let num_shades = palette.len() - 1;
        let map_color = |v: &f32| {
            let idx = match (min_temp.partial_cmp(v), max_temp.partial_cmp(v)) {
                (Some(Ordering::Greater), _) => 0,
                (_, Some(Ordering::Less)) => num_shades,
                (_, _) => ((v - min_temp) / (max_temp - min_temp) * num_shades as f32) as usize,
            };

            let to_u8 = |f| (f * 255.0) as u8;
            let color = [
                // Create color array sized [u8; num_bands]
                to_u8(palette[idx][0]),
                to_u8(palette[idx][1]),
                to_u8(palette[idx][2]),
            ];

            // Create iterator out of the array so we can use this in flat_map
            (0..num_bands).map(move |i| color[i])
        };

        // Convert thermal array into a color array by iterating over all values,
        // converting thermal values to RGB arrays, flattening the result into a
        // single vector of u8s. Lastly we recreate an ndarray with the shape
        // (height, width, num_bands) from this vector.
        let colored_array: Vec<u8> = self.thermal().iter().flat_map(map_color).collect();

        let width = self.thermal().ncols();
        let height = self.thermal().nrows();
        Array::from_shape_vec((height, width, num_bands), colored_array).unwrap()
    }

    /// Render the thermogram using the minimum and maximum thermal value and the
    // `palette::TURBO` palette.
    fn render_defaults(&self) -> Array<u8, Ix3> {
        self.render(self.min_temp(), self.max_temp(), &palettes::TURBO)
    }

    /// Export thermal data to a tiff file.
    ///
    /// # Arguments
    /// `path` - Where to save the thermogram export to. Regardless of the file extension, a tiff
    ///     file is created.
    ///
    /// # Returns
    /// `Some<()>` in case of success, otherwise `None`.
    fn export_thermal_png(&self, path: &PathBuf) -> Option<()> {
        let w = self.thermal_shape()[1] as u32;
        let h = self.thermal_shape()[0] as u32;
        let pixels: Vec<u16> = self.thermal().iter()
            .map(|&c| (c * 100.0 + 27315.0).clamp(0.0, 65535.0) as u16)
            .collect();
        image::ImageBuffer::<image::Luma<u16>, _>::from_raw(w, h, pixels)?
            .save(path).ok()
    }

    fn export_thermal(&self, path: &PathBuf) -> Option<()> {
        // TODO Return LibblackbodyErrorEnum with finegrained failure info instead of Option
        let thermal = self.thermal().iter().map(|v| *v).collect::<Vec<f32>>();

        let width = self.thermal_shape()[1] as u32;
        let height = self.thermal_shape()[0] as u32;

        // File::create(path)
        //     .and_then(|mut file| TiffEncoder::new(&mut file))
        //     .and_then(|mut tiff| tiff.write_image::<colortype::Gray32Float>(width, height, &thermal))
        //     .ok()

        match File::create(path) {
            // TODO Return error codes and handle in Blackbody
            Ok(mut file) => match TiffEncoder::new(&mut file) {
                Ok(mut tiff) => {
                    tiff.write_image::<colortype::Gray32Float>(width, height, &thermal).ok()
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Save render to file.
    ///
    /// # Arguments
    /// `path` - Where to save the render to. The image type is extrapolated from the extension.
    /// `min_temp` - The minimum temperature for the render, see `render(..)`.
    /// `max_temp` - The maximum temperature for the render, see `render(..)`.
    /// `palette` - The color palette to render the thermogram with, see `render(..)`.
    ///
    /// # Returns
    /// `Some<()>` in case of success, otherwise `None`.
    fn save_render(
        &self,
        path: PathBuf,
        min_temp: f32,
        max_temp: f32,
        palette: &[[f32; 3]],
    ) -> Option<()> {
        let render = self.render(min_temp, max_temp, palette);
        let width = render.shape()[1] as u32;
        let height = render.shape()[0] as u32;
        let render = render.iter().map(|v| *v).collect::<Vec<u8>>();

        // TODO Return LibblackbodyErrorEnum with finegrained failure info instead of Option
        save_buffer(path, &render.as_slice(), width, height, ColorType::Rgb8).ok()
    }

    /// Gives the shape of the thermal data, in the order of [height, width].
    fn thermal_shape(&self) -> [usize; 2] {
        let thermal = self.thermal();
        [thermal.nrows(), thermal.ncols()]
    }

    /// Temperature statistics for a measurement tool, or `None` for tools whose
    /// geometry is not decoded (ellipses, alarms, differences).
    ///
    /// Measurement coordinates come from the file and are clamped to the thermal
    /// dimensions, so corrupt records cannot index out of bounds.
    fn measurement_stats(&self, measurement: &Measurement) -> Option<TempStats> {
        let thermal = self.thermal();
        let (h, w) = (thermal.nrows(), thermal.ncols());
        if h == 0 || w == 0 {
            return None;
        }
        let temp_at = move |x: usize, y: usize| thermal[[y.min(h - 1), x.min(w - 1)]];

        let temps: Vec<f32> = match measurement {
            Measurement::Spot { x, y, .. } | Measurement::Endpoint { x, y, .. } => {
                vec![temp_at(*x as usize, *y as usize)]
            }
            // FLIR area params are x, y, width, height (verified against camera-rendered
            // overlays and exiftool); flyr 0.7 misnames width/height as x2/y2.
            Measurement::Area { x1: x, y1: y, x2: width, y2: height, .. } => {
                let (x, y) = (*x as usize, *y as usize);
                (y..y + *height as usize)
                    .flat_map(|py| (x..x + *width as usize).map(move |px| (px, py)))
                    .map(|(px, py)| temp_at(px, py))
                    .collect()
            }
            Measurement::Line { x1, y1, x2, y2, .. } => {
                line_points(*x1, *y1, *x2, *y2).map(|(x, y)| temp_at(x, y)).collect()
            }
            // Ellipse params are centre, then the endpoints of the two semi-axes:
            // xc, yc, x1, y1, x2, y2 (verified against a camera-rendered overlay).
            Measurement::Ellipse { params, .. } if params.len() >= 6 => {
                let (xc, yc) = (params[0] as f32, params[1] as f32);
                let (ux, uy) = (params[2] as f32 - xc, params[3] as f32 - yc);
                let (vx, vy) = (params[4] as f32 - xc, params[5] as f32 - yc);
                let (u2, v2) = (ux * ux + uy * uy, vx * vx + vy * vy);
                if u2 == 0.0 || v2 == 0.0 {
                    return None;
                }
                // A pixel is inside iff its offset d from the centre, decomposed onto the
                // semi-axes as a = d·u/|u|², b = d·v/|v|², satisfies a² + b² ≤ 1.
                let r = u2.sqrt().max(v2.sqrt()).ceil() as isize;
                let (xc_i, yc_i) = (xc as isize, yc as isize);
                ((yc_i - r).max(0)..=(yc_i + r).min(h as isize - 1))
                    .flat_map(|py| {
                        ((xc_i - r).max(0)..=(xc_i + r).min(w as isize - 1)).map(move |px| (px, py))
                    })
                    .filter(|&(px, py)| {
                        let (dx, dy) = (px as f32 - xc, py as f32 - yc);
                        let a = (dx * ux + dy * uy) / u2;
                        let b = (dx * vx + dy * vy) / v2;
                        a * a + b * b <= 1.0
                    })
                    .map(|(px, py)| temp_at(px as usize, py as usize))
                    .collect()
            }
            // ponytail: raw-parameter tools are skipped; decode their geometry
            // when a camera that uses them shows up.
            Measurement::Ellipse { .. } | Measurement::Alarm { .. } | Measurement::Difference { .. } => {
                return None;
            }
        };

        if temps.is_empty() {
            return None;
        }
        let n = temps.len() as f32;
        let min = temps.iter().cloned().fold(f32::MAX, f32::min);
        let max = temps.iter().cloned().fold(f32::MIN, f32::max);
        let avg = temps.iter().sum::<f32>() / n;
        Some(TempStats { min, max, avg })
    }

    fn has_optical(&self) -> bool {
        self.optical().is_some()
    }

    fn has_palette(&self) -> bool {
        self.palette().is_some()
    }

    /// Returns the lowest temperature in the thermogram, or `f32::MAX` if there is no such value.
    fn min_temp(&self) -> f32 {
        self.thermal().fold(f32::MAX, |acc, elem| acc.min(*elem))
    }

    /// Returns the highest temperature in the thermogram, or `f32::MIN` if there is no such value.
    fn max_temp(&self) -> f32 {
        self.thermal().fold(f32::MIN, |acc, elem| acc.max(*elem))
    }

}

/// Pixels along a line, sampled once per step on the longest axis.
fn line_points(x1: u16, y1: u16, x2: u16, y2: u16) -> impl Iterator<Item = (usize, usize)> {
    let (x1, y1, x2, y2) = (x1 as f32, y1 as f32, x2 as f32, y2 as f32);
    let steps = (x2 - x1).abs().max((y2 - y1).abs()).max(1.0) as usize;
    (0..=steps).map(move |i| {
        let t = i as f32 / steps as f32;
        ((x1 + (x2 - x1) * t).round() as usize, (y1 + (y2 - y1) * t).round() as usize)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Array<f32, Ix2>);
    impl ThermogramTrait for Fake {
        fn thermal(&self) -> &Array<f32, Ix2> { &self.0 }
        fn optical(&self) -> Option<Array<u8, Ix3>> { None }
        fn identifier(&self) -> &str { "fake" }
        fn path(&self) -> Option<&PathBuf> { None }
        fn palette(&self) -> Option<Vec<[f32; 3]>> { None }
    }

    #[test]
    fn measurement_stats_spot_area_line() {
        // 2x3 grid: row 0 = [0, 1, 2], row 1 = [10, 11, 12]
        let t = Fake(Array::from_shape_vec((2, 3), vec![0.0, 1.0, 2.0, 10.0, 11.0, 12.0]).unwrap());

        let spot = t.measurement_stats(&Measurement::Spot { label: "".into(), x: 2, y: 1 }).unwrap();
        assert_eq!((spot.min, spot.max, spot.avg), (12.0, 12.0, 12.0));

        // Area params are x, y, width, height: a 2×1 box at (1, 0)
        let area = Measurement::Area { label: "".into(), x1: 1, y1: 0, x2: 2, y2: 1 };
        let a = t.measurement_stats(&area).unwrap();
        assert_eq!((a.min, a.max, a.avg), (1.0, 2.0, 1.5));

        let line = Measurement::Line { label: "".into(), x1: 0, y1: 0, x2: 2, y2: 0 };
        let l = t.measurement_stats(&line).unwrap();
        assert_eq!((l.min, l.max, l.avg), (0.0, 2.0, 1.0));

        // Out-of-bounds coordinates clamp instead of panicking
        let oob = t.measurement_stats(&Measurement::Spot { label: "".into(), x: 99, y: 99 }).unwrap();
        assert_eq!(oob.avg, 12.0);

        let ellipse = Measurement::Ellipse { label: "".into(), params: vec![] };
        assert!(t.measurement_stats(&ellipse).is_none());
    }

    #[test]
    fn measurement_stats_ellipse() {
        // 5x5 grid with value y*10 + x
        let t = Fake(Array::from_shape_fn((5, 5), |(y, x)| (y * 10 + x) as f32));

        // Circle: centre (2, 2), semi-axis endpoints (3, 2) and (2, 1) → radius 1.
        // Inside: (2,2), (1,2), (3,2), (2,1), (2,3) → 22, 21, 23, 12, 32.
        let circle = Measurement::Ellipse { label: "".into(), params: vec![2, 2, 3, 2, 2, 1] };
        let c = t.measurement_stats(&circle).unwrap();
        assert_eq!((c.min, c.max, c.avg), (12.0, 32.0, 22.0));

        // Degenerate axis → no stats
        let flat = Measurement::Ellipse { label: "".into(), params: vec![2, 2, 3, 2, 2, 2] };
        assert!(t.measurement_stats(&flat).is_none());
    }
}
