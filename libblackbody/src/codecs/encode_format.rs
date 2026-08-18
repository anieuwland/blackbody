#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeFormat {
    /// HTI/ToolTop JPEG. Rescales the visible frame, fixed measurement set, some metadata.
    Hti,
    /// InfiRay IRG. Thermal and visible frame carry over; some parameters don't.
    Irg,
    /// 32-bit float grayscale TIFF in kelvin. Thermal data only.
    ThermalTiff,
    /// 16-bit grayscale PNG in centikelvin. Thermal data only.
    ThermalPng,
}
