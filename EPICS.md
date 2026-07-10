# Blackbody — epics & work items

Checkboxes track completion. Epics are ordered; later epics may depend on earlier ones.

---

## Epic 1 — Correctness bugs

Fix before anything else — these are wrong today, not missing features.

- [x] `normalized_minmax` — dead code with a broken formula; removed entirely
- [x] Double render on imagery toggle — replaced `connect_toggle_callback` / `connect_clicked` with `set_on_change` called from `activate_toggle`; one draw path, no spurious re-renders on re-click
- [x] U32/U64 TIFF decoding: doc said "transmute as f32 bit pattern" but code (and intent) is centikelvin like U16 — doc corrected
- [x] Dual EXIF orientation — upgraded to flyr 0.6.0 (via local `flyr-rs`); `celsius_array()` and `optical_array()` now apply orientation internally; removed blackbody's redundant `correct_orientation` / `orientation` / rexif pass
- [x] Version mismatch — all three (`Cargo.toml`, `meson.build`, About dialog) set to 2.0.0
- [x] EXIF warnings on file open — rexif was already unused after the orientation fix; removed the dependency entirely, warnings gone

---

## Epic 2 — Interactive UI design

Design gate before the GTK4 port locks in structure. No deliverable code — outputs are decisions that prevent rework in every later epic.

- [ ] **Prior art survey** — FLIR Tools, GNOME Loupe, darktable, ImageJ; extract what works and what to avoid
- [ ] **HIG pattern mapping** — assign each feature from epics 3–7 to its correct HIG surface (header bar, side panel, toolbar, popover, overlay)
- [ ] **Information architecture** — decide always-visible vs on-demand for thermometer bar, metadata, measurements; single-window multi-pane vs slide-in panels
- [ ] **Header bar audit** — temperature spinners don't belong there per HIG; find them a new home
- [ ] **Wireframe: main view** — toolbar anatomy, pane layout, thermometer position
- [ ] **Wireframe: metadata / capture-parameters inspector**
- [ ] **Wireframe: measurement overlay + list panel**
- [ ] **Wireframe: palette picker**
- [ ] **HIG + prior-art critique pass** on all wireframes before any Rust is written
- [ ] **Widget inventory** — commit to specific libadwaita widgets for each surface (feeds Epic 3 scope directly)

---

## Epic 3 — Modernize the stack (GTK4 + libadwaita)

Needed for "modern Linux-native GNOME" credibility. Do after the design is settled so the port targets the right structure.

- [ ] Port to `gtk4` + `libadwaita` crates
- [ ] `AdwApplicationWindow` for free dark-mode, window decoration, responsive breakpoints
- [ ] Replace manual dark-theme hack with `AdwStyleManager`
- [ ] Header bar → `AdwToolbarView` + primary menu (HIG pattern)
- [ ] `GtkComboBoxText` → `AdwComboRow`
- [ ] `GtkImage` + manual pixbuf → `GtkPicture`
- [ ] Popovers → `AdwDialog` or inline `AdwActionRow`s where appropriate
- [ ] Remove dead `render_palette_model` GtkListStore artifact

---

## Epic 4 — Surface the data already decoded

The `flyr` crate decodes all of this; blackbody silently discards it at the library boundary. Zero new parsing needed.

- [ ] **Camera metadata panel** — make/model, serial number, lens
- [ ] **Capture parameters panel** — emissivity, reflected temp, atmospheric distance/humidity, object distance
- [ ] **Planck constants display** — for validation and scientific use
- [ ] **Camera's embedded display range** — `embedded_range()` as a "camera default" preset alongside the min/max controls
- [ ] **Spot / area / line measurements** — overlay on image (crosshairs, bounding boxes) with temperature labels; also list in a panel
- [ ] **PIP rendering** — `pip_info` geometry is already decoded; wire up the permanently-insensitive Picture-in-Picture button

---

## Epic 5 — Thermometer / colour legend

The bar exists but only as a hover tooltip. No labels, hidden behind a checkbox buried in a popover.

- [ ] Draw min/max temperature labels at top and bottom of the bar
- [ ] Draw 3–5 intermediate tick marks with temperature values
- [ ] Make the bar first-class visible (not opt-in via a palette popover checkbox)
- [ ] Interactive range: drag the endpoints to adjust display range, replacing the two spinners as the primary range interaction

---

## Epic 6 — Optical / thermal integration

Optical is currently scaled to thermal resolution (discards the optical sensor's higher native res); PIP is unimplemented.

- [ ] Show optical at its native resolution with its own zoom level, or zoom-to-match
- [ ] Auto-zoom sync when toggling between thermal and optical (the TODO already exists in `imagery_toggles.rs`)
- [ ] Implement PIP overlay using `pip_info` alignment geometry (unblocks the PIP button from Epic 4)

---

## Epic 7 — Settings persistence

`eu.nimmerfort.blackbody.gschema.xml` is completely empty; nothing survives a restart.

- [ ] Last used palette
- [ ] Last used temperature range mode (auto / manual / embedded)
- [ ] Window geometry
- [ ] Last opened directory
- [ ] Thermometer bar visibility

---

## Epic 8 — Error handling & robustness

- [ ] Introduce a proper error type in `libblackbody` (TODOs already name this)
- [ ] Surface errors as toasts or alert dialogs (use `AdwToastOverlay` / `AdwAlertDialog` once on GTK4)
- [ ] Fix `.unwrap()` panics on non-UTF-8 filenames in `FlirThermogram::identifier()` and `TiffThermogram::identifier()`

---

## Epic 9 — Palette exposure

~80 matplotlib palettes are compiled in; only 10 reach the UI.

- [ ] Replace `ComboBoxText` with a searchable/scrollable picker (grid of swatches or a full list)
- [ ] Group by type: perceptually uniform, diverging, sequential, classic
- [ ] Favourite / pinned palettes
