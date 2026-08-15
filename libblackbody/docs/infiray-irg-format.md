# Infiray IRG format (C200/C201, P200, Vevor SC240M, Topdon, Autel, …)

Documentation of the `.irg` raw-data format used by Infiray-based thermal
cameras, which — like the HTI hardware — are rebranded widely: Vevor SC240M,
Topdon TC005, some Autel drones, and others. Written as groundwork for a
possible `IrgThermogram` codec (decoder *and* encoder) in libblackbody. Reads
naturally after `hti-tooltop-format.md`, since HTI JPEGs convert losslessly
to IRG.

Sources, in decreasing order of authority:

- [jaseg/infiray_irg](https://github.com/jaseg/infiray_irg) — a mature,
  well-tested Python decoder (MIT, on PyPI). Handles all known variants. The
  layout below was verified by hand-decoding the C200 reference header
  embedded in its source.
- [jelle737/Vevor-Thermal-Utilities](https://github.com/jelle737/Vevor-Thermal-Utilities)
  — the original reverse engineering of the Vevor SC240M variant, with a
  field table in its README.
- [jbtronics/IRImageParser](https://github.com/jbtronics/IRImageParser)'s
  `to_irg.py` — a working **encoder**: it converts HTI JPEGs into IRG files
  that the vendor tools (Infiray IR Discovery, Topdon TDView) accept. Proof
  of which fields those tools actually require.

The three sources disagree on some header fields; see "Disputed fields"
below. Where they conflict, this document follows the interpretation
consistent with the C200 reference bytes.

## The idea in one paragraph

An IRG file is about the simplest radiometric format imaginable: a 128-byte
header, then three concatenated payloads — an 8-bit contrast-stretched
preview, a 16-bit absolute-temperature array, and a visible-light JPEG.
Like HTI (and unlike FLIR), temperatures are stored **directly** as fixed
point kelvin; there is no Planck calibration math. Unlike HTI, the container
is not a viewable JPEG with data appended — it's a bare binary blob, and
cameras save a separate ordinary `.jpg` screenshot alongside it.

## File layout

All multi-byte integers are **little-endian**. Pixel data is row-major
(scan order), matching how `ImgVec`/`ThermVec` are laid out.

```
┌──────────────────────────────────────────────┐
│ Header — 128 bytes (see below)               │
├──────────────────────────────────────────────┤
│ Coarse image                                 │
│   width × height × u8   (0–255)              │
├──────────────────────────────────────────────┤
│ Fine image                                   │
│   width × height × u16  (fixed-point kelvin) │
├──────────────────────────────────────────────┤
│ Visible-light JPEG (variant-dependent;       │
│   absent on P200, optional on some C201s)    │
└──────────────────────────────────────────────┘
```

### Variants

The first two header bytes are a magic number identifying the camera family,
and three variants are known. They differ in the fine image's scale and in
what follows the fine image:

| Magic (bytes 0–1) | Family | Fine value → kelvin | After fine image |
|-------------------|--------|---------------------|------------------|
| `CA AC` | C200/C201 series | `v / 16` (but `v / 10` if flag at offset 12 is 1 — seen on Autel Evo II Dual 640T V3) | JPEG of `jpeg_length` bytes; may be absent (`jpeg_length == 0`) |
| `BA AB` | "other": Vevor SC240M, HTI conversions | `v / 10` | JPEG: all remaining bytes |
| `04 A0` | P200 series | `v / 10` (with a −0.05 K quirk: vendor Celsius uses −273.2) | JSON, e.g. `{"roi":[]}` — no JPEG |

The `CA AC` and `BA AB` variants end the header with a trailer `AC CA` at
bytes 126–127 (the magic mirrored); validate it. The P200 variant does not.

For **encoding**, use `BA AB`: it's what jbtronics' converter emits and the
vendor analysis tools demonstrably accept, and its "JPEG = rest of file"
rule is the simplest to satisfy.

### Header

Fields are **not** naturally aligned — note the single-byte separators at
offsets 12 and 21. (This tripped up one of the reference parsers; see
"Disputed fields".) Layout, verified against the C200 reference header:

| Offset | Size | Type      | Field                                                          |
|-------:|-----:|-----------|----------------------------------------------------------------|
| 0      | 2    | `u8[2]`   | Magic (see variant table)                                      |
| 2      | 2    | `u16`     | Header length, `128`                                           |
| 4      | 4    | `u32`     | Coarse section length in bytes = `width × height`              |
| 8      | 2    | `u16`     | Coarse height                                                  |
| 10     | 2    | `u16`     | Coarse width                                                   |
| 12     | 1    | `u8`      | Separator/flag: normally 0; 1 selects `v/10` scaling on `CA AC` |
| 13     | 4    | `u32`     | Fine section length in bytes = `width × height × 2`            |
| 17     | 2    | `u16`     | Fine height                                                    |
| 19     | 2    | `u16`     | Fine width                                                     |
| 21     | 1    | `u8`      | Separator: 0 or 1, meaning unknown                             |
| 22     | 4    | `u32`     | Visible JPEG length in bytes (0 if absent)                     |
| 26     | 2    | `u16`     | Visible height                                                 |
| 28     | 2    | `u16`     | Visible width                                                  |
| 30     | 4    | `u32`     | Emissivity × 10 000 (`9500` → 0.95)                            |
| 34     | 4    | `u32`     | Reflected temperature, kelvin × 10 000 (`2730000` → 273 K)     |
| 38     | 4    | `u32`     | Ambient temperature, kelvin × 10 000                           |
| 42     | 4    | `u32`     | Distance (scale disputed: ×1 000 or ×10 000; see below)        |
| 46     | 4    | `u32`     | Constant `4000`, meaning unknown                               |
| 50     | 4    | `u32`     | Transmissivity × 10 000 (`10000` → 1.0)                        |
| 54     | 4    | `u32`     | Zero                                                           |
| 58     | 4    | `u32`     | Constant `10000`, meaning unknown                              |
| 62     | 8    | —         | Zeros                                                          |
| 70     | 4    | `u32`     | `4` or `1026` observed; meaning unknown                        |
| 74     | 1    | `u8`      | Temperature unit: 0 = °C, 1 = K, 2 = °F (disputed; see below)  |
| 75     | 51   | —         | Zeros                                                          |
| 126    | 2    | `u8[2]`   | Trailer `AC CA` (`CA AC` / `BA AB` variants only)              |

Dimensions are height-first, and cameras in this family typically save
**portrait** images (a "256×192" C200 stores height 256, width 192) — the
same don't-assume-landscape caveat as HTI. Emissivity, reflected/ambient
temperature, and transmissivity mirror FLIR's Planck-pipeline inputs, but
here they are advisory metadata: the fine image already contains final
temperatures, so a decoder never needs them.

Sanity checks a decoder should make: header length is 128, coarse length
equals `width × height`, fine length equals `2 × width × height`, trailer
present for the two trailer variants, and total remaining bytes cover the
declared sections (jaseg's parser errors on truncation — do the same).

### Coarse image

One `u8` per pixel: the temperature field histogram-equalized/normalized
into 0–255, where 0 is the frame's coldest pixel and 255 the hottest. The
direct analogue of HTI's grayscale block — a pre-baked render, redundant
with the fine image. Parse-and-skip when decoding; recompute when encoding.

### Fine image

One `u16` per pixel in fixed-point kelvin — divide by the variant's scale
(16 or 10, table above) to get kelvin for a `ThermVec`. No offset needed for
kelvin; the −273.15 (or the P200's −273.2) only enters when the vendor
converts to Celsius.

Resolution note: 1/16 K on the C201 beats HTI's 1/10 °C. And since `u16`
kelvin can't go negative, the format bottoms out at 0 K and tops out at
4096 K (÷16) or 6553.5 K (÷10) — no practical constraint.

### Visible JPEG

The unprocessed visible-camera photo. On the `BA AB` variant it's simply
every byte after the fine image; on `CA AC` read exactly `jpeg_length`
bytes. In stock camera files its resolution matches or exceeds the thermal
resolution; the vendor tools require thermal and visible dimensions to
relate sanely (see encoding notes).

## Decoding: mapping onto `ThermogramTrait`

Same shape as the HTI mapping, following the `fluke.rs` adapter pattern:

| Trait method              | Source in the file                                          |
|---------------------------|-------------------------------------------------------------|
| `thermal()`               | Fine image → `v / scale` kelvin → `into_therm_vec`          |
| `visual()`                | Decode the trailing JPEG, if present                        |
| `embedded_render_range()` | Not stored — return `None`; `render_defaults` falls back to min/max |
| `measurements()`          | None stored                                                 |
| `palette()`               | Not stored (the sidecar `.jpg` bakes one in, but we don't parse it) |
| `camera_metadata()`       | Nothing beyond the magic-implied family; effectively `None` |
| `pip_geometry()`          | Not stored; `estimated_pip_geometry` is the only option     |

Decoding really is just: validate header, read three dimension pairs, slice
three sections, divide. The only branching is the magic → scale table.

## Encoding

The encoder is what makes IRG interesting for libblackbody: any
`ThermogramTrait` implementor could export to IRG, making its data readable
in Infiray IR Discovery / Topdon TDView — the same trick `to_irg.py` plays
for HTI files. Lessons from that working encoder:

1. **Emit the `BA AB` variant.** Header length 128, trailer `AC CA`,
   visible JPEG as the final section.
2. **Match thermal and visible resolutions.** IR Discovery expects them to
   correspond; `to_irg.py` upscales the thermal/coarse arrays to the visible
   resolution by integer pixel-doubling (`numpy.repeat` ×2 per axis — plain
   nearest-neighbor). Downscaling the visible instead also works. If we
   export without a visible image, `jpeg_length = 0` may work but is only
   attested on the `CA AC` variant — test before relying on it.
3. **Fixed defaults are fine.** The vendor tools accept hardcoded metadata:
   reflected/ambient 273 K–298 K range, distance 5 m, transmissivity 1.0,
   the `4000` and `10000` constants, `4` at offset 70, unit byte as desired.
   Only emissivity is worth carrying over from the source thermogram.
4. **Compute the coarse image** by min-max normalizing the fine image to
   0–255 — exactly the `render` binning logic with a 256-step grayscale
   palette, or reuse the source's grayscale block when converting from HTI.
5. **Fine values:** `round(kelvin × 10)` as `u16`. (`to_irg.py` adds a
   fudge `+1` — one deci-kelvin — noting its output otherwise reads 0.1 K
   low in the vendor tool. Off-by-one in the vendor's display rounding,
   most likely. Decide by comparing against the tool, not by copying the
   fudge blindly.)
6. **Ship the sidecar.** IR Discovery wants the camera's `.jpg` next to the
   `.irg` (same basename) to open it. When exporting from libblackbody, a
   `render_defaults()` save-out can play the sidecar role. Beware: IR
   Discovery may overwrite that JPEG with its own version.

## Disputed fields

The reference implementations genuinely disagree; recorded here so future
work doesn't re-litigate it:

- **Alignment (offsets 12–21).** jaseg's parser reads this region as aligned
  `u16`s (`flag0`, `_unk1`, `_zero1`, `fine_offset`, `_unk2`). Decoding the
  C200 reference header shows the jbtronics/Vevor reading is the coherent
  one: byte 12 separator, unaligned `u32` fine length at 13, dims at 17/19,
  separator at 21 (jaseg's parser gets away with it because it never uses
  those fields — it derives the fine size from the coarse dimensions).
  This doc uses the unaligned layout.
- **Bytes 70–77.** jbtronics/jelle737: `u32` (`4`/`1026`) at 70, unit `u8`
  at 74. jaseg: unit `u16` at 72, "high gain mode" `u32` flag at 74. In the
  C200 reference bytes, 70–73 = `1026` and 74–77 = `1`, which fits both
  readings (unit = kelvin vs. high-gain = on). Unresolved; fortunately the
  field is display-only either way. A decoder should ignore it; an encoder
  copies whatever it can attest.
- **Distance scale.** jelle737/jbtronics say ×1 000 (5 m → `5000`); jaseg
  divides by 10 000. The C200 reference value `2500` reads as either 2.5 m
  or 0.25 m. Advisory-only, so low stakes.
- **Offsets 34/38.** Interpreted here (and by jbtronics) as reflected and
  ambient temperature; jaseg calls them two copies of a "fine temperature
  zero offset" and warns if they differ. The kelvin-×10 000 encoding fits
  both stories. Again advisory-only.

## Relationship to other formats

IRG and the HTI JPEG format are two containers around the same idea —
dimensions plus an array of plain fixed-point temperatures, no calibration
math — which is why `to_irg.py` can convert between them with nothing but
byte shuffling and a unit change (HTI deci-°C → IRG deci-K). If libblackbody
grows both, they should share a "temperatures stored directly" backend, in
contrast to FLIR's Planck pipeline. IRG additionally makes a natural
*export* target for every other format, FLIR and Fluke included, since any
`ThermVec` plus optional visual is enough to write one.
