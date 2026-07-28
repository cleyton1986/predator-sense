//! "Drivers e manuais" - points the user at Acer's official drivers/manuals
//! site, which asks for the machine's serial number to look up the right
//! downloads. Shows that serial (with a copy button, same as Settings) plus
//! an illustration of where to physically find it on the machine, since not
//! every user knows the sticker location off-hand.

use gtk4::prelude::*;
use gtk4::{self as gtk};

const ACER_DRIVERS_URL: &str = "https://www.acer.com/us-en/support/drivers-and-manuals";

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
    page.set_margin_top(10);
    page.set_margin_bottom(10);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(crate::i18n::t("drivers_and_manuals")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let intro = gtk::Label::new(Some(crate::i18n::t("drivers_and_manuals_intro")));
    intro.add_css_class("info-note");
    intro.set_halign(gtk::Align::Start);
    intro.set_wrap(true);
    page.append(&intro);

    if let Some(serial) = crate::hardware::extras::get_serial_number() {
        let serial_row = crate::ui::window::create_setting_row(crate::i18n::t("serial_number"), &serial);

        let copy_btn = gtk::Button::from_icon_name("edit-copy-symbolic");
        copy_btn.set_tooltip_text(Some(crate::i18n::t("copy")));
        copy_btn.add_css_class("flat");
        copy_btn.connect_clicked(move |_btn| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&serial);
            }
        });
        serial_row.append(&copy_btn);
        page.append(&serial_row);
    }

    let open_btn = gtk::Button::with_label(crate::i18n::t("open_acer_drivers_site"));
    open_btn.add_css_class("accent-button");
    open_btn.set_halign(gtk::Align::Start);
    open_btn.connect_clicked(|_| {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::gio::AppInfo::launch_default_for_uri(
                ACER_DRIVERS_URL,
                Some(&display.app_launch_context()),
            )
            .ok();
        }
    });
    page.append(&open_btn);

    if let Some(path) = crate::ui::window::find_resource("find-serial-number.svg") {
        let caption = gtk::Label::new(Some(crate::i18n::t("find_serial_number_caption")));
        caption.add_css_class("info-note");
        caption.set_halign(gtk::Align::Start);
        caption.set_margin_top(10);
        page.append(&caption);

        let picture = gtk::Picture::for_filename(path);
        picture.set_can_shrink(true);
        picture.set_halign(gtk::Align::Start);
        picture.set_size_request(-1, 260);
        page.append(&picture);
    }

    scroll.set_child(Some(&page));
    scroll
}
