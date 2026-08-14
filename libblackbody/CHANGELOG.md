# Unreleased

* Picture-in-picture composites for Fluke zip-style is2 files (Ti400, Ti401P,
  TiS75+): `has_pip` and `picture_in_picture` now work on `FlukeThermogram`
* PiP is now a single shared compositor (`pip::composite`); formats implement
  only `pip_geometry`.

# 0.6.0 (2026-08-13)

Breaking changes:
* Temperatures are typed `uom::ThermodynamicTemperature` values instead of bare f32s
* `optical()` is renamed to `visual()`
* Image data is exposed as `imgref::Img` buffers

New:
* Fluke is2 support via the `serendip` crate, both older binary and newer zip-style files
* Decode 16-bit grayscale PNGs as thermograms (centikelvin)
* Export thermal data to 16-bit centikelvin PNG with `export_thermal_png`
* Access camera metadata and embedded measurements/markers, with per-measurement statistics
* Fluke thermograms use their embedded palette, if present
* `render_defaults` uses the embedded render range for FLIR and Fluke files, if present
* Optional `ndarray` feature for ndarray conversions

# 0.5.0 and earlier
* Added support for reading thermal FLIR data
* Added support for reading thermal TIFFs (single-banded ones)
* Added method to render thermal data to several palettes
