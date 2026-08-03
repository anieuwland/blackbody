use flyr::measurement_info::Measurement as Flir;
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
            Flir::Spot { label, x, y } => Measurement::Spot {
                label: label.clone(),
                x: (*x).into(),
                y: (*y).into(),
            },
            Flir::Ellipse { label, params } => {
                Measurement::Ellipse { label: label.clone(), params: widen(params) }
            }
            // FLIR area params are x, y, width, height; flyr misnames them x2/y2.
            Flir::Area { label, x1, y1, x2, y2 } => Measurement::Area {
                label: label.clone(),
                x: (*x1).into(),
                y: (*y1).into(),
                width: (*x2).into() ,
                height: (*y2).into(),
            },
            Flir::Line { label, x1, y1, x2, y2 } => Measurement::Line {
                label: label.clone(),
                x1: (*x1).into(),
                y1: (*y1).into(),
                x2: (*x2).into(),
                y2: (*y2).into(),
            },
            Flir::Endpoint { label, x, y } => Measurement::Endpoint {
                label: label.clone(),
                x: (*x).into(),
                y: (*y).into(),
            },
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
                // TODO Move the division by 2 to the domain model (specific to 1 camera)
                x: (coords.x / 2).into(),
                y: (coords.y / 2).into(),
            },
            Fluke::Box { start, end, metadata } => Measurement::Area {
                label: metadata.label2.clone(),
                // TODO Move the division by 2 to the domain model (specific to 1 camera)
                x: start.x.min(end.x) / 2,
                y: start.y.min(end.y) / 2,
                width: end.x.abs_diff(start.x) / 2,
                height: end.y.abs_diff(start.y) / 2,
            },
        }
    }
}
