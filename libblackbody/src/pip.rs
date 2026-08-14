//! The shared picture-in-picture compositor: formats translate their stored geometry
//! into a [`PipGeometry`], which [`composite`] crops, resizes and overlays.

use imgref::{Img, ImgVec};
use rgb::{ComponentBytes, FromSlice, RGB8};

/// An axis-aligned rectangle in pixel coordinates.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PipRect {
    pub x: i64,
    pub y: i64,
    pub width: u32,
    pub height: u32,
}

/// Picture-in-picture geometry: which part of the thermal image is shown,
/// and where it lands on the visual light image.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PipGeometry {
    /// Region of the thermal render shown, in thermal pixels. Must lie within the thermal.
    pub source: PipRect,
    /// Where the source lands, in visual pixels. May fall outside the visual; it is clipped.
    pub destination: PipRect,
}

/// Crops the render to `geometry.source`, resizes it to `geometry.destination` and overlays
/// it there, clipped to the visual's bounds. The result has the visual's dimensions.
///
/// Returns `None` when either rectangle is empty or the source does not fit the render.
pub fn composite(
    visual: &ImgVec<RGB8>,
    render: &ImgVec<RGB8>,
    geometry: &PipGeometry,
) -> Option<ImgVec<RGB8>> {
    let (source, destination) = (geometry.source, geometry.destination);
    if source.width == 0 || source.height == 0 || destination.width == 0 || destination.height == 0
    {
        return None;
    }
    let source_fits = source.x >= 0
        && source.y >= 0
        && source.x + i64::from(source.width) <= render.width() as i64
        && source.y + i64::from(source.height) <= render.height() as i64;
    if !source_fits {
        return None;
    }

    let render = image::RgbImage::from_raw(
        render.width() as u32,
        render.height() as u32,
        render.buf().as_bytes().to_vec(),
    )?;
    let crop = image::imageops::crop_imm(
        &render,
        source.x as u32,
        source.y as u32,
        source.width,
        source.height,
    )
    .to_image();
    let scaled = image::imageops::resize(
        &crop,
        destination.width,
        destination.height,
        image::imageops::FilterType::Triangle,
    );

    let mut base = image::RgbImage::from_raw(
        visual.width() as u32,
        visual.height() as u32,
        visual.buf().as_bytes().to_vec(),
    )?;
    image::imageops::overlay(&mut base, &scaled, destination.x, destination.y);

    let (width, height) = (base.width() as usize, base.height() as usize);
    Some(Img::new(base.into_raw().as_rgb().to_vec(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(color: RGB8, width: usize, height: usize) -> ImgVec<RGB8> {
        Img::new(vec![color; width * height], width, height)
    }

    const BLUE: RGB8 = RGB8::new(0, 0, 255);
    const RED: RGB8 = RGB8::new(255, 0, 0);

    fn rect(x: i64, y: i64, width: u32, height: u32) -> PipRect {
        PipRect { x, y, width, height }
    }

    #[test]
    fn overlays_source_at_destination() {
        let visual = solid(BLUE, 8, 8);
        let render = solid(RED, 4, 4);
        let geometry = PipGeometry { source: rect(0, 0, 4, 4), destination: rect(2, 2, 4, 4) };
        let img = composite(&visual, &render, &geometry).expect("composites");

        assert_eq!([img.width(), img.height()], [8, 8]);
        assert_eq!(img[(1usize, 1usize)], BLUE); // outside destination
        assert_eq!(img[(2usize, 2usize)], RED); // inside
        assert_eq!(img[(5usize, 5usize)], RED); // inside, far corner
        assert_eq!(img[(6usize, 6usize)], BLUE); // outside
    }

    #[test]
    fn scales_source_to_destination_size() {
        let visual = solid(BLUE, 10, 10);
        let render = solid(RED, 2, 2);
        let geometry = PipGeometry { source: rect(0, 0, 2, 2), destination: rect(0, 0, 10, 10) };
        let img = composite(&visual, &render, &geometry).expect("composites");
        assert!(img.pixels().all(|p| p == RED));
    }

    /// FLIR offsets can push the overlay past the visual's edges; it must clip, not wrap or panic.
    #[test]
    fn negative_destination_origin_clips() {
        let visual = solid(BLUE, 4, 4);
        let render = solid(RED, 4, 4);
        let geometry = PipGeometry { source: rect(0, 0, 4, 4), destination: rect(-2, -2, 4, 4) };
        let img = composite(&visual, &render, &geometry).expect("composites");
        assert_eq!(img[(0usize, 0usize)], RED); // clipped overlay reaches here
        assert_eq!(img[(1usize, 1usize)], RED);
        assert_eq!(img[(2usize, 2usize)], BLUE); // beyond the overlay
    }

    #[test]
    fn empty_rectangles_yield_none() {
        let visual = solid(BLUE, 4, 4);
        let render = solid(RED, 4, 4);
        let empty_source = PipGeometry { source: rect(0, 0, 0, 4), destination: rect(0, 0, 4, 4) };
        let empty_destination =
            PipGeometry { source: rect(0, 0, 4, 4), destination: rect(0, 0, 4, 0) };
        assert_eq!(composite(&visual, &render, &empty_source), None);
        assert_eq!(composite(&visual, &render, &empty_destination), None);
    }

    /// A source outside the render means corrupt geometry; refuse rather than crop silently.
    #[test]
    fn source_outside_render_yields_none() {
        let visual = solid(BLUE, 8, 8);
        let render = solid(RED, 4, 4);
        let past_edge = PipGeometry { source: rect(2, 2, 4, 4), destination: rect(0, 0, 4, 4) };
        let negative = PipGeometry { source: rect(-1, 0, 4, 4), destination: rect(0, 0, 4, 4) };
        assert_eq!(composite(&visual, &render, &past_edge), None);
        assert_eq!(composite(&visual, &render, &negative), None);
    }
}
