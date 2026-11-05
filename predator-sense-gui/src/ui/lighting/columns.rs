//! How the Lighting page is laid out: one column, or on a wide page the top band over two
//! columns, the top band staying put.
//!
//! One column: the title, "Lighting for", the preset
//! row and the tab strip (the band), the preview card, the tabs' panes and the idle
//! settings, in one scrolled window and an 880 px clamp. Two columns keep the band fixed
//! across the top and put two scrolled windows under it: the left one (about 60 %) with the
//! preview card (its bar and the brightness are part of it) and the idle settings, the
//! right one (40 %, 540 to 640 px, `frame_model::column_widths`) with the tabs' panes.
//!
//! The left scrolled window is the one-column page's own, so the preview card and the idle
//! settings never move: the drawing keeps its size cache, its tick, its key choice and Pause.
//! Only the band and the panes' stack change parent, each as a whole, which keeps what their
//! widgets hold (an open colour picker, a half-typed preset name, the gallery's cards; the
//! debounced slider edits are the page's own). The keyboard focus stays on the widget that
//! had it (`Arrangement::set`).

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{glib, graphene, gsk};
use libadwaita as adw;
use std::cell::Cell;

use crate::ui::lighting::frame_model::{self, PageLayout, COLUMN_GAP, PAGE_MARGIN};

/// One column: at most 880 px wide (the reference design's), the clamp giving width back below 600.
const ONE_COLUMN_MOST: i32 = 880;
const ONE_COLUMN_TIGHTENING: i32 = 600;
/// Two columns: the left one fills its scrolled window (no clamp).
const UNCLAMPED: i32 = 100_000;

/// A widget's four margins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Margins {
    top: i32,
    bottom: i32,
    start: i32,
    end: i32,
}

impl Margins {
    const NONE: Margins = Margins {
        top: 0,
        bottom: 0,
        start: 0,
        end: 0,
    };

    fn apply(self, widget: &impl IsA<gtk::Widget>) {
        widget.set_margin_top(self.top);
        widget.set_margin_bottom(self.bottom);
        widget.set_margin_start(self.start);
        widget.set_margin_end(self.end);
    }
}

/// Where everything sits in each layout.
struct Spacing {
    /// Around the column in the left (or only) scrolled window.
    shell: Margins,
    /// Around the band.
    band: Margins,
    /// Above the two columns, under the tab strip.
    columns_top: i32,
    /// Above the preview card and the tabs' panes.
    preview_top: i32,
    stack_top: i32,
    /// The clamp's most and tightening threshold.
    clamp: (i32, i32),
}

impl Spacing {
    fn of(layout: PageLayout) -> Spacing {
        match layout {
            // The page in one column, as it was before the wide layout.
            PageLayout::OneColumn => Spacing {
                shell: Margins {
                    top: 24,
                    bottom: 32,
                    start: 16,
                    end: 16,
                },
                band: Margins::NONE,
                columns_top: 0,
                preview_top: 14,
                stack_top: 18,
                clamp: (ONE_COLUMN_MOST, ONE_COLUMN_TIGHTENING),
            },
            // The reference design's `.lay-b.stay`: the page's 24 px margins at the top and sides,
            // the
            // columns 14 px under the tab strip, 12 px under the left column's end.
            PageLayout::TwoColumns => Spacing {
                shell: LEFT,
                band: Margins {
                    top: PAGE_MARGIN,
                    bottom: 0,
                    start: PAGE_MARGIN,
                    end: PAGE_MARGIN,
                },
                columns_top: 14,
                preview_top: 0,
                stack_top: 0,
                clamp: (UNCLAMPED, UNCLAMPED),
            },
        }
    }
}

/// The two columns' margins inside their scrolled windows (`frame_model::column_split`):
/// the page's margin on the outer side, half the gap on the inner one, and the reference design's
/// space under each column's end (12 px on the left, 32 on the right).
const LEFT: Margins = Margins {
    top: 0,
    bottom: 12,
    start: PAGE_MARGIN,
    end: COLUMN_GAP / 2,
};
const RIGHT: Margins = Margins {
    top: 0,
    bottom: 32,
    start: COLUMN_GAP / 2,
    end: PAGE_MARGIN,
};

mod imp {
    use gtk4 as gtk;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    /// The two scrolled windows side by side, split by `frame_model::column_split`; while
    /// the right one is hidden the left one has the whole width.
    #[derive(Default)]
    pub struct Columns;

    #[glib::object_subclass]
    impl ObjectSubclass for Columns {
        const NAME: &'static str = "PredatorSenseLightingColumns";
        type Type = super::Columns;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Columns {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Columns {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (mut least, mut natural) = (0, 0);
            for child in self.obj().laid_out() {
                let (l, n, _, _) = child.measure(orientation, -1);
                if orientation == gtk::Orientation::Horizontal {
                    least += l;
                    natural += n;
                } else {
                    least = least.max(l);
                    natural = natural.max(n);
                }
            }
            (least, natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            self.obj().place(width, height);
        }
    }
}

glib::wrapper! {
    /// See [`imp::Columns`].
    pub struct Columns(ObjectSubclass<imp::Columns>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Columns {
    fn new(left: &impl IsA<gtk::Widget>, right: &impl IsA<gtk::Widget>) -> Columns {
        let columns: Columns = glib::Object::new();
        left.set_parent(&columns);
        right.set_parent(&columns);
        columns
    }

    /// The children GTK lays out (the shown ones), left first.
    fn laid_out(&self) -> Vec<gtk::Widget> {
        let mut out = Vec::new();
        let mut child = self.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.should_layout() {
                out.push(widget);
            }
        }
        out
    }

    fn place(&self, width: i32, height: i32) {
        match self.laid_out().as_slice() {
            [left, right] => {
                let least = (
                    left.measure(gtk::Orientation::Horizontal, -1).0,
                    right.measure(gtk::Orientation::Horizontal, -1).0,
                );
                let (left_width, right_width) = frame_model::column_split(width, least);
                left.allocate(left_width, height, -1, None);
                let at =
                    gsk::Transform::new().translate(&graphene::Point::new(left_width as f32, 0.0));
                right.allocate(right_width, height, -1, Some(at));
            }
            [only] => only.allocate(width, height, -1, None),
            _ => {}
        }
    }
}

/// The page's breakpoint for two columns: from [`frame_model::two_columns_from`] px of width
/// at `font_scale`, `set` runs with [`PageLayout::TwoColumns`] when it applies and with
/// [`PageLayout::OneColumn`] when it stops applying. It is above both of the narrow
/// layouts' widths at every font scale, so it never applies together with them.
pub(crate) fn breakpoint(font_scale: f64, set: impl Fn(PageLayout) + 'static) -> adw::Breakpoint {
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MinWidth,
        frame_model::two_columns_from(font_scale),
        adw::LengthUnit::Px,
    ));
    let set = std::rc::Rc::new(set);
    {
        let set = set.clone();
        breakpoint.connect_apply(move |_| set(PageLayout::TwoColumns));
    }
    breakpoint.connect_unapply(move |_| set(PageLayout::OneColumn));
    breakpoint
}

/// The page's containers and the widgets that change place between the layouts.
pub(crate) struct Arrangement {
    /// What the page's breakpoint bin holds.
    pub frame: gtk::Box,
    band: gtk::Widget,
    preview: gtk::Widget,
    stack: gtk::Widget,
    /// The one column's scrolled window (the left column's in two), then the right column's.
    columns: Columns,
    clamp: adw::Clamp,
    shell: gtk::Box,
    right: gtk::ScrolledWindow,
    right_box: gtk::Box,
    layout: Cell<PageLayout>,
}

/// A scrolled window that scrolls up and down only, and asks for no width of its own.
fn scroller(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);
    scroll.set_child(Some(child));
    scroll
}

impl Arrangement {
    /// The page from top to bottom: `band` (the title, "Lighting for", the preset row and
    /// the tab strip), the preview card, the tabs' panes (`stack`) and the idle settings.
    /// It starts in one column.
    pub fn new(
        band: &impl IsA<gtk::Widget>,
        preview: &impl IsA<gtk::Widget>,
        stack: &impl IsA<gtk::Widget>,
        idle: &impl IsA<gtk::Widget>,
    ) -> Arrangement {
        let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shell.append(band);
        shell.append(preview);
        shell.append(stack);
        shell.append(idle);
        let clamp = adw::Clamp::new();
        clamp.set_child(Some(&shell));
        let left = scroller(&clamp);
        let right_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        RIGHT.apply(&right_box);
        let right = scroller(&right_box);
        right.set_visible(false);
        let columns = Columns::new(&left, &right);
        columns.set_hexpand(true);
        columns.set_vexpand(true);
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.append(&columns);
        let arrangement = Arrangement {
            frame,
            band: band.clone().upcast(),
            preview: preview.clone().upcast(),
            stack: stack.clone().upcast(),
            columns,
            clamp,
            shell,
            right,
            right_box,
            layout: Cell::new(PageLayout::OneColumn),
        };
        arrangement.space(PageLayout::OneColumn);
        arrangement
    }

    /// Lays the page out as `layout`: the band and the panes' stack change parent, and the
    /// keyboard focus, when it is on the page, stays on the same widget.
    ///
    /// The breakpoint bin hides the page while it switches (`gtk_widget_set_child_visible`),
    /// so GTK moves a focus inside it elsewhere once the frame is drawn, and libadwaita gives
    /// it back only on the next frame, by then often to the page's first control. Taking the
    /// focus again here keeps it in place. A focus inside a part that moves is let go before
    /// the move, so GTK forgets the old chain of focused containers, and taken after it, so
    /// the new chain is set and its column scrolls to it.
    pub fn set(&self, layout: PageLayout) {
        if self.layout.replace(layout) == layout {
            return;
        }
        let root = self.frame.root();
        let focus = root
            .as_ref()
            .and_then(|root| root.focus())
            .filter(|focus| focus.is_ancestor(&self.frame));
        let moving = focus.as_ref().is_some_and(|focus| self.moves(focus));
        if let (Some(root), true) = (&root, moving) {
            root.set_focus(None::<&gtk::Widget>);
        }
        match layout {
            PageLayout::TwoColumns => {
                self.shell.remove(&self.band);
                self.frame.prepend(&self.band);
                self.shell.remove(&self.stack);
                self.right_box.append(&self.stack);
            }
            PageLayout::OneColumn => {
                self.frame.remove(&self.band);
                self.shell.prepend(&self.band);
                self.right_box.remove(&self.stack);
                self.shell
                    .insert_child_after(&self.stack, Some(&self.preview));
            }
        }
        self.right.set_visible(layout == PageLayout::TwoColumns);
        self.space(layout);
        if let Some(focus) = focus {
            take_focus(&focus);
        }
    }

    /// Whether `widget` is in the band or the panes' stack, the parts that move.
    fn moves(&self, widget: &gtk::Widget) -> bool {
        [&self.band, &self.stack]
            .into_iter()
            .any(|moving| widget == moving || widget.is_ancestor(moving))
    }

    fn space(&self, layout: PageLayout) {
        let spacing = Spacing::of(layout);
        spacing.shell.apply(&self.shell);
        spacing.band.apply(&self.band);
        self.columns.set_margin_top(spacing.columns_top);
        self.preview.set_margin_top(spacing.preview_top);
        self.stack.set_margin_top(spacing.stack_top);
        let (most, tightening) = spacing.clamp;
        self.clamp.set_maximum_size(most);
        self.clamp.set_tightening_threshold(tightening);
    }
}

/// Gives `widget` the keyboard focus again. A text field is not made to select all of its
/// text, as a plain grab would. (On the next frame libadwaita's breakpoint bin takes the focus
/// again with a plain grab, as it does at every breakpoint, the narrow ones too.)
pub(crate) fn take_focus(widget: &gtk::Widget) {
    match widget.downcast_ref::<gtk::Text>() {
        Some(text) => text.grab_focus_without_selecting(),
        None => widget.grab_focus(),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita::prelude::BreakpointBinExt;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// The page's parts as stand-ins of the sizes that matter: a 240 px band, a 560 px
    /// preview and 300 px of idle settings on the left, 1500 px of panes on the right.
    struct Page {
        band: gtk::Box,
        preview: gtk::Box,
        stack: gtk::Box,
        idle: gtk::Box,
        arrangement: Arrangement,
    }

    fn block(height: i32) -> gtk::Box {
        let block = gtk::Box::new(gtk::Orientation::Vertical, 0);
        block.set_size_request(200, height);
        block
    }

    fn stand_ins() -> Page {
        let band = block(240);
        let preview = block(560);
        let stack = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stack.append(&block(1500));
        let idle = block(300);
        let arrangement = Arrangement::new(&band, &preview, &stack, &idle);
        Page {
            band,
            preview,
            stack,
            idle,
            arrangement,
        }
    }

    fn widget(w: &impl IsA<gtk::Widget>) -> gtk::Widget {
        w.clone().upcast()
    }

    impl Page {
        fn left(&self) -> gtk::ScrolledWindow {
            self.arrangement
                .columns
                .first_child()
                .and_downcast()
                .expect("the left scrolled window")
        }

        fn right(&self) -> gtk::ScrolledWindow {
            self.arrangement.right.clone()
        }

        /// The one column's (the left column's) widgets, top to bottom.
        fn column(&self) -> Vec<gtk::Widget> {
            let mut out = Vec::new();
            let mut child = self.arrangement.shell.first_child();
            while let Some(widget) = child {
                child = widget.next_sibling();
                out.push(widget);
            }
            out
        }

        /// Lays the page out as `layout` and allocates it off screen, `width` by `height`.
        fn show(&self, layout: PageLayout, width: i32, height: i32) {
            self.arrangement.set(layout);
            let frame = &self.arrangement.frame;
            let (least, _, _, _) = frame.measure(gtk::Orientation::Vertical, width);
            frame.allocate(width, height.max(least), -1, None);
        }

        fn top_in(&self, w: &impl IsA<gtk::Widget>, of: &impl IsA<gtk::Widget>) -> f32 {
            w.compute_bounds(of).expect("placed").y()
        }
    }

    /// The breakpoint applies from 1320 px of width, 1650 px at font scale 1.25, also among
    /// the page's two narrow breakpoints (which unapplies it on the way down). On the GTK test
    /// thread; nothing is shown.
    #[test]
    fn the_breakpoint_switches_at_1320_px_scaled_with_the_font() {
        crate::ui::gtk_test::run("columns", breakpoint_switches);
    }

    fn breakpoint_switches() {
        use PageLayout::{OneColumn as One, TwoColumns as Two};
        for (scale, widths, wanted) in [
            (
                1.0,
                vec![1480, 1319, 1320, 1200, 1500, 600],
                vec![Two, One, Two, One, Two, One],
            ),
            (1.25, vec![1480, 1650, 1649], vec![Two, One]),
        ] {
            let bin = adw::BreakpointBin::new();
            bin.set_size_request(360, 200);
            bin.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
            let (three_two, dropdown) = frame_model::breakpoint_widths(scale);
            for width in [three_two, dropdown] {
                bin.add_breakpoint(adw::Breakpoint::new(adw::BreakpointCondition::new_length(
                    adw::BreakpointConditionLengthType::MaxWidth,
                    width,
                    adw::LengthUnit::Px,
                )));
            }
            let seen = Rc::new(RefCell::new(Vec::new()));
            {
                let seen = seen.clone();
                bin.add_breakpoint(breakpoint(scale, move |layout| {
                    seen.borrow_mut().push(layout)
                }));
            }
            for width in widths {
                bin.allocate(width, 900, -1, None);
            }
            assert_eq!(*seen.borrow(), wanted, "font scale {scale}");
        }
    }

    /// Two columns take the reference design's widths, 845 and 563 px on a 1480 px page, in
    /// scrolled windows that hold the page's 24 px margins and half the 24 px gap, under the
    /// band; below the breakpoint the page is the one column it was.
    #[test]
    fn two_columns_take_the_mockups_widths() {
        crate::ui::gtk_test::run("columns", two_columns_widths);
    }

    fn two_columns_widths() {
        let page = stand_ins();
        let frame = page.arrangement.frame.clone();
        page.show(PageLayout::TwoColumns, 1480, 900);
        assert_eq!(page.left().width(), 24 + 845 + 12);
        assert_eq!(page.right().width(), 12 + 563 + 24);
        assert_eq!(page.preview.width(), 845);
        assert_eq!(page.stack.width(), 563);
        assert_eq!(page.band.width(), 1480 - 2 * 24);
        assert_eq!(page.band.parent(), Some(widget(&frame)));
        assert_eq!(page.top_in(&page.band, &frame), 24.0);
        // The columns start 14 px under the band, each at the top of its scrolled window.
        assert_eq!(
            page.top_in(&page.arrangement.columns, &frame),
            (24 + 240 + 14) as f32
        );
        assert_eq!(page.top_in(&page.preview, &page.left()), 0.0);
        assert_eq!(page.top_in(&page.stack, &page.right()), 0.0);
        assert_eq!(
            page.stack.parent(),
            Some(widget(&page.arrangement.right_box))
        );
        assert_eq!(page.column(), [widget(&page.preview), widget(&page.idle)]);

        // At the breakpoint the right column is at its least.
        page.show(PageLayout::TwoColumns, 1320, 900);
        assert_eq!(page.preview.width(), 708);
        assert_eq!(page.stack.width(), 540);

        // Below it, the single column as before: band, preview, panes, idle, at most 880 px
        // with 16 px sides, 24 px from the top, the preview 14 px and the panes 18 px down.
        page.show(PageLayout::OneColumn, 1200, 900);
        assert!(!page.right().is_visible());
        assert_eq!(page.left().width(), 1200);
        assert_eq!(
            page.column(),
            [
                widget(&page.band),
                widget(&page.preview),
                widget(&page.stack),
                widget(&page.idle)
            ]
        );
        // The clamp as the page always had it (it eases towards 880 px: 873 at 1200).
        let before = adw::Clamp::new();
        before.set_maximum_size(880);
        before.set_tightening_threshold(600);
        let column = block(10);
        column.set_margin_start(16);
        column.set_margin_end(16);
        before.set_child(Some(&column));
        before.allocate(1200, 100, -1, None);
        assert!(column.width() <= 880 - 2 * 16 && column.width() > 800);
        for part in page.column() {
            assert_eq!(part.width(), column.width());
        }
        assert_eq!(page.top_in(&page.band, &page.left()), 24.0);
        let shell = &page.arrangement.shell;
        assert_eq!(page.top_in(&page.preview, shell), (240 + 14) as f32);
        assert_eq!(
            page.top_in(&page.stack, shell),
            (240 + 14 + 560 + 18) as f32
        );
        // And back: the preview card and the idle settings never changed parent.
        page.show(PageLayout::TwoColumns, 1480, 900);
        assert_eq!(page.column(), [widget(&page.preview), widget(&page.idle)]);
        assert_eq!(page.preview.width(), 845);
    }

    /// The keyboard focus stays on its widget across the switch, in a part that moves (the
    /// panes, the band's text field, whose selection the switch leaves alone) or not (the
    /// drawing), and GTK's chain of focused containers follows it. In a window that is never
    /// shown.
    #[test]
    fn the_focus_stays_on_its_widget_across_the_switch() {
        crate::ui::gtk_test::run("columns", focus_stays);
    }

    fn focus_stays() {
        let page = stand_ins();
        let tile = gtk::Button::with_label("Wave");
        page.stack.append(&tile);
        let name = gtk::Entry::new();
        name.set_text("Evening glow");
        page.band.append(&name);
        let drawing = gtk::Button::with_label("drawing");
        page.preview.append(&drawing);
        let window = gtk::Window::new();
        window.set_child(Some(&page.arrangement.frame));
        let focus = || RootExt::focus(&window);
        let a = &page.arrangement;

        assert!(tile.grab_focus());
        for layout in [
            PageLayout::TwoColumns,
            PageLayout::OneColumn,
            PageLayout::TwoColumns,
        ] {
            a.set(layout);
            assert_eq!(focus(), Some(widget(&tile)), "{layout:?}");
            let (column, other) = match layout {
                PageLayout::TwoColumns => (page.right(), page.left()),
                PageLayout::OneColumn => (page.left(), page.right()),
            };
            assert_eq!(a.columns.focus_child(), Some(widget(&column)), "{layout:?}");
            assert_eq!(
                other.focus_child(),
                None,
                "{layout:?}: the old chain is gone"
            );
        }

        assert!(name.grab_focus_without_selecting());
        name.select_region(2, 7);
        let text = name.delegate().expect("the entry's text");
        for layout in [PageLayout::OneColumn, PageLayout::TwoColumns] {
            a.set(layout);
            assert_eq!(focus(), Some(widget(&text)), "{layout:?}");
            assert_eq!(name.selection_bounds(), Some((2, 7)), "{layout:?}");
            assert_eq!(name.text(), "Evening glow");
        }

        assert!(drawing.grab_focus());
        a.set(PageLayout::OneColumn);
        assert_eq!(focus(), Some(widget(&drawing)));
        window.destroy();
    }

    /// `lighting_page::bring_into_view` scrolls the scrolled window around the widget it is
    /// given: the one column, or on a wide page that widget's own column; the band is always
    /// in view.
    #[test]
    fn bringing_into_view_scrolls_the_widgets_own_column() {
        crate::ui::gtk_test::run("columns", brought_into_view);
    }

    fn brought_into_view() {
        let page = stand_ins();
        let deep = block(40);
        page.stack.append(&deep);
        let low = block(40);
        page.idle.append(&low);
        let value = |scroll: &gtk::ScrolledWindow| scroll.vadjustment().value();

        page.show(PageLayout::TwoColumns, 1480, 700);
        crate::ui::lighting_page::bring_into_view(&deep);
        assert!(value(&page.right()) > 1000.0, "{}", value(&page.right()));
        assert_eq!(value(&page.left()), 0.0);
        crate::ui::lighting_page::bring_into_view(&low);
        assert!(value(&page.left()) > 0.0);
        let right = value(&page.right());
        crate::ui::lighting_page::bring_into_view(&page.band);
        assert_eq!(value(&page.right()), right);

        page.show(PageLayout::OneColumn, 1200, 700);
        page.left().vadjustment().set_value(0.0);
        page.show(PageLayout::OneColumn, 1200, 700);
        crate::ui::lighting_page::bring_into_view(&deep);
        assert!(value(&page.left()) > 1000.0, "{}", value(&page.left()));
    }
}
