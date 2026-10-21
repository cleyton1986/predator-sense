//! The Lighting page's state notices: one row
//! per [`Notice`], built once and shown only while its state holds (`states`), under the
//! page's title. A row is its sentence, wrapping at any width (German, Portuguese and Russian
//! included), and for the one that can be fixed from here, a button.

use gtk4 as gtk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::ui::lighting::states::{Action, Notice};

struct Row {
    notice: Notice,
    widget: gtk::Box,
}

pub struct StateNotices {
    /// The rows, hidden whole while none shows, so the band keeps no gap for them.
    pub widget: gtk::Box,
    rows: Vec<Row>,
}

impl StateNotices {
    /// `on_action` runs with a row's [`Action`] when its button is pressed.
    pub fn new(on_action: Rc<dyn Fn(Action)>) -> StateNotices {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);
        widget.add_css_class("lighting-notices");
        widget.set_visible(false);
        let rows = Notice::ALL
            .into_iter()
            .map(|notice| {
                let role = if notice.is_problem() {
                    gtk::AccessibleRole::Alert
                } else {
                    gtk::AccessibleRole::Status
                };
                let row: gtk::Box = gtk4::glib::Object::builder()
                    .property("accessible-role", role)
                    .property("orientation", gtk::Orientation::Horizontal)
                    .property("spacing", 12)
                    .build();
                row.add_css_class("lighting-notice");
                if notice.is_problem() {
                    row.add_css_class("lighting-notice-problem");
                }
                let text = crate::i18n::t(notice.text_key());
                let label = gtk::Label::new(Some(text));
                label.add_css_class("lighting-notice-text");
                label.set_wrap(true);
                label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_valign(gtk::Align::Center);
                row.append(&label);
                row.update_property(&[gtk::accessible::Property::Label(text)]);
                if let Some((action, key)) = notice.action() {
                    let words = crate::i18n::t(key);
                    let button_label = gtk::Label::new(Some(words));
                    button_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    let button = gtk::Button::new();
                    button.set_child(Some(&button_label));
                    button.set_tooltip_text(Some(words));
                    button.add_css_class("lighting-small-button");
                    button.set_valign(gtk::Align::Center);
                    let on_action = on_action.clone();
                    button.connect_clicked(move |_| on_action(action));
                    row.append(&button);
                }
                row.set_visible(false);
                widget.append(&row);
                Row {
                    notice,
                    widget: row,
                }
            })
            .collect();
        StateNotices { widget, rows }
    }

    /// Shows the notices in `holding` and hides the rest.
    pub fn show(&self, holding: &[Notice]) {
        for row in &self.rows {
            row.widget.set_visible(holding.contains(&row.notice));
        }
        self.widget.set_visible(!holding.is_empty());
    }

    /// The notices shown now, in the page's order (for the tests).
    #[cfg(test)]
    pub fn shown(&self) -> Vec<Notice> {
        self.rows
            .iter()
            .filter(|row| row.widget.is_visible())
            .map(|row| row.notice)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A row shows only while its notice holds; the box is hidden with none, and a button
    /// says which action it takes. Off screen, on the GTK test thread.
    #[test]
    fn a_notice_shows_only_while_it_holds() {
        crate::ui::gtk_test::run("state_notices", notices_show_and_hide);
    }

    fn notices_show_and_hide() {
        let pressed: Rc<RefCell<Vec<Action>>> = Rc::default();
        let notices = {
            let pressed = pressed.clone();
            StateNotices::new(Rc::new(move |action| pressed.borrow_mut().push(action)))
        };
        assert!(!notices.widget.is_visible());
        notices.show(&[Notice::IdleDark, Notice::AuthCancelled]);
        assert!(notices.widget.is_visible());
        assert_eq!(notices.shown(), [Notice::AuthCancelled, Notice::IdleDark]);
        notices.show(&[]);
        assert!(!notices.widget.is_visible());
        assert!(notices.shown().is_empty());

        for row in &notices.rows {
            let mut child = row.widget.first_child();
            let mut buttons = Vec::new();
            while let Some(widget) = child {
                if let Some(button) = widget.downcast_ref::<gtk::Button>() {
                    buttons.push(button.clone());
                }
                child = widget.next_sibling();
            }
            assert_eq!(buttons.len(), usize::from(row.notice.action().is_some()));
            if let Some(button) = buttons.first() {
                button.emit_clicked();
            }
        }
        assert_eq!(*pressed.borrow(), [Action::TryAgain]);
    }
}
