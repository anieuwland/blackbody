//! The widget handles of the application window, grouped by screen area,
//! and their construction from the GtkBuilder UI definition. Behaviour lives
//! in `app_window` and the topic-specific sibling modules.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::BoxExt;
use gtk4::{
    Builder, Button, DrawingArea, FileFilter, Label, MenuButton, Orientation, Overlay,
    ScrolledWindow, ToggleButton,
};
use libadwaita as adw;

use super::range_slider::RangeSlider;

/// The header bar: the display mode toggle group and the popover/sidebar buttons.
pub(super) struct HeaderUi {
    /// Toggles named "thermal", "visible" and "overlay".
    pub(super) mode_group: adw::ToggleGroup,
    pub(super) palette_button: MenuButton,
    pub(super) info_button: ToggleButton,
    pub(super) measurements_button: ToggleButton,
}

/// The thermogram canvas and its scroll viewport.
pub(super) struct CanvasUi {
    pub(super) image: DrawingArea,
    pub(super) scrolled_window: ScrolledWindow,
    pub(super) overlay: Overlay,
    pub(super) placeholder: gtk4::Box,
    /// Shown instead of the image when directory navigation hits an
    /// unloadable file.
    pub(super) error_page: adw::StatusPage,
}

/// The on-screen display floating over the canvas: the temperature range
/// slider and the zoom menu, plus the fade animations.
pub(super) struct OsdUi {
    pub(super) container: gtk4::Box,
    pub(super) show_anim: adw::TimedAnimation,
    pub(super) hide_anim: adw::TimedAnimation,
    pub(super) hide_source: Rc<Cell<Option<glib::SourceId>>>,
    pub(super) range_bar: gtk4::Box,
    pub(super) range_slider: Rc<RangeSlider>,
    pub(super) zoom_button: MenuButton,
    pub(super) zoom_label: Label,
    /// Directory navigation pill, visible only when the open file has
    /// browsable siblings.
    pub(super) nav_bar: gtk4::Box,
    pub(super) nav_prev_button: Button,
    pub(super) nav_next_button: Button,
}

/// The split view holding the info and measurements sidebars.
pub(super) struct SidebarUi {
    pub(super) split_view: adw::OverlaySplitView,
    pub(super) info: gtk4::Box,
    pub(super) measurements: gtk4::Box,
}

/// The palette popover content and the colour bar beside the canvas.
pub(super) struct PaletteUi {
    pub(super) color_bar: DrawingArea,
    pub(super) palette_box: gtk4::Box,
    pub(super) embedded_section: gtk4::Box,
    pub(super) embedded_swatch: RefCell<Option<Button>>,
    /// Standard-palette swatch buttons, keyed by their `PALETTES` index.
    pub(super) all_swatches: Rc<RefCell<Vec<(usize, Button)>>>,
}

/// All widget handles, grouped by screen area.
pub(super) struct Ui {
    pub(super) window: adw::ApplicationWindow,
    pub(super) toast_overlay: adw::ToastOverlay,
    pub(super) header: HeaderUi,
    pub(super) canvas: CanvasUi,
    pub(super) osd: OsdUi,
    pub(super) sidebar: SidebarUi,
    pub(super) palette: PaletteUi,
    pub(super) filter_thermograms: FileFilter,
    pub(super) filter_all_files: FileFilter,
}

impl HeaderUi {
    fn from_builder(builder: &Builder) -> HeaderUi {
        HeaderUi {
            mode_group: builder.object("mode_group").unwrap(),
            palette_button: builder.object("palette_button").unwrap(),
            info_button: builder.object("info_button").unwrap(),
            measurements_button: builder.object("measurements_button").unwrap(),
        }
    }
}

impl CanvasUi {
    fn from_builder(builder: &Builder) -> CanvasUi {
        CanvasUi {
            image: builder.object("viewed_image").unwrap(),
            scrolled_window: builder.object("scrolled_window").unwrap(),
            overlay: builder.object("canvas_overlay").unwrap(),
            placeholder: gtk4::Box::new(Orientation::Vertical, 24),
            error_page: adw::StatusPage::builder()
                .icon_name("image-missing-symbolic")
                .visible(false)
                .build(),
        }
    }
}

impl OsdUi {
    fn from_builder(builder: &Builder) -> OsdUi {
        let container: gtk4::Box = builder.object("osd_container").unwrap();
        let show_target = adw::PropertyAnimationTarget::new(&container, "opacity");
        let hide_target = adw::PropertyAnimationTarget::new(&container, "opacity");
        let range_bar: gtk4::Box = builder.object("range_bar").unwrap();
        let range_slider = RangeSlider::new();
        range_bar.append(range_slider.widget());
        OsdUi {
            show_anim: adw::TimedAnimation::new(&container, 0.0, 1.0, 200, show_target),
            hide_anim: adw::TimedAnimation::new(&container, 1.0, 0.0, 1000, hide_target),
            hide_source: Rc::new(Cell::new(None)),
            container,
            range_bar,
            range_slider,
            zoom_button: builder.object("zoom_button").unwrap(),
            zoom_label: builder.object("zoom_label").unwrap(),
            nav_bar: builder.object("nav_bar").unwrap(),
            nav_prev_button: builder.object("nav_prev_button").unwrap(),
            nav_next_button: builder.object("nav_next_button").unwrap(),
        }
    }
}

impl SidebarUi {
    fn from_builder(builder: &Builder) -> SidebarUi {
        SidebarUi {
            split_view: builder.object("info_split_view").unwrap(),
            info: builder.object("info_sidebar").unwrap(),
            measurements: builder.object("measurements_sidebar").unwrap(),
        }
    }
}

impl PaletteUi {
    fn from_builder(builder: &Builder) -> PaletteUi {
        PaletteUi {
            color_bar: builder.object("color_bar").unwrap(),
            palette_box: builder.object("palette_box").unwrap(),
            embedded_section: gtk4::Box::new(Orientation::Vertical, 8),
            embedded_swatch: RefCell::new(None),
            all_swatches: Rc::new(RefCell::new(Vec::new())),
        }
    }
}

impl Ui {
    pub(super) fn from_builder(builder: &Builder) -> Ui {
        Ui {
            window: builder.object("blackbody_window").unwrap(),
            toast_overlay: builder.object("toast_overlay").unwrap(),
            header: HeaderUi::from_builder(builder),
            canvas: CanvasUi::from_builder(builder),
            osd: OsdUi::from_builder(builder),
            sidebar: SidebarUi::from_builder(builder),
            palette: PaletteUi::from_builder(builder),
            filter_thermograms: builder.object("filter_thermograms").unwrap(),
            filter_all_files: builder.object("filter_all_files").unwrap(),
        }
    }
}
