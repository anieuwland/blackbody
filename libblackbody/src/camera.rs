//! Identification of the camera that captured a thermogram.

use flyr::camera_metadata::CameraMetadata as FlyrCameraMetadata;

/// The camera details a thermogram records; `None` where the format stores none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CameraMetadata {
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,

    /// Focal length in millimetres
    pub focal_length: Option<f32>,

    /// Capture time EXIF's `YYYY:MM:DD HH:MM:SS` convention.
    pub date_time: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    /// Metres above sea level; negative is below.
    pub altitude: Option<f32>,
    pub heading: Option<f32>,
}

impl CameraMetadata {
    /// Whether every field is `None`.
    pub fn is_empty(&self) -> bool {
        *self == CameraMetadata::default()
    }

    /// Make and model joined, skipping a make the model already repeats.
    pub fn description(&self) -> Option<String> {
        match (self.make.as_deref(), self.model.as_deref()) {
            (Some(make), Some(model)) if model.starts_with(make) => Some(model.to_string()),
            (Some(make), Some(model)) => Some(format!("{make} {model}")),
            (make, model) => make.or(model).map(str::to_string),
        }
    }

    pub fn coordinates(&self) -> Option<(f64, f64)> {
        Some((self.latitude?, self.longitude?))
    }
}

impl From<&FlyrCameraMetadata> for CameraMetadata {
    fn from(m: &FlyrCameraMetadata) -> Self {
        CameraMetadata {
            make: m.make.clone(),
            model: m.model.clone(),
            serial_number: None,
            focal_length: m.focal_length,
            date_time: m.date_time.clone(),
            latitude: m.gps_latitude,
            longitude: m.gps_longitude,
            altitude: m.gps_altitude,
            heading: m.gps_img_direction,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(make: Option<&str>, model: Option<&str>) -> CameraMetadata {
        CameraMetadata {
            make: make.map(str::to_string),
            model: model.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn default_is_empty_and_any_field_fills_it() {
        assert!(CameraMetadata::default().is_empty());
        assert!(!info(Some("FLIR"), None).is_empty());
    }

    #[test]
    fn description_does_not_repeat_the_make() {
        assert_eq!(info(Some("FLIR"), Some("E5")).description().as_deref(), Some("FLIR E5"));
        assert_eq!(
            info(Some("Fluke"), Some("Fluke Ti400")).description().as_deref(),
            Some("Fluke Ti400")
        );
        assert_eq!(info(None, Some("HT-04D")).description().as_deref(), Some("HT-04D"));
        assert_eq!(info(Some("HTI"), None).description().as_deref(), Some("HTI"));
        assert_eq!(info(None, None).description(), None);
    }

    #[test]
    fn coordinates_need_both_halves() {
        let mut m = CameraMetadata { latitude: Some(52.0), ..Default::default() };
        assert_eq!(m.coordinates(), None);
        m.longitude = Some(4.3);
        assert_eq!(m.coordinates(), Some((52.0, 4.3)));
    }
}
