# Code review improvements

From a full review of the non-generated code (2026-07-16). Verified against `cargo test` and `cargo clippy`, both of which are currently red.

## P1 — Broken today

- [x] **1. Corrupt TIFF crashes the app.** `src/lib/tiff.rs:48-50` unwraps `File::open`, `Decoder::new`, and `dimensions()`. Any file with a TIFF magic number that fails to decode panics the app instead of reaching the error dialog that `Thermogram::from_file`'s `Option` contract promises. Return `None` instead.
- [x] **2. `cargo test` fails: doctests don't compile.** The ```` ```rust ```` blocks in `lib.rs`, `thermogram.rs`, and `thermogram_trait.rs` are pseudo-code (`&Array<u8, Ix3>>`, missing commas, unresolved imports) — 5 doctests fail. Fix the examples or mark blocks ```` ```text ````. Related: the same broken trait listing is copy-pasted in three places with outdated signatures; keep one accurate copy in `thermogram_trait.rs`.
- [x] **3. `cargo clippy` fails: 26 `approx_constant` errors.** Generated palette tables contain values like 0.318… that clippy (deny-by-default) mistakes for approximations of 1/π. Add `#![allow(clippy::approx_constant)]` to `palettes/mod.rs`. Also sweep the ~14 real warnings (unneeded `return`s, redundant clones).
- [x] **4. Opening a file while the app is running is silently ignored.** `src/main.rs:24` handles remote command lines with `app.activate()` and discards the arguments; `cli_path` is only read from the primary instance's own `std::env::args()`. Opening a second thermogram from a file manager just re-presents the old window. Read `command_line.arguments()` in the handler and route the path to a window.
- [x] **5. All three view-mode toggles can be untoggled.** The T/O/P buttons in the `.ui` aren't grouped, and the handlers (`src/gtkui/app_window.rs:993-1003`) only act when a button becomes active — clicking the active "T" deactivates it, leaving no mode selected. Set the GTK4 `group` property in the `.ui`; that also lets `apply_mode`'s manual mutual-exclusion be deleted.

## P2 — Real bugs, harder to hit

- [x] **6. Tooltip reports wrong temperatures in optical/PIP mode and letterbox margins.** `query_tooltip` (`src/gtkui/app_window.rs:344`) always maps cursor position through the *thermal* shape, even when the displayed image is optical (different dimensions/aspect). Negative coordinates saturate to 0 in the `as usize` cast, so hovering left/above the image reports row/column 0 temperatures. Guard on `is_thermal_mode()` and negative offsets.
- [x] **7. Render-thread race: stale frames can win.** Every slider tick spawns a fresh thread (`draw_render_threaded`, `src/gtkui/app_window.rs:301`); nothing orders completions, so a slow older render can overwrite a newer one, and a drag spawns dozens of full-image renders. Fix the race with a generation counter (`Cell<u64>`, checked in the `invoke` callback before storing); fix the waste with a single worker consuming only the latest request.
- [ ] **8. Closed windows leak their full state.** Signal handlers capture strong `Rc<RefCell<AppState>>` clones; widgets own the handlers and `AppState` owns the widgets — a cycle. With the `new-window` action, every closed window permanently leaks its thermal array, BGRA buffer, and thermogram. Capture weak refs (`glib::clone!(@weak …)` pattern).

## P3 — Performance and dead weight

- [ ] **9. The whole thermogram is deep-cloned on every render.** `draw_render_threaded` does `self.thermogram.borrow().clone()` per call — the thermal f32 array plus embedded optical JPEG, copied on every slider tick. Wrap in `Arc<Thermogram>` (it already crosses a thread, so `Arc` fits).
- [ ] **10. The BGRA buffer is cloned on every draw.** The draw func (`src/gtkui/app_window.rs:1060`) clones the full pixel Vec each frame because `ImageSurface::create_for_data` takes ownership — megabytes of memcpy per frame during pan/zoom. Build the `ImageSurface` once on the main thread when a render arrives and cache it.
- [ ] **11. Delete the 38 dead palette files (~9,800 lines).** `accent autumn brbg bwr cmrmap cool cubehelix dark2 flag gist_earth gist_ncar gist_rainbow gist_stern gnuplot gnuplot2 nipy_spectral paired pastel1 pastel2 pink piyg prgn prism puor purd purples rainbow rdbu rdgy rdylbu rdylgn seismic set1 set2 set3 spring summer winter` are not declared in `palettes/mod.rs` and never compile. Delete them (or declare the few actually wanted — the UI exposes 10 of the 35 that are compiled; the rest are only justified if the published lib API needs them).
- [ ] **12. Dependency refresh.** `image 0.23` (2021) and `tiff 0.6` are several major versions behind with known RUSTSEC advisories in their decoder stacks; cargo warns that `binrw 0.15` (via flyr) will be rejected by a future Rust. Bump `image` → 0.25 and `tiff` → 0.9; check whether modern `image` can replace the direct `tiff` dependency entirely (it decodes and encodes Gray32Float TIFF now).

## P4 — Hygiene and UX polish

- [ ] **13. About dialog hardcodes "2.0.0"** (`src/gtkui/app_window.rs:723`) while Meson already generates `config::VERSION`. Use it.
- [ ] **14. Library error handling.** The lib prints errors to stdout (`thermogram.rs:63,85`), `identifier()` panics on non-UTF8 filenames (`flir.rs:121`, `tiff.rs:118` — the `FIXME` already knows), and the `Option<()>` export returns swallow failure causes (existing TODOs). One small error type across the lib closes all three.
- [ ] **15. Error dialog title is always "Could not open file"** even for export/save failures (`show_error_dialog`). Take the title as a parameter.
- [ ] **16. Export dialog extension handling.** Typing `foo.tif` with the TIFF filter yields `foo.tif.tiff` because only the exact string `tiff` is accepted (`src/gtkui/app_window.rs:650`). Accept `tif`/`tiff`; consider unifying with the newer `FileDialog` API used by "save render" (the last `#[allow(deprecated)]` holdout).
- [ ] **17. Tests require a sibling `flyr-rs` checkout** (`src/lib/flir.rs:155` uses `../flyr-rs/thermograms/…`), so `cargo test` fails on a standalone clone. Vendor the three fixture JPEGs or gate the tests on the path existing.
- [ ] **18. Arrow-key browsing resets the palette on every image.** `set_thermogram_from_path` lets an embedded camera palette override the user's explicit selection each load. Make the user's choice sticky until they pick "Camera palette".
- [ ] **19. i18n is plumbed but unused in Rust code.** gettext is initialized and the `.ui` is marked translatable, but every string in `app_window.rs` (sidebar labels, dialog titles) is a bare literal. Wrap them in `gettext()` if translations are a goal; otherwise drop the gettext init.

## Suggested order

1–5 first (user-visible breakage and CI blockers, each small), then 11 and 12 (big cleanup wins), then the rest opportunistically. Items 1, 3, 5, 13, and 15 together are roughly an hour of work.
