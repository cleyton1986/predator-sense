//! Setting a `GtkFlowBox`'s contents while it is on screen, without ever removing one of its
//! `GtkFlowBoxChild` wrappers.
//!
//! GTK's `gtk_flow_box_remove` (gtk/gtkflowbox.c, 4.24.1 and `main` alike) clears the box's
//! active and selected child but not its cursor child, which GTK sets when a child is
//! clicked, activated, or takes the keyboard focus (also a wrapper that is not focusable
//! itself, when the button inside it takes the focus). Once a rebuild removed that child, the
//! next arrow key (`move-cursor`), Enter or Space (`activate-cursor-child`), Ctrl+Space or
//! Page Up/Down reads freed memory. The Lighting page crashed that way on 2026-10-19: SIGSEGV
//! in `g_sequence_iter_next` under `gtk_flow_box_move_cursor`, after its colour swatches were
//! rebuilt. It does not crash every time, since a new child often reuses the freed memory.
//!
//! [`set_contents`] therefore keeps every wrapper a box has held: the new widgets go into the
//! existing wrappers in order, wrappers are added only when more are needed, and the spare
//! ones are emptied and hidden, which GTK's layout and keyboard navigation skip. Every box
//! whose contents change while it is shown goes through it.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;

/// How a box's wrappers are made: their accessible role (fixed when a wrapper is made, so
/// one box always uses the same) and whether they take the focus themselves (a box of
/// buttons leaves it to the buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wrappers {
    pub role: gtk::AccessibleRole,
    pub focusable: bool,
}

impl Wrappers {
    /// GTK's own: a grid cell that takes the focus.
    pub const PLAIN: Wrappers = Wrappers {
        role: gtk::AccessibleRole::GridCell,
        focusable: true,
    };

    /// Cells that only hold a focusable control (a button, a swatch).
    pub const HOLDERS: Wrappers = Wrappers {
        role: gtk::AccessibleRole::GridCell,
        focusable: false,
    };
}

/// Shows `widgets` in `flow`, in order, and returns their wrappers in that order (the callers
/// style them and map them back to their items). No wrapper is ever removed (see the module
/// doc). A widget may come from one of this box's own wrappers (it moves); one with a parent
/// elsewhere must be taken out of it first.
pub fn set_contents(
    flow: &gtk::FlowBox,
    widgets: &[gtk::Widget],
    wrappers: Wrappers,
) -> Vec<gtk::FlowBoxChild> {
    let mut existing = Vec::new();
    while let Some(wrapper) = flow.child_at_index(existing.len() as i32) {
        existing.push(wrapper);
    }
    // Free every widget first: one may move to another wrapper of this box.
    for wrapper in &existing {
        wrapper.set_child(None::<&gtk::Widget>);
    }
    let mut used = Vec::with_capacity(widgets.len());
    for (index, widget) in widgets.iter().enumerate() {
        let wrapper = match existing.get(index) {
            Some(wrapper) => {
                reset(wrapper, wrappers);
                wrapper.clone()
            }
            None => {
                let wrapper: gtk::FlowBoxChild = glib::Object::builder()
                    .property("accessible-role", wrappers.role)
                    .build();
                wrapper.set_focusable(wrappers.focusable);
                flow.insert(&wrapper, -1);
                wrapper
            }
        };
        wrapper.set_child(Some(widget));
        used.push(wrapper);
    }
    for spare in existing.iter().skip(widgets.len()) {
        reset(spare, wrappers);
        spare.set_visible(false);
    }
    used
}

/// A reused wrapper as a new one: what a caller may have set on it for its last widget goes.
fn reset(wrapper: &gtk::FlowBoxChild, wrappers: Wrappers) {
    wrapper.set_css_classes(&[]);
    wrapper.set_tooltip_text(None);
    wrapper.reset_property(gtk::AccessibleProperty::Label);
    wrapper.reset_state(gtk::AccessibleState::Checked);
    wrapper.set_focusable(wrappers.focusable);
    wrapper.set_visible(true);
}

/// The number of visible children on each of `flow`'s lines, top to bottom, as it is
/// allocated now.
#[cfg(test)]
pub(crate) fn lines(flow: &gtk::FlowBox) -> Vec<usize> {
    let mut lines: Vec<(f32, usize)> = Vec::new();
    let mut index = 0;
    while let Some(child) = flow.child_at_index(index) {
        index += 1;
        if !child.is_visible() {
            continue;
        }
        let top = child.compute_bounds(flow).expect("a placed child").y();
        match lines.last_mut() {
            Some((at, count)) if *at == top => *count += 1,
            _ => lines.push((top, 1)),
        }
    }
    lines.into_iter().map(|(_, count)| count).collect()
}

/// The number of children on `flow`'s first line as it is allocated now.
#[cfg(test)]
pub(crate) fn first_line(flow: &gtk::FlowBox) -> usize {
    lines(flow).first().copied().unwrap_or(0)
}

/// The number of children on `flow`'s first line when it is allocated `width` px wide (off
/// screen).
#[cfg(test)]
pub(crate) fn first_line_at(flow: &gtk::FlowBox, width: i32) -> usize {
    let (_, height, _, _) = flow.measure(gtk::Orientation::Vertical, width);
    flow.allocate(width, height, -1, None);
    first_line(flow)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrappers_of(flow: &gtk::FlowBox) -> Vec<gtk::FlowBoxChild> {
        let mut out = Vec::new();
        while let Some(wrapper) = flow.child_at_index(out.len() as i32) {
            out.push(wrapper);
        }
        out
    }

    /// Runs on the GTK test thread (`ui::gtk_test`). Without a display it says so and passes.
    #[test]
    fn a_box_keeps_every_wrapper_it_ever_held() {
        crate::ui::gtk_test::run("flow_box", a_box_keeps_every_wrapper);
    }

    fn a_box_keeps_every_wrapper() {
        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        let mut ever: Vec<gtk::FlowBoxChild> = Vec::new();
        let moved: gtk::Widget = gtk::Button::with_label("+").upcast();
        let rounds: [&[&str]; 6] = [
            &["a", "b", "c"],
            &["d", "e", "f", "g", "h"],
            &["i", "j"],
            &["k", "l", "m", "n"],
            &[],
            &["o"],
        ];
        for (round, names) in rounds.iter().enumerate() {
            let mut widgets: Vec<gtk::Widget> = names
                .iter()
                .map(|n| gtk::Label::new(Some(n)).upcast())
                .collect();
            // The same widget moves between wrappers from round to round ("+" in a row).
            if round % 2 == 0 {
                widgets.push(moved.clone());
            } else {
                widgets.insert(0, moved.clone());
            }
            let used = set_contents(&flow, &widgets, Wrappers::HOLDERS);
            for wrapper in &used {
                if !ever.contains(wrapper) {
                    ever.push(wrapper.clone());
                }
            }
            let all = wrappers_of(&flow);
            for wrapper in &ever {
                assert!(
                    all.contains(wrapper),
                    "round {round}: a wrapper left the box"
                );
            }
            let shown: Vec<gtk::FlowBoxChild> =
                all.iter().filter(|w| w.is_visible()).cloned().collect();
            assert_eq!(shown, used, "round {round}: the visible wrappers, in order");
            let held: Vec<Option<gtk::Widget>> = shown.iter().map(|w| w.child()).collect();
            let wanted: Vec<Option<gtk::Widget>> =
                widgets.iter().map(|w| Some(w.clone())).collect();
            assert_eq!(held, wanted, "round {round}: the widgets, in order");
            for spare in all.iter().filter(|w| !w.is_visible()) {
                assert!(
                    spare.child().is_none(),
                    "round {round}: a spare holds nothing"
                );
            }
            assert!(used.iter().all(|w| !w.is_focusable()));
        }
        assert_eq!(
            wrappers_of(&flow).len(),
            6,
            "never more wrappers than at most shown"
        );

        // What crashed: the cursor on a wrapper the next contents do not need, then the arrow
        // keys and Enter. The wrapper is kept (hidden), so the cursor stays valid memory.
        let widgets: Vec<gtk::Widget> = ["p", "q", "r"]
            .iter()
            .map(|n| gtk::Label::new(Some(n)).upcast())
            .collect();
        let used = set_contents(&flow, &widgets, Wrappers::HOLDERS);
        used[2].emit_by_name::<()>("activate", &[]);
        set_contents(&flow, &widgets[..1], Wrappers::HOLDERS);
        assert!(wrappers_of(&flow).contains(&used[2]));
        for count in [1i32, -1, 1] {
            flow.emit_by_name::<bool>(
                "move-cursor",
                &[&gtk::MovementStep::VisualPositions, &count, &false, &false],
            );
        }
        flow.emit_by_name::<()>("activate-cursor-child", &[]);

        // A reused wrapper loses what a caller set on it for its last widget.
        let used = set_contents(&flow, &widgets, Wrappers::HOLDERS);
        used[0].add_css_class("checked");
        used[0].set_tooltip_text(Some("tip"));
        let used = set_contents(&flow, &widgets, Wrappers::HOLDERS);
        assert!(!used[0].has_css_class("checked"));
        assert_eq!(used[0].tooltip_text(), None);
    }
}
