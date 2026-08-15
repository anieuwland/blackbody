# HTI / ToolTop JPEG format (HT-04D, ET692B, HT-19, …)

Documentation of the file format used by HTI-Xintai (a.k.a. Xintest) thermal
cameras, which are rebranded and sold under many names — the Tooltop ET692B is
an HTI HT-04D internally. Written as groundwork for a possible
`HtiThermogram` format in libblackbody.

Sources:

- [EEVblog thread: "Tooltop ET692B / HTI HT-04D: Hardware and file format
  analysis"](https://www.eevblog.com/forum/thermal-imaging/tooltop-et692b-hti-ht-04d-hardware-and-file-format-analysis/)
  by jbtronics, who reverse-engineered the format from a firmware dump using
  ghidra.
- [jbtronics/IRImageParser](https://github.com/jbtronics/IRImageParser), the
  resulting reference parser (Python, MIT). The byte layout below was verified
  against `thermo/structures.py` in that repo.

Confirmed working on HT-04D and HT-19 files; a forum member confirmed it on an
HT-18+ too. Rule of thumb: if the vendor's "IRImageTools" desktop program can
open a camera's JPEGs, this format probably applies.

## The idea in one paragraph

Like FLIR's R-JPEG, an HTI capture is a normal JPEG that any image viewer can
open — but with extra data smuggled in. FLIR hides its payload *inside* the
JPEG, in APP1 segments (see the record extraction step in `flyr`). HTI takes a
much cruder approach: it simply **appends everything after the JPEG's
end-of-image marker** (`FF D9`). Image viewers stop reading at that marker, so
they never notice the second JPEG, the raw temperature array, and the metadata
block that follow it.

A second important difference from FLIR: HTI files store **final temperatures
directly** (as 16-bit integers in tenths of a degree Celsius). There is no
Planck calibration data and no raw-sensor-counts-to-kelvin math. Decoding step
2 of the usual flyr pipeline ("thermal decode") collapses to a unit
conversion.

## File layout

All multi-byte integers are **little-endian**.

```
┌──────────────────────────────────────────────┐
│ JPEG #1 — "mixed" image                      │  what image viewers show:
│   FF D8 … FF D9                              │  the camera's screen render
├──────────────────────────────────────────────┤
│ JPEG #2 — visible-light image                │  clean photo, no overlays
│   FF D8 … FF D9                              │
├──────────────────────────────────────────────┤
│ Temperature block                            │
│   u16 width, u16 height                      │
│   width × height × i16  (deci-°C)            │
├──────────────────────────────────────────────┤
│ Grayscale block                              │
│   u16 width, u16 height  (same as above)     │
│   width × height × u8   (0–255)              │
├──────────────────────────────────────────────┤
│ Metadata block ("t_IRInfo" in the firmware)  │
│   u32 size, then `size` bytes (see below)    │
└──────────────────────────────────────────────┘
```

### JPEG #1 — mixed image

The screenshot as shown on the camera display: thermal render, possibly
blended with the visible photo, including any on-screen overlays. Useful as a
thumbnail/fallback, not as data.

### JPEG #2 — visible-light image

The unmodified photo from the visible camera. On the HT-04D it is 240×320,
i.e. exactly 2× the thermal resolution. This is what `visual()` should return.

**Finding the boundary:** the reference parser just searches for the byte
sequence `FF D9 FF D8` (end marker immediately followed by a new start
marker). That works in practice but can false-positive if the same bytes
occur inside JPEG #1's compressed data. A robust parser should walk JPEG #1's
segment structure to find its true `FF D9` — libblackbody/flyr already do
this kind of marker walking for FLIR record extraction, so reuse that
approach.

### Temperature block

A 4-byte header of two `u16`s (width, height), then one `i16` per pixel in
scan order. Each value is the temperature in **deci-degrees Celsius**, so
`253` means 25.3 °C. Conversion for a `ThermVec`:

```text
kelvin = value / 10.0 + 273.15
```

Note the dimensions describe the file's stored orientation. The HT-04D writes
portrait images (120 wide × 160 tall thermal, 240×320 visible); don't assume
landscape.

### Grayscale block

Same header, then one `u8` per pixel. This is just the temperature array
normalized to 0–255 between the frame's min and max — a pre-baked version of
what `ThermogramTrait::render` computes with a grayscale palette. It's
redundant with the temperature block, so libblackbody can parse-and-skip it
(it still must be consumed to reach the metadata).

### Metadata block

Starts with a `u32` byte count that acts as a de-facto **format version**:

- `112` — current format (HT-04D firmware 2.5.1)
- `104` — older format (seen on HT-19, firmware 2.1.19); identical but
  missing the trailing image-margins field

Reject other sizes. Layout after the size field:

| Offset | Size | Type       | Field                                                    |
|-------:|-----:|------------|----------------------------------------------------------|
| 0      | 20   | `char[20]` | Model, null-padded UTF-8, e.g. `"HT-04D"` (`devType`)    |
| 20     | 20   | `char[20]` | Firmware version, e.g. `"2.5.1"` (`devVersion`)          |
| 40     | 20   | `char[20]` | Capture time as text: `YYYY/MM/DD-HH:MM:SS`              |
| 60     | 8    | spot       | Center spot                                              |
| 68     | 8    | spot       | **Max** spot                                             |
| 76     | 8    | spot       | **Min** spot                                             |
| 84     | 4    | `u32`      | Emissivity × 100 (`95` → 0.95)                           |
| 88     | 4    | `u32`      | Palette enum (below)                                     |
| 92     | 4    | `u32`      | Unit enum: 0 = Celsius, 1 = Fahrenheit                   |
| 96     | 4    | `u32`      | Mix factor, 0–100 (0 = pure thermal, 100 = pure visible) |
| 100    | 8    | `u16[4]`   | Image margins [top, right, bottom, left] — 112-byte only |

Watch the field order: it's center, **max**, then **min**.

A **spot** is 8 bytes:

| Size | Type  | Field                             |
|-----:|-------|-----------------------------------|
| 2    | `u16` | x                                 |
| 2    | `u16` | y                                 |
| 4    | `i32` | temperature in deci-°C (`/10` °C) |

Two gotchas about spots:

1. **Coordinate space.** The reference output shows the center spot at
   (120, 160) on a camera whose *thermal* image is 120×160 — that's the
   center of the 240×320 *visible* image. So spot coordinates appear to be in
   visible/screen pixels, 2× the thermal grid on the HT-04D. (Inference from
   sample output, not confirmed against firmware; verify before relying on
   it.) Divide accordingly before using them as thermal-image coordinates in
   a `Measurement`.
2. **They won't match a naive argmin/argmax.** The camera averages a 3×3
   neighborhood before picking min/max, so the stored spots can differ
   slightly (position and value) from scanning the temperature array
   yourself.

Palette enum:

| Value | Name      |
|------:|-----------|
| 0     | Spectra   |
| 1     | Iron      |
| 2     | Cool      |
| 3     | White hot |
| 4     | Black hot |

## Mapping onto `ThermogramTrait`

How the pieces line up with `libblackbody/src/thermogram_trait.rs`, following
the pattern of the simpler adapters like `fluke.rs`:

| Trait method              | Source in the file                                                                       |
|---------------------------|------------------------------------------------------------------------------------------|
| `thermal()`               | Temperature block → deci-°C to kelvin → `into_therm_vec` → `ThermVec`                     |
| `visual()`                | Decode JPEG #2 → `ImgVec<RGB8>`                                                            |
| `embedded_render_range()` | Min/max spot temperatures                                                                  |
| `measurements()`          | The three spots as spot measurements (mind the coordinate space caveat above)              |
| `palette()`               | Palette enum → a stored copy of the vendor palette, or `None` if we don't replicate them   |
| `camera_metadata()`       | Model + firmware strings                                                                   |
| `pip_geometry()`          | Possibly derivable from image margins + the fixed 2× scale; needs experimentation          |

Not needed: the grayscale block (recomputable via `render`), the mixed JPEG
(recomputable via `render` + PiP compositing), the unit enum (display
preference only — stored temperatures are always Celsius-based), the mix
factor (display preference for the mixed image).

## Hardware background (context, not needed for parsing)

Condensed from the same thread, for the curious:

- The HT-04D runs BusyBox Linux on an Allwinner A33 (quad Cortex-A7,
  Mali-400 GPU, 512 MB RAM, 8 GB eMMC). The main camera application is a
  single binary (`/work/app/ht-04`) that still ships with symbols, which is
  what made the ghidra decompilation — and hence this format doc — easy.
- The sensor is an Infiray Tiny1C core (natively 256×192) behind an
  FPGA-to-USB bridge ("HM-TM31", Hikvision/Hikmicro USB IDs). The ET692B's
  advertised 160×120 is the same core in a windowed/electronic-zoom
  configuration; there is no documented way to unlock full resolution.
- A UART on three test pads (115200 baud) gives a U-Boot console and a Linux
  root shell. The Allwinner FEL recovery mode is also reachable via a button
  combo.
- Recent firmware has a built-in updater: at shutdown it looks for a
  specially-named packed container file on the USB mass-storage root and
  installs its contents with root privileges — the practical modding path
  (a community packer script exists in the thread).
- The infamous "OVER" reading in high-temperature mode is a display-side
  range check, not a sensor limit; the file always contains real temperature
  values, and the check has been binary-patched out by hobbyists. Good news
  for us: even images displaying "OVER" carry usable data.

## Relationship to other formats

The temperature block (dimensions + array of plain temperature integers) is
close in spirit to Infiray's IRG format; IRImageParser ships a `to_irg.py`
converter. If libblackbody ever grows IRG support, the two decoders could
share the "temperatures stored directly, no calibration math" backend, in
contrast to the Planck-equation pipeline FLIR requires.
