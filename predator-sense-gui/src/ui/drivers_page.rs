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

    // A photo of a real label rather than the upstream line drawing.
    //
    // Scaled during decode, and never upscaled. `GtkPicture` reports its
    // image's full size as its natural size and a size request is a minimum
    // rather than a cap, so a photo taller than this made the widget that tall
    // and pushed the rest of the page out of view. Capping at decode time
    // fixes that; refusing to scale up keeps a smaller image pixel-exact
    // instead of softening it, which is what happened to the drawing this
    // replaced (its 443x84 viewBox was stretched to the request and went
    // blurry).
    //
    // `find-serial-number.svg` is still here if the drawing is ever wanted;
    // it needs width/height attributes to render sharp.
    if let Some(path) = crate::ui::window::find_resource("find-serial-number.png") {
        let caption = gtk::Label::new(Some(crate::i18n::t("find_serial_number_caption")));
        caption.add_css_class("info-note");
        caption.set_halign(gtk::Align::Start);
        caption.set_margin_top(10);
        page.append(&caption);

        const MAX_IMAGE_HEIGHT: i32 = 260;
        let natural_height = gtk4::gdk_pixbuf::Pixbuf::file_info(&path).map(|(_, _, h)| h);
        let picture = match natural_height {
            // Taller than we draw: scale it down while decoding, so the
            // texture is already the size the widget asks for.
            Some(height) if height > MAX_IMAGE_HEIGHT => {
                gtk4::gdk_pixbuf::Pixbuf::from_file_at_scale(&path, -1, MAX_IMAGE_HEIGHT, true)
                    .ok()
                    .map(|scaled| {
                        gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&scaled))
                    })
                    .unwrap_or_else(|| gtk::Picture::for_filename(&path))
            }
            // Already at or under the drawn height: leave it alone. Asking for
            // more would scale it up and soften it.
            _ => gtk::Picture::for_filename(&path),
        };
        picture.set_can_shrink(true);
        picture.set_halign(gtk::Align::Start);
        page.append(&picture);
    }

    scroll.set_child(Some(&page));
    scroll
}
