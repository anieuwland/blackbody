use imgref::{Img, ImgVec};
use rgb::{ComponentBytes, FromSlice, RGB8};
use serendip::Thermogram as Serendip;
use std::path::{Path, PathBuf};
use uom::si::{f32::ThermodynamicTemperature, thermodynamic_temperature::kelvin};

use crate::{Measurement, ThermVec, ThermogramTrait, thermal::into_therm_vec};

/// This is the struct and `ThermogramTrait` implementation for Fluke thermograms, using
/// [serendip](https://crates.io/crates/serendip).
#[derive(Clone, Debug)]
pub struct FlukeThermogram {
    pub thermogram: Serendip,
    pub file_path: PathBuf,
    thermal: ThermVec,
}

impl FlukeThermogram {
    /// Read a Fluke file (is2) referenced by a path.
    ///
    /// # Arguments
    /// * `file_path` - The path to the file to read.
    ///
    /// # Returns
    /// In case of success, `Some<FlukeThermogram>` is returned, otherwise `None`.
    pub fn from_file(file_path: &Path) -> Option<FlukeThermogram> {
        FlukeThermogram::read_thermal(file_path)
    }

    fn read_thermal(file_path: &Path) -> Option<FlukeThermogram> {
        let thermogram = Serendip::new_from_path(file_path).ok()?;
        let thermal = thermogram.kelvin()?;
        let thermal = into_therm_vec::<kelvin>(thermal.pixels(), thermal.width(), thermal.height());

        Some(FlukeThermogram { thermogram, file_path: file_path.to_path_buf(), thermal })
    }
}

impl ThermogramTrait for FlukeThermogram {
    fn thermal(&self) -> &ThermVec {
        &self.thermal
    }

    fn visual(&self) -> Option<ImgVec<RGB8>> {
        self.thermogram.visual()
    }

    fn has_visual(&self) -> bool {
        match &self.thermogram {
            Serendip::Zip(t) => !t.visuals.is_empty(),
            Serendip::Blob(t) => t.visual_data.is_some(),
        }
    }

    fn identifier(&self) -> &str {
        self.file_path.file_name().and_then(|n| n.to_str()).unwrap_or("<thermogram>")
    }

    fn path(&self) -> Option<&PathBuf> {
        Some(&self.file_path)
    }

    /// Palette in RGB, normalized to 0.0–1.0. Alpha is discarded.
    fn palette(&self) -> Option<Vec<[f32; 3]>> {
        self.thermogram.palette().map(|p| {
            p.iter().map(|c| [c.r, c.g, c.b].map(|channel| f32::from(channel) / 255.0)).collect()
        })
    }

    fn embedded_render_range(&self) -> Option<[ThermodynamicTemperature; 2]> {
        let scale = self.thermogram.embedded_render_range()?;
        Some([
            ThermodynamicTemperature::new::<kelvin>(scale[0]),
            ThermodynamicTemperature::new::<kelvin>(scale[1]),
        ])
    }

    fn measurements(&self) -> Vec<Measurement> {
        self.thermogram.markers().iter().map(Into::into).collect()
    }

    fn has_pip(&self) -> bool {
        let has_frame = match &self.thermogram {
            Serendip::Zip(t) => t.visual_bytes().is_some(),
            Serendip::Blob(_) => false,
        };
        self.thermogram.ir_footprint().is_some() && has_frame
    }

    /// Composite the thermal render onto the visual light frame using the file's
    /// embedded IR footprint geometry. Palette colors in 0.0–1.0 RGB.
    fn picture_in_picture(
        &self,
        min_temp: ThermodynamicTemperature,
        max_temp: ThermodynamicTemperature,
        palette: &[[f32; 3]],
    ) -> Option<ImgVec<RGB8>> {
        let footprint = self.thermogram.ir_footprint()?;
        let frame = self.thermogram.visual()?;

        let thermal = self.render(min_temp, max_temp, palette);
        let thermal = image::RgbImage::from_raw(
            thermal.width() as u32,
            thermal.height() as u32,
            thermal.buf().as_bytes().to_vec(),
        )?;
        let scaled = image::imageops::resize(
            &thermal,
            footprint.width,
            footprint.height,
            image::imageops::FilterType::Triangle,
        );

        let mut base = image::RgbImage::from_raw(
            frame.width() as u32,
            frame.height() as u32,
            frame.buf().as_bytes().to_vec(),
        )?;
        image::imageops::overlay(&mut base, &scaled, footprint.x.into(), footprint.y.into());

        let (width, height) = (base.width() as usize, base.height() as usize);
        Some(Img::new(base.into_raw().as_rgb().to_vec(), width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ti400_sample() -> FlukeThermogram {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/thermograms/fluke_ti400_1.is2");
        FlukeThermogram::from_file(Path::new(path)).expect("test thermogram")
    }

    /// The composite has the full visual frame's shape (1280×960 on the
    /// Ti400), not the thermal data's (320×240) or the display crop's
    /// (640×480, which `visual()` returns).
    #[test]
    fn pip_composite_has_visual_frame_shape() {
        let t = ti400_sample();
        assert!(t.has_pip());
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        assert_eq!([img.width(), img.height()], [1280, 960]);
    }

    /// The thermal render must actually land inside the footprint: pixels
    /// there differ from the plain visual frame, pixels outside it don't.
    #[test]
    fn pip_overlays_thermal_inside_footprint_only() {
        let t = ti400_sample();
        let img = t
            .picture_in_picture(t.min_temp(), t.max_temp(), &crate::palettes::TURBO)
            .expect("pip composite");
        let frame = t.thermogram.visual().expect("visual frame");

        // Footprint on this sample: x 399, y 252, 462 × 346 (see serendip)
        let inside = (399usize + 231, 252usize + 173);
        let outside = (10usize, 10usize);
        assert_ne!(img[inside], frame[inside]);
        assert_eq!(img[outside], frame[outside]);
    }
}
