use flyr::measurement_info::Measurement as Flir;
use imgref::ImgVec;
use serendip::markers::Marker as Fluke;

/// Measurement shapes supported by libblackbody.
///
/// Coordinates are in thermal-image pixels. Labels are the names assigned
/// to each measurement on the camera.
#[derive(Clone, Debug)]
pub enum Measurement {
    Spot {
        label: String,
        x: u32,
        y: u32,
    },
    Area {
        label: String,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    /// Ellipse with raw parameter list (centre x/y, radii, rotation).
    Ellipse {
        label: String,
        params: Vec<u32>,
    },
    Line {
        label: String,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
    },
    Endpoint {
        label: String,
        x: u32,
        y: u32,
    },
    /// Alarm with raw parameter list.
    Alarm {
        label: String,
        params: Vec<u32>,
    },
    /// Difference measurement with raw parameter list.
    Difference {
        label: String,
        params: Vec<u32>,
    },
}

/// Losslessly widen a raw FLIR parameter list.
fn widen(params: &[u16]) -> Vec<u32> {
    params.iter().map(|&p| p.into()).collect()
}

impl From<&Flir> for Measurement {
    fn from(m: &Flir) -> Self {
        match m {
            Flir::Spot { label, x, y } => {
                Measurement::Spot { label: label.clone(), x: (*x).into(), y: (*y).into() }
            }
            Flir::Ellipse { label, params } => {
                Measurement::Ellipse { label: label.clone(), params: widen(params) }
            }
            // FLIR area params are x, y, width, height; flyr misnames them x2/y2.
            Flir::Area { label, x1, y1, x2, y2 } => Measurement::Area {
                label: label.clone(),
                x: (*x1).into(),
                y: (*y1).into(),
                width: (*x2).into(),
                height: (*y2).into(),
            },
            Flir::Line { label, x1, y1, x2, y2 } => Measurement::Line {
                label: label.clone(),
                x1: (*x1).into(),
                y1: (*y1).into(),
                x2: (*x2).into(),
                y2: (*y2).into(),
            },
            Flir::Endpoint { label, x, y } => {
                Measurement::Endpoint { label: label.clone(), x: (*x).into(), y: (*y).into() }
            }
            Flir::Alarm { label, params } => {
                Measurement::Alarm { label: label.clone(), params: widen(params) }
            }
            Flir::Difference { label, params } => {
                Measurement::Difference { label: label.clone(), params: widen(params) }
            }
        }
    }
}

impl From<&Fluke> for Measurement {
    fn from(m: &Fluke) -> Self {
        match m {
            Fluke::Point { coords, metadata } => Measurement::Spot {
                label: metadata.label2.clone(),
                x: coords.x.into(),
                y: coords.y.into(),
            },
            Fluke::Box { start, end, metadata } => Measurement::Area {
                label: metadata.label2.clone(),
                x: start.x.min(end.x),
                y: start.y.min(end.y),
                width: end.x.abs_diff(start.x),
                height: end.y.abs_diff(start.y),
            },
        }
    }
}

/// Temperature statistics over a measurement tool's pixels, in celsius.
/// For single-pixel tools (spots, endpoints) min, max and avg are equal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempStats {
    pub min: f32,
    pub max: f32,
    pub avg: f32,
}

impl Measurement {
    /// Temperature statistics for a measurement tool, or `None` for tools whose
    /// geometry is not decoded (ellipses, alarms, differences).
    ///
    /// Measurement coordinates come from the file and are clamped to the thermal
    /// dimensions, so corrupt records cannot index out of bounds.
    pub fn measurement_stats(&self, thermal: &ImgVec<f32>) -> Option<TempStats> {
        let (w, h) = (thermal.width(), thermal.height());
        if h == 0 || w == 0 {
            return None;
        }
        let temp_at = move |x: usize, y: usize| thermal[(x.min(w - 1), y.min(h - 1))];

        let temps: Vec<f32> = match self {
            Measurement::Spot { x, y, .. } | Measurement::Endpoint { x, y, .. } => {
                vec![temp_at(*x as usize, *y as usize)]
            }
            Measurement::Area { x, y, width, height, .. } => {
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

            // Measurements for which it is not clear yet how to handle them
            Measurement::Ellipse { .. }
            | Measurement::Alarm { .. }
            | Measurement::Difference { .. } => {
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
}

/// Pixels along a line, sampled once per step on the longest axis.
fn line_points(x1: u32, y1: u32, x2: u32, y2: u32) -> impl Iterator<Item = (usize, usize)> {
    let (x1, y1, x2, y2) = (x1 as f32, y1 as f32, x2 as f32, y2 as f32);
    let steps = (x2 - x1).abs().max((y2 - y1).abs()).max(1.0) as usize;
    (0..=steps).map(move |i| {
        let t = i as f32 / steps as f32;
        ((x1 + (x2 - x1) * t).round() as usize, (y1 + (y2 - y1) * t).round() as usize)
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use imgref::Img;

    use crate::{FlirThermogram, Measurement, ThermogramTrait, fake::Fake};

    #[test]
    fn measurement_stats_spot_area_line() {
        // 2x3 grid: row 0 = [0, 1, 2], row 1 = [10, 11, 12]
        let t = Fake(Img::new(vec![0.0, 1.0, 2.0, 10.0, 11.0, 12.0], 3, 2));

        let spot = Measurement::Spot { label: "".into(), x: 2, y: 1 };
        let s = spot.measurement_stats(t.thermal()).unwrap();
        assert_eq!((s.min, s.max, s.avg), (12.0, 12.0, 12.0));

        // Area params are x, y, width, height: a 2×1 box at (1, 0)
        let area = Measurement::Area { label: "".into(), x: 1, y: 0, width: 2, height: 1 };
        let a = area.measurement_stats(t.thermal()).unwrap();
        assert_eq!((a.min, a.max, a.avg), (1.0, 2.0, 1.5));

        let line = Measurement::Line { label: "".into(), x1: 0, y1: 0, x2: 2, y2: 0 };
        let l = line.measurement_stats(t.thermal()).unwrap();
        assert_eq!((l.min, l.max, l.avg), (0.0, 2.0, 1.0));

        // Out-of-bounds coordinates clamp instead of panicking
        let spot = Measurement::Spot { label: "".into(), x: 99, y: 99 };
        let oob = spot.measurement_stats(t.thermal()).unwrap();
        assert_eq!(oob.avg, 12.0);

        let ellipse = Measurement::Ellipse { label: "".into(), params: vec![] };
        assert!(ellipse.measurement_stats(t.thermal()).is_none());
    }

    #[test]
    fn measurement_stats_ellipse() {
        // 5x5 grid with value y*10 + x
        let values: Vec<f32> =
            (0..5).flat_map(|y| (0..5).map(move |x| (y * 10 + x) as f32)).collect();
        let t = Fake(Img::new(values, 5, 5));

        // Circle: centre (2, 2), semi-axis endpoints (3, 2) and (2, 1) → radius 1.
        // Inside: (2,2), (1,2), (3,2), (2,1), (2,3) → 22, 21, 23, 12, 32.
        let circle = Measurement::Ellipse { label: "".into(), params: vec![2, 2, 3, 2, 2, 1] };
        let c = circle.measurement_stats(t.thermal()).unwrap();
        assert_eq!((c.min, c.max, c.avg), (12.0, 32.0, 22.0));

        // Degenerate axis → no stats
        let flat = Measurement::Ellipse { label: "".into(), params: vec![2, 2, 3, 2, 2, 2] };
        assert!(flat.measurement_stats(t.thermal()).is_none());
    }

    // The reference values in the two measurement tests below are the min/max/avg the
    // camera itself rendered into the JPEG overlay. Tolerances absorb the difference
    // between flyr's and FLIR's Planck evaluation, not geometry errors: a wrong region
    // (e.g. reading width/height as a second corner) is off by whole degrees.

    #[test]
    fn area_stats_match_camera_overlay() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_sc660_1.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let measurements = t.measurements();
        let area = measurements
            .iter()
            .find(|m| matches!(m, Measurement::Area { .. }))
            .expect("sc660_1 should contain an area measurement");
        let s = area.measurement_stats(t.thermal()).expect("area stats");
        // Camera overlay: Max 34.8, Min 22.7, Avg 28.1
        assert!((s.avg - 28.1).abs() < 0.1, "avg {} != 28.1", s.avg);
        assert!((s.min - 22.7).abs() < 0.5, "min {} != 22.7", s.min);
        assert!((s.max - 34.8).abs() < 0.7, "max {} != 34.8", s.max);
    }

    #[test]
    fn ellipse_stats_match_camera_overlay() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/flir_thermocam_b400_2.jpg");
        let t = FlirThermogram::from_file(Path::new(path)).expect("test thermogram");
        let measurements = t.measurements();
        let ellipse = measurements
            .iter()
            .find(|m| matches!(m, Measurement::Ellipse { .. }))
            .expect("b400_2 should contain an ellipse measurement");
        let s = ellipse.measurement_stats(t.thermal()).expect("ellipse stats");
        // Camera overlay: El1 Max -0.1
        assert!((s.max - -0.1).abs() < 0.2, "max {} != -0.1", s.max);
    }
}
