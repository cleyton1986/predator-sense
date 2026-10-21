//! The Lighting page (the reference design): every power mode owns
//! a lighting setup, and the page edits one mode's at a time.
//!
//! Top to bottom: the title; "Lighting for"
//! (`lighting::mode_select`); the preset row (`lighting::presets_row`); the tab strip,
//! Lighting control and Themes, marking the one whose kind the edited mode uses; the
//! preview card shared by both tabs (`lighting::preview`), with the brightness of each
//! light right under the preview; the two tabs' panes (Lighting control:
//! `lighting::control`; Themes: `lighting::themes`); and the idle settings
//! (`lighting::idle`). From about 1320 px of width ("Wide windows") the
//! title, "Lighting for", the preset row and the tab strip stay across the top, over two
//! columns that scroll on their own: the preview and the idle settings on the left, the
//! tab's panes on the right (`lighting::columns`).
//!
//! Everything applies live with autosave (no Apply button). An edit changes the edited
//! mode's setup in config.json through `lighting_model`, on the UI thread; when that mode
//! is the one the lights show (the active mode, or the edited one while "Show on my laptop"
//! is on), `lighting::apply_mode` or `lighting::apply_preview` puts it on the lights on a
//! worker. A slider's edits (and a colour picker's drag, a paint stroke) are gathered and
//! written once it rests for [`APPLY_DEBOUNCE_MS`], and before anything else that reads the
//! setup ([`flush_pending_edits`]); the preview and the controls show them at once. Load,
//! Save, Save as new, Discard and Delete offer Undo in a toast, and so does picking a theme
//! over a Manual setup.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::BreakpointBinExt;
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::{Rc, Weak};

use crate::config::{self, AppConfig};
use crate::hardware::effects::themes::{Hardware, Theme};
use crate::hardware::lighting;
use crate::hardware::lighting_model::{self, LightingSetup, Lights, Undo};
use crate::hardware::profile::{self, PowerProfile};
use crate::ui::lighting::colour_model;
use crate::ui::lighting::columns::{self, Arrangement};
use crate::ui::lighting::control::ControlPane;
use crate::ui::lighting::control_model::{Facts, FourZoneFacts};
use crate::ui::lighting::frame_model::{self, ModeLayout, PageLayout, Tab, MODES};
use crate::ui::lighting::idle;
use crate::ui::lighting::mode_select::ModeSelect;
use crate::ui::lighting::presets_row::{self, LoadItem, PresetAction, PresetsRow};
use crate::ui::lighting::preview::{self, PreviewCard, PreviewLight};
use crate::ui::lighting::state_notices::StateNotices;
use crate::ui::lighting::states::{self, Action, Presence, Signals};
use crate::ui::lighting::themes::{Shown, ThemesPane};
use crate::ui::lighting::themes_model::Memory;

/// A slider's edits are written once it has rested this long.
pub(crate) const APPLY_DEBOUNCE_MS: u32 = 120;

/// How often the page looks at the lights' states while it is shown (`states`).
const STATE_CHECK_SECS: u32 = 1;

/// The narrowest the page goes (360 px of content) and the least height
/// the breakpoints need.
const MIN_WIDTH: i32 = 360;
const MIN_HEIGHT: i32 = 200;

thread_local! {
    /// The page, once built, so a power-mode change, the Fn keys and a resume reach it.
    static PAGE: RefCell<Option<Rc<Page>>> = const { RefCell::new(None) };
}

fn page() -> Option<Rc<Page>> {
    PAGE.with(|slot| slot.borrow().clone())
}

/// An edit of a mode's setup waiting to be written (a slider's), the latest per `key`. It
/// is applied to what the page shows until then, and once more when written.
struct PendingEdit {
    mode: PowerProfile,
    key: &'static str,
    /// The light whose brightness slider made it, which a refresh then leaves alone.
    light: Option<PreviewLight>,
    edit: Rc<dyn Fn(&mut LightingSetup)>,
}

/// The tab strip and the two panes under the preview.
struct Tabs {
    strip: gtk::Box,
    control: gtk::ToggleButton,
    themes: gtk::ToggleButton,
    control_note: gtk::Label,
    themes_note: gtk::Label,
    stack: gtk::Stack,
    /// The Lighting control tab's content (`lighting::control`).
    control_pane: gtk::Box,
    /// The Themes tab's content (`lighting::themes`).
    themes_pane: gtk::Box,
}

/// What the page is built for: the lights, the facts about them that do not change while the
/// app runs, and whether it talks to the system.
pub(crate) struct Machine {
    /// The lights as the setups are applied to them. Their kinds stay what they were when the
    /// page was built; the rest is asked again on every show while `live`.
    pub lights: Lights,
    pub facts: Facts,
    /// The idle settings' devices, the PH16-71's keyboard and the bar (`idle::build`). With
    /// neither, the idle settings are not shown: idle turns off only those.
    pub idle: (bool, bool),
    /// The page reads and writes the system: the active power mode, the lights' states and
    /// nodes, and the applies to the lights. False only for the render harness, whose page
    /// opens no device and writes nothing to the lights.
    pub live: bool,
    /// The active power mode while not `live`.
    pub active: Option<PowerProfile>,
}

impl Machine {
    /// This machine, from the system.
    fn system(lights: Lights) -> Machine {
        let caps = crate::hardware::capabilities::get();
        let four_zone =
            if lights.hw.keyboard == Some(crate::hardware::effects::themes::KeyboardKind::Zones4) {
                use crate::hardware::{audio_sync, hid_rgb, rgb};
                let hid = hid_rgb::is_available();
                let writable = hid || rgb::is_static_device_available();
                FourZoneFacts::found(
                    hid,
                    hid_rgb::keyboard_supports_effect,
                    hid_rgb::keyboard_effect_supports_direction,
                    writable && audio_sync::is_available(),
                )
            } else {
                FourZoneFacts::default()
            };
        Machine {
            lights,
            facts: Facts {
                bar_modes: crate::hardware::light_bar::LightBarMode::available(),
                bar_speed: crate::hardware::light_bar::speed_range(),
                zones: caps.light_bar_zones,
                four_zone,
                // From its capability report, as answered when the page was built.
                cover_logo_effects: lighting::cover_logo_effects(),
            },
            idle: (caps.keyboard_rgb, caps.light_bar),
            live: true,
            active: None,
        }
    }
}

pub(crate) struct Page {
    root: adw::BreakpointBin,
    /// One column or two (`lighting::columns`), set up once the page is assembled.
    arrangement: OnceCell<Arrangement>,
    hw: Hardware,
    /// `Machine::live`: the page talks to the system.
    live: bool,
    /// `Machine::lights`, what [`Page::lights`] gives while not `live`.
    base_lights: Lights,
    /// `Machine::active`.
    fixed_active: Option<PowerProfile>,
    /// Audio Sync is on (`audio_sync_enabled`), as config.json held it at the last show.
    audio_sync_on: Cell<bool>,
    edited: Cell<PowerProfile>,
    show_on_laptop: Cell<bool>,
    modes: Rc<ModeSelect>,
    presets: Rc<PresetsRow>,
    tabs: Tabs,
    preview: Rc<PreviewCard>,
    /// The Lighting control tab.
    control: Rc<ControlPane>,
    /// The Themes tab.
    themes: Rc<ThemesPane>,
    /// The theme each mode shows, as config.json held them at the last show (the gallery's
    /// "Also on").
    mode_themes: RefCell<Vec<(PowerProfile, Option<String>)>>,
    /// What the edited mode remembers for the themes it does not show, as config.json held
    /// it at the last show (the gallery's cards).
    theme_memory: RefCell<Memory>,
    /// The edited mode's setup and the kept colours, as config.json held them at the last
    /// show: what the waiting edits apply to ([`Page::current_setup`]).
    saved_setup: RefCell<LightingSetup>,
    saved_colours: RefCell<Vec<String>>,
    /// The lights as of that show, so a slider drag's many shows ask nothing of the system.
    shown_lights: Cell<Lights>,
    pending: RefCell<Vec<PendingEdit>>,
    /// Starts (or restarts) the wait before the pending edits are written.
    debounce: Rc<dyn Fn()>,
    watching_window: Cell<bool>,
    /// The state notices under the title (`states`).
    notices: StateNotices,
    /// Whether the keyboard's and the bar's nodes are there, settled over the checks, and
    /// what the last check settled.
    presence: Cell<(Presence, Presence)>,
    present: Cell<(bool, bool)>,
    /// `parec` was on `PATH` at the last check.
    parec_on_path: Cell<bool>,
    /// The edited setup runs a look that captures sound, as of the last show.
    music_look: Cell<bool>,
    /// The states' check while the page is shown.
    state_check: RefCell<Option<glib::SourceId>>,
    /// The preview's accessible name, which mode it shows ("Preview of Balanced: Calm."),
    /// as of the last show; its description follows every edit (`show_setup`).
    preview_label: RefCell<String>,
}

/// The Lighting page for every model: built from the lights the machine has,
/// or its empty state when it has none the app can drive.
pub fn build() -> gtk::Widget {
    // The ENE logo's capability report, asked here as the RGB page asked it when it was
    // built, if no apply has had an answer yet: `lights` never asks on this thread.
    lighting::ask_cover_logo_effects();
    let lights = lighting::lights();
    if !lights.hw.has_lights() {
        return empty_state();
    }
    let page = Page::new(Machine::system(lights));
    PAGE.with(|slot| *slot.borrow_mut() = Some(page.clone()));
    page.root.clone().upcast()
}

/// The page on a machine with no light the app can drive (its empty state): its
/// title and one line saying so.
pub(crate) fn empty_state() -> gtk::Widget {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let margin = frame_model::PAGE_MARGIN;
    column.set_margin_top(margin);
    column.set_margin_bottom(margin);
    column.set_margin_start(margin);
    column.set_margin_end(margin);
    let title = gtk::Label::new(Some(crate::i18n::t("lighting")));
    title.add_css_class("lighting-page-title");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let note = gtk::Label::new(Some(crate::i18n::t("lighting_no_devices")));
    note.add_css_class("lighting-row-sub");
    note.set_wrap(true);
    note.set_xalign(0.0);
    column.append(&title);
    column.append(&note);
    scroll.set_child(Some(&column));
    scroll.upcast()
}

/// A tab of the strip: its title and the "in use" mark.
fn tab_button(title: &str) -> (gtk::ToggleButton, gtk::Label) {
    let button: gtk::ToggleButton = glib::Object::builder()
        .property("accessible-role", gtk::AccessibleRole::Tab)
        .build();
    button.add_css_class("lighting-tab");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(Some(title));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let note = gtk::Label::new(Some(crate::i18n::t("lighting_in_use")));
    note.add_css_class("lighting-tab-note");
    note.set_valign(gtk::Align::Baseline);
    note.set_visible(false);
    content.append(&label);
    content.append(&note);
    button.set_child(Some(&content));
    (button, note)
}

/// A tab's pane, filled by its tab's module.
fn pane() -> gtk::Box {
    glib::Object::builder()
        .property("accessible-role", gtk::AccessibleRole::TabPanel)
        .property("orientation", gtk::Orientation::Vertical)
        .build()
}

impl Tabs {
    fn new() -> Tabs {
        let strip: gtk::Box = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::TabList)
            .property("orientation", gtk::Orientation::Horizontal)
            .property("spacing", 24)
            .build();
        strip.add_css_class("lighting-tabs");
        strip.update_property(&[gtk::accessible::Property::Label(crate::i18n::t("lighting"))]);
        let (control, control_note) = tab_button(crate::i18n::t("lighting_tab_control"));
        let (themes, themes_note) = tab_button(crate::i18n::t("lighting_tab_themes"));
        themes.set_group(Some(&control));
        strip.append(&control);
        strip.append(&themes);
        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        stack.set_hhomogeneous(true);
        let control_pane = pane();
        let themes_pane = pane();
        stack.add_named(&control_pane, Some("control"));
        stack.add_named(&themes_pane, Some("themes"));
        for (tab, pane) in [(&control, &control_pane), (&themes, &themes_pane)] {
            tab.update_relation(&[gtk::accessible::Relation::Controls(&[pane.upcast_ref()])]);
            pane.update_relation(&[gtk::accessible::Relation::LabelledBy(&[tab.upcast_ref()])]);
        }
        Tabs {
            strip,
            control,
            themes,
            control_note,
            themes_note,
            stack,
            control_pane,
            themes_pane,
        }
    }

    fn set_in_use(&self, tab: Option<Tab>) {
        self.control_note.set_visible(tab == Some(Tab::Control));
        self.themes_note.set_visible(tab == Some(Tab::Themes));
        for (button, title, this) in [
            (&self.control, "lighting_tab_control", Tab::Control),
            (&self.themes, "lighting_tab_themes", Tab::Themes),
        ] {
            let mut label = crate::i18n::t(title).to_string();
            if tab == Some(this) {
                label = format!("{label}, {}", crate::i18n::t("lighting_in_use"));
            }
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
        }
    }

    fn show(&self, tab: Tab) {
        let (button, name) = match tab {
            Tab::Control => (&self.control, "control"),
            Tab::Themes => (&self.themes, "themes"),
        };
        if !button.is_active() {
            button.set_active(true);
        }
        self.stack.set_visible_child_name(name);
        for (button, this) in [(&self.control, Tab::Control), (&self.themes, Tab::Themes)] {
            button.update_state(&[gtk::accessible::State::Selected(Some(this == tab))]);
        }
    }
}

impl Page {
    fn new(machine: Machine) -> Rc<Page> {
        let Machine {
            lights,
            facts,
            idle,
            live,
            active: fixed_active,
        } = machine;
        let hw = lights.hw;
        let active = if live {
            profile::get_current_profile()
        } else {
            fixed_active
        };
        // The page opens on the active mode.
        let edited = active.unwrap_or(PowerProfile::Balanced);
        let font_scale = config::load_app_config().font_scale;

        let page = Rc::new_cyclic(|weak: &Weak<Page>| {
            let modes = {
                let weak = weak.clone();
                ModeSelect::new(Rc::new(move |mode| {
                    if let Some(page) = weak.upgrade() {
                        page.select_mode(mode);
                    }
                }))
            };
            let presets = PresetsRow::new(presets_row::Hooks {
                act: {
                    let weak = weak.clone();
                    Rc::new(move |action| {
                        if let Some(page) = weak.upgrade() {
                            page.preset_action(action);
                        }
                    })
                },
                check_name: Rc::new(|name, renaming| {
                    let (cfg, _) = config::load_lighting_config();
                    frame_model::name_problem(&cfg, name, renaming)
                        .map(|problem| frame_model::name_problem_text(&problem))
                }),
                suggest: {
                    let weak = weak.clone();
                    Rc::new(move || {
                        let (cfg, _) = config::load_lighting_config();
                        let mode = weak
                            .upgrade()
                            .map(|page| page.edited.get())
                            .unwrap_or(PowerProfile::Balanced);
                        frame_model::suggested_name(&cfg, &frame_model::name_base(&cfg, mode))
                    })
                },
            });
            let preview = PreviewCard::new(
                &lights,
                preview::Callbacks {
                    show_on_laptop: {
                        let weak = weak.clone();
                        Rc::new(move |on| {
                            if let Some(page) = weak.upgrade() {
                                page.set_show_on_laptop(on);
                            }
                        })
                    },
                    brightness: {
                        let weak = weak.clone();
                        Rc::new(move |light, value| {
                            if let Some(page) = weak.upgrade() {
                                page.edit_later(brightness_key(light), Some(light), move |setup| {
                                    light.set_brightness(&mut setup.brightness, value)
                                });
                            }
                        })
                    },
                    bar_follows: {
                        let weak = weak.clone();
                        Rc::new(move |on| {
                            if let Some(page) = weak.upgrade() {
                                // A switch offers no Undo.
                                let _ = page
                                    .edit(|setup| setup.brightness.set_bar_follows_keyboard(on));
                            }
                        })
                    },
                },
                font_scale,
            );
            let control = ControlPane::new(weak.clone(), preview.clone(), facts);
            let themes = ThemesPane::new(weak.clone(), preview.clone(), lights);
            Page {
                root: adw::BreakpointBin::new(),
                arrangement: OnceCell::new(),
                hw,
                live,
                base_lights: lights,
                fixed_active,
                audio_sync_on: Cell::new(false),
                edited: Cell::new(edited),
                show_on_laptop: Cell::new(false),
                modes,
                presets,
                tabs: Tabs::new(),
                preview,
                control,
                themes,
                mode_themes: RefCell::new(Vec::new()),
                theme_memory: RefCell::new(Memory::new()),
                saved_setup: RefCell::new(LightingSetup::default()),
                saved_colours: RefCell::new(Vec::new()),
                shown_lights: Cell::new(Lights::from(hw)),
                pending: RefCell::new(Vec::new()),
                debounce: {
                    let weak = weak.clone();
                    let (trigger, _) = crate::ui::color_picker::debouncer(
                        APPLY_DEBOUNCE_MS,
                        Rc::new(move || {
                            if let Some(page) = weak.upgrade() {
                                page.flush();
                            }
                        }),
                    );
                    trigger
                },
                watching_window: Cell::new(false),
                notices: StateNotices::new({
                    let weak = weak.clone();
                    Rc::new(move |action| {
                        if let Some(page) = weak.upgrade() {
                            page.notice_action(action);
                        }
                    })
                }),
                presence: Cell::new((Presence::default(), Presence::default())),
                present: Cell::new((true, true)),
                parec_on_path: Cell::new(true),
                music_look: Cell::new(false),
                state_check: RefCell::new(None),
                preview_label: RefCell::new(String::new()),
            }
        });
        page.assemble(font_scale, idle);
        page
    }

    /// The active power mode: the system's, or the render harness's.
    fn active(&self) -> Option<PowerProfile> {
        if self.live {
            profile::get_current_profile()
        } else {
            self.fixed_active
        }
    }

    fn assemble(self: &Rc<Self>, font_scale: f64, (idle_keyboard, idle_bar): (bool, bool)) {
        // The band: what stays across the top on a wide page.
        let band = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let title = gtk::Label::new(Some(crate::i18n::t("lighting")));
        title.add_css_class("lighting-page-title");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        band.append(&title);
        self.notices.widget.set_margin_top(12);
        band.append(&self.notices.widget);
        band.append(&self.modes.widget);
        self.presets.widget.set_margin_top(10);
        band.append(&self.presets.widget);
        band.append(&self.tabs.strip);

        self.tabs.control_pane.append(&self.control.widget);
        self.tabs.themes_pane.append(&self.themes.widget);
        // Each tab's bar under the preview; the slot shows only the one in use
        // (`PreviewCard::show_context`).
        self.preview.context_slot.append(&self.control.brush_bar);
        self.preview.context_slot.append(&self.themes.pick_bar);
        let idle_block = idle::build(idle_keyboard, idle_bar);
        idle_block.set_margin_top(26);
        // Idle turns off only the PH16-71's keyboard and the bar.
        idle_block.set_visible(idle_keyboard || idle_bar);

        let arrangement =
            Arrangement::new(&band, &self.preview.widget, &self.tabs.stack, &idle_block);
        self.root.set_child(Some(&arrangement.frame));
        let _ = self.arrangement.set(arrangement);
        // AdwBreakpointBin needs its least size set; the content scrolls under it.
        self.root.set_size_request(MIN_WIDTH, MIN_HEIGHT);

        // Narrow and tiling widths: the modes three over two, then a
        // dropdown. The app's sidebar stays.
        let (three_two, dropdown) = frame_model::breakpoint_widths(font_scale);
        for (width, layout) in [
            (three_two, ModeLayout::ThreeTwo),
            (dropdown, ModeLayout::Dropdown),
        ] {
            let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
                adw::BreakpointConditionLengthType::MaxWidth,
                width,
                adw::LengthUnit::Px,
            ));
            let weak = Rc::downgrade(self);
            breakpoint.connect_apply(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.set_layout(layout);
                }
            });
            let weak = Rc::downgrade(self);
            breakpoint.connect_unapply(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.set_layout(ModeLayout::Row);
                }
            });
            self.root.add_breakpoint(breakpoint);
        }
        // Wide pages: the band over two columns.
        {
            let weak = Rc::downgrade(self);
            self.root
                .add_breakpoint(columns::breakpoint(font_scale, move |layout| {
                    if let Some(page) = weak.upgrade() {
                        page.set_page_layout(layout);
                    }
                }));
        }

        for (button, tab) in [
            (&self.tabs.control, Tab::Control),
            (&self.tabs.themes, Tab::Themes),
        ] {
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |button| {
                if !button.is_active() {
                    return;
                }
                if let Some(page) = weak.upgrade() {
                    if tab != Tab::Control {
                        page.control.leave();
                    }
                    if tab != Tab::Themes {
                        page.themes.leave();
                    }
                    page.tabs.show(tab);
                    page.preview.set_tab(tab);
                }
            });
        }

        // A tab list: Left, Right, Home and End move to the other tab and select it (the
        // reference design's `radioArrows` on its tabs).
        {
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(page) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let tab = match key {
                    gtk::gdk::Key::Left | gtk::gdk::Key::Home => Tab::Control,
                    gtk::gdk::Key::Right | gtk::gdk::Key::End => Tab::Themes,
                    _ => return glib::Propagation::Proceed,
                };
                let button = match tab {
                    Tab::Control => &page.tabs.control,
                    Tab::Themes => &page.tabs.themes,
                };
                button.set_active(true);
                button.grab_focus();
                glib::Propagation::Stop
            });
            self.tabs.strip.add_controller(keys);
        }

        // "Show on my laptop" ends when the page is left or the window hides (both unmap
        // it) and when the window is suspended (minimised, another workspace). Each time the
        // page is shown again it edits the active mode; the tab stays.
        {
            let weak = Rc::downgrade(self);
            self.root.connect_unmap(move |_| {
                if let Some(page) = weak.upgrade() {
                    page.control.leave();
                    page.themes.leave();
                    page.end_show_on_laptop_soon();
                    page.check_states(false);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.root.connect_map(move |root| {
                if let Some(page) = weak.upgrade() {
                    page.watch_window(root);
                    page.return_to_active();
                    page.refresh();
                    page.check_states(true);
                }
            });
        }

        let (cfg, _) = config::load_lighting_config();
        let opening = cfg
            .lighting_modes
            .get(&self.edited.get())
            .and_then(|l| frame_model::tab_in_use(&l.setup))
            .unwrap_or(Tab::Control);
        self.tabs.show(opening);
        self.preview.set_tab(opening);
        self.show(&cfg);
    }

    fn watch_window(self: &Rc<Self>, root: &adw::BreakpointBin) {
        if self.watching_window.get() {
            return;
        }
        let Some(window) = root.root().and_downcast::<gtk::Window>() else {
            return;
        };
        if window.find_property("suspended").is_none() {
            return;
        }
        self.watching_window.set(true);
        let weak = Rc::downgrade(self);
        window.connect_notify_local(Some("suspended"), move |window, _| {
            if window.property::<bool>("suspended") {
                if let Some(page) = weak.upgrade() {
                    page.end_show_on_laptop_soon();
                }
            }
        });
    }

    /// The page edits the active mode again (each time it is shown): waiting edits are
    /// written, the naming field closes, "Show on my laptop" ends.
    fn return_to_active(&self) {
        let Some(active) = self.active() else {
            return;
        };
        if self.edited.get() == active {
            return;
        }
        self.flush();
        self.presets.close_form();
        self.control.leave();
        self.themes.leave();
        self.end_show_on_laptop(true);
        self.edited.set(active);
    }

    /// Ends "Show on my laptop" from the next turn of the main loop, so that a page hidden by
    /// the window closing leaves it to the exit ([`at_exit`]): the loop ends before that
    /// turn, and the exit puts the active mode back itself, in order. Once the exit has
    /// begun, it ends without an apply.
    fn end_show_on_laptop_soon(self: &Rc<Self>) {
        if !self.show_on_laptop.get() {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            if let Some(page) = weak.upgrade() {
                page.end_show_on_laptop(!exiting());
            }
        });
    }

    /// The narrow layouts (the 680 and 440 px breakpoints), with the keyboard focus kept
    /// where it was ([`keeping_focus`]).
    fn set_layout(&self, layout: ModeLayout) {
        keeping_focus(self.root.upcast_ref(), &self.modes, || {
            self.modes.set_layout(layout);
            let narrow = layout != ModeLayout::Row;
            self.presets.set_narrow(narrow);
            self.preview.set_narrow(narrow);
        });
    }

    /// One column or two: the tabs line up their tiles and cards for it, then the page moves
    /// the band and the panes (`lighting::columns`).
    fn set_page_layout(&self, layout: PageLayout) {
        self.control.set_layout(layout);
        self.themes.set_layout(layout);
        if let Some(arrangement) = self.arrangement.get() {
            arrangement.set(layout);
        }
    }

    /// The lights as the setups are applied to them now (drawable or not), of the kinds the
    /// page was built for.
    pub(crate) fn lights(&self) -> Lights {
        if !self.live {
            return self.base_lights;
        }
        Lights {
            hw: self.hw,
            ..lighting::lights()
        }
    }

    /// Shows config.json as it is now.
    fn refresh(&self) {
        let (cfg, _) = config::load_lighting_config();
        self.show(&cfg);
    }

    fn show(&self, cfg: &AppConfig) {
        let active = self.active();
        self.audio_sync_on.set(cfg.audio_sync_enabled);
        let edited = self.edited.get();
        let captions: Vec<(PowerProfile, String)> = MODES
            .into_iter()
            .map(|mode| {
                let lighting = cfg.lighting_modes.get(&mode).cloned().unwrap_or_default();
                (
                    mode,
                    frame_model::caption(&frame_model::shows(&lighting, &self.hw)),
                )
            })
            .collect();
        self.modes.update(&captions, active, edited);

        let view = frame_model::preset_row(cfg, edited);
        let items: Vec<LoadItem> = cfg
            .lighting_presets
            .iter()
            .map(|preset| {
                let holders = in_page_order(lighting_model::modes_holding(cfg, &preset.name));
                LoadItem {
                    name: preset.name.clone(),
                    held_by: if holders.is_empty() {
                        crate::i18n::t("lighting_unused").to_string()
                    } else {
                        crate::i18n::tf("lighting_on_modes", &[&frame_model::mode_list(&holders)])
                    },
                    current: view.held.as_deref() == Some(preset.name.as_str()),
                }
            })
            .collect();
        self.presets.update(&view, &items);

        *self.saved_setup.borrow_mut() = cfg
            .lighting_modes
            .get(&edited)
            .map(|l| l.setup.clone())
            .unwrap_or_default();
        *self.saved_colours.borrow_mut() = cfg.saved_colors.clone();
        *self.mode_themes.borrow_mut() = MODES
            .into_iter()
            .map(|mode| {
                let id = cfg
                    .lighting_modes
                    .get(&mode)
                    .and_then(|l| match &l.setup.kind {
                        lighting_model::SetupKind::Theme(choice) => Some(choice.id.clone()),
                        _ => None,
                    });
                (mode, id)
            })
            .collect();
        *self.theme_memory.borrow_mut() = cfg
            .lighting_modes
            .get(&edited)
            .map(|l| l.theme_memory.clone())
            .unwrap_or_default();
        self.shown_lights.set(self.lights());
        let caption = captions
            .iter()
            .find(|(m, _)| *m == edited)
            .map(|(_, c)| c.as_str())
            .unwrap_or("");
        *self.preview_label.borrow_mut() =
            crate::i18n::tf("lighting_preview_label", &[edited.label(), caption]);
        self.show_setup();
        self.preview
            .set_notice(edited, active, self.show_on_laptop.get());
    }

    /// The edited mode's setup as the page shows it: as saved, with the edits still waiting
    /// applied.
    pub(crate) fn current_setup(&self) -> LightingSetup {
        let edited = self.edited.get();
        let mut setup = self.saved_setup.borrow().clone();
        for pending in self.pending.borrow().iter().filter(|p| p.mode == edited) {
            (pending.edit)(&mut setup);
        }
        setup
    }

    /// The mode the page edits.
    pub(crate) fn edited(&self) -> PowerProfile {
        self.edited.get()
    }

    /// The kept colours, as the page shows them.
    pub(crate) fn saved_colours(&self) -> Vec<String> {
        self.saved_colours.borrow().clone()
    }

    /// Shows the edited mode's setup, edits still waiting included, in the tab strip's mark,
    /// the preview and both tabs.
    pub(crate) fn show_setup(&self) {
        let edited = self.edited.get();
        let setup = self.current_setup();
        self.tabs.set_in_use(frame_model::tab_in_use(&setup));
        let keep: Vec<PreviewLight> = self
            .pending
            .borrow()
            .iter()
            .filter(|p| p.mode == edited)
            .filter_map(|p| p.light)
            .collect();
        let lights = self.shown_lights.get();
        self.music_look
            .set(lighting_model::plan(&setup, &lights).needs_sound());
        self.preview.set_setup(&setup, &lights, &keep);
        self.preview.describe(
            &self.preview_label.borrow(),
            &frame_model::describe_setup(&setup, &lights),
        );
        let saved = self.saved_colours.borrow().clone();
        self.control
            .show(&setup, lights, &saved, self.audio_sync_on.get());
        let modes = self.mode_themes.borrow().clone();
        let memory = self.theme_memory.borrow().clone();
        self.themes.show(&Shown {
            setup: &setup,
            lights,
            saved: &saved,
            edited,
            modes: &modes,
            memory: &memory,
            // The key gate's own state, not the file's (see `key_input::react_to_typing`).
            react_to_typing: crate::hardware::key_input::react_to_typing(),
        });
        self.update_states();
    }

    /// Starts or stops looking at the lights' states every [`STATE_CHECK_SECS`] (while the
    /// page is shown), and looks once now when starting.
    fn check_states(self: &Rc<Self>, on: bool) {
        if let Some(id) = self.state_check.borrow_mut().take() {
            id.remove();
        }
        if !on || !self.live {
            return;
        }
        self.check_devices();
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_seconds_local(STATE_CHECK_SECS, move || match weak.upgrade() {
            Some(page) => {
                page.check_devices();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        *self.state_check.borrow_mut() = Some(id);
    }

    /// The page has the PH16-71's 24-bit keyboard and its zone bar (the lights `states`
    /// watches).
    fn watched_lights(&self) -> (bool, bool) {
        let caps = crate::hardware::capabilities::get();
        let keyboard = caps.keyboard_rgb
            && self.hw.keyboard == Some(crate::hardware::effects::themes::KeyboardKind::PerKey);
        (keyboard, caps.light_bar_zones && self.hw.bar.is_some())
    }

    /// One state check: whether the lights' nodes are there (settled over the checks,
    /// `Presence`) and `parec` is on `PATH`, then the notices. Only on the check's own beat,
    /// since a node's presence counts checks, and not on every edit. A keyboard whose node is
    /// found again gets its look written again ([`Page::rewrite_keyboard`]).
    fn check_devices(&self) {
        let (keyboard, bar) = self.watched_lights();
        let (mut keyboard_node, mut bar_node) = self.presence.get();
        let keyboard_present =
            keyboard && keyboard_node.seen(crate::hardware::keyboard_rgb::hidraw_node_present());
        let bar_present = bar
            && bar_node
                .seen(std::path::Path::new(crate::hardware::light_bar_zones::DEVICE).exists());
        self.presence.set((keyboard_node, bar_node));
        self.present.set((keyboard_present, bar_present));
        if keyboard && keyboard_node.came_back() {
            // Enumerated again, the keyboard shows its power-on state, and the record still
            // says what it showed before: its look is written again whatever the record says.
            self.rewrite_keyboard();
        }
        if self.music_look.get() {
            self.parec_on_path.set(
                std::env::var_os("PATH")
                    .is_some_and(|path| states::on_path(&path, "parec", states::is_executable)),
            );
        }
        self.update_states();
    }

    /// Reads the lights' states (see `states` for each signal), with the nodes as the last
    /// check found them, and shows their notices.
    fn update_states(&self) {
        if !self.live {
            self.notices.show(&[]);
            return;
        }
        let (keyboard, bar) = self.watched_lights();
        let (keyboard_present, bar_present) = self.present.get();
        let status = crate::hardware::lighting_renderer::renderer().status();
        let music_look = self.music_look.get();
        let parec_missing = music_look
            && (status.audio == crate::hardware::lighting_renderer::AudioState::Missing
                || !self.parec_on_path.get());
        let signals = Signals {
            keyboard,
            keyboard_present,
            keyboard_writable: keyboard && crate::hardware::keyboard_rgb::is_hidraw_writable(),
            keyboard_denied: keyboard && crate::hardware::keyboard_rgb::hidraw_denied(),
            bar,
            bar_present,
            bar_denied: bar && bar_present && !crate::hardware::light_bar_zones::writable(),
            auth_dismissed: crate::hardware::helper::authentication_dismissed(),
            keyboard_blanked: keyboard && crate::hardware::keyboard_rgb::is_blanked(),
            bar_blanked: self.hw.bar.is_some() && crate::hardware::light_bar::is_blanked(),
            paused_on_battery: status.paused_on_battery,
            music_look,
            parec_missing,
        };
        self.notices.show(&states::notices(&signals));
    }

    /// A notice's button.
    fn notice_action(self: &Rc<Self>, action: Action) {
        match action {
            // The keyboard is written whatever the record says, so the request is made
            // again even when the cancelled write was the start-up restore's.
            Action::TryAgain => self.rewrite_keyboard(),
        }
    }

    /// The "React to typing" switch: through its own setter only, the one
    /// path that changes the key gate. The switch then shows the gate's state, which a failed
    /// save leaves off for this session (the setter never fails open). The looks that can
    /// use key presses change with it (`Lights::looks`), so the lights get them again.
    pub(crate) fn set_react_to_typing(&self, on: bool) {
        if let Err(e) = config::set_react_to_typing(on) {
            crate::hardware::applog::error(&format!(
                "lighting page: React to typing not saved: {e}"
            ));
        }
        self.refresh();
        self.apply_lights();
    }

    /// The four-zone keyboard's "Audio Sync" switch: one setting for the app, as on its old
    /// page. The lights then get the setup again, which starts or stops the capture
    /// (`lighting::apply_mode`); nothing else changes on them.
    pub(crate) fn set_audio_sync(&self, on: bool) {
        self.change(|cfg| cfg.audio_sync_enabled = on);
        self.apply_lights();
    }

    /// "Keep this colour": it joins the kept colours, shared by every light and mode.
    pub(crate) fn keep_colour(&self, rgb: [u8; 3]) {
        self.change(|cfg| colour_model::keep(&mut cfg.saved_colors, rgb));
    }

    /// A kept colour's x: forgotten at once, with Undo in a toast.
    pub(crate) fn forget_colour(self: &Rc<Self>, rgb: [u8; 3]) {
        let forgotten = self
            .change(|cfg| colour_model::forget(&mut cfg.saved_colors, rgb))
            .flatten();
        let Some(forgotten) = forgotten else {
            return;
        };
        let [r, g, b] = rgb;
        let text = crate::i18n::tf("colour_forgotten", &[&format!("#{r:02X}{g:02X}{b:02X}")]);
        let weak = Rc::downgrade(self);
        crate::ui::toasts::show_undo(&text, move || {
            if let Some(page) = weak.upgrade() {
                page.change(|cfg| colour_model::restore(&mut cfg.saved_colors, &forgotten));
            }
        });
    }

    /// The user picked another mode to edit.
    fn select_mode(&self, mode: PowerProfile) {
        if mode == self.edited.get() {
            return;
        }
        self.flush();
        self.presets.close_form();
        self.control.leave();
        self.themes.leave();
        self.end_show_on_laptop(true);
        self.edited.set(mode);
        self.refresh();
    }

    fn set_show_on_laptop(&self, on: bool) {
        if self.show_on_laptop.get() == on {
            return;
        }
        self.flush();
        self.show_on_laptop.set(on);
        self.apply_lights();
    }

    /// Ends "Show on my laptop" if it is on; `apply` puts the active mode's lighting back
    /// (not when a mode change or a restore is about to).
    fn end_show_on_laptop(&self, apply: bool) {
        if !self.show_on_laptop.replace(false) {
            return;
        }
        self.preview
            .set_notice(self.edited.get(), self.active(), false);
        if apply {
            self.apply_lights();
        }
    }

    /// Puts the setup of the mode the lights show on them, on a worker (see the module doc).
    fn apply_lights(&self) {
        self.apply_lights_as(false);
    }

    /// [`Page::apply_lights`], with the keyboard written whatever the record says it shows
    /// (`lighting::apply_mode_rewriting_keyboard`): Try again after a cancelled password
    /// request, and a keyboard that came back after it was gone.
    fn rewrite_keyboard(&self) {
        self.apply_lights_as(true);
    }

    fn apply_lights_as(&self, rewrite_keyboard: bool) {
        if exiting() || !self.live {
            // No worker may start writing the lights once the exit has begun, nor ever for
            // the render harness's page.
            return;
        }
        let active = self.active();
        let Some(mode) =
            frame_model::lights_mode(active, self.edited.get(), self.show_on_laptop.get())
        else {
            return;
        };
        let preview = Some(mode) != active;
        let change = lighting::mode_changed(mode);
        crate::ui::background::run(
            move || {
                if rewrite_keyboard {
                    lighting::apply_mode_rewriting_keyboard(change, preview, "lighting page")
                } else if preview {
                    lighting::apply_preview(change, "lighting page")
                } else {
                    lighting::apply_mode(change, "lighting page")
                }
            },
            |_| {
                lighting::record_written();
                if let Some(page) = page() {
                    page.refresh();
                }
            },
        );
    }

    /// Changes config.json with `change`, on the UI thread: what the applies wrote and the
    /// slider edits waiting are written first, the file is saved (never over one that could
    /// not be read), the lights get the new setup when the mode they show changed, and the
    /// page shows the result. `None` when nothing could be saved.
    fn change<R>(&self, change: impl FnOnce(&mut AppConfig) -> R) -> Option<R> {
        self.flush();
        lighting::record_written();
        let (mut cfg, may_save) = config::load_lighting_config();
        if !may_save {
            crate::hardware::applog::error(
                "lighting page: config.json could not be read, so the change was not saved",
            );
            self.show(&cfg);
            return None;
        }
        let lights_mode =
            frame_model::lights_mode(self.active(), self.edited.get(), self.show_on_laptop.get());
        let setup_of = |cfg: &AppConfig| {
            lights_mode.and_then(|mode| cfg.lighting_modes.get(&mode).map(|l| l.setup.clone()))
        };
        let before = setup_of(&cfg);
        let result = change(&mut cfg);
        if let Err(e) = config::save_app_config(&cfg) {
            crate::hardware::applog::error(&format!("lighting page: not saved: {e}"));
            self.refresh();
            return None;
        }
        let after = setup_of(&cfg);
        let differs = match (&before, &after) {
            (Some(before), Some(after)) => !before.shows_same_as(after),
            (None, None) => false,
            _ => true,
        };
        if differs {
            self.apply_lights();
        }
        self.show(&cfg);
        Some(result)
    }

    /// Changes the edited mode's setup now (the Lighting control and Themes tabs' controls).
    /// Returns the Undo for a toast.
    pub(crate) fn edit(&self, edit: impl FnOnce(&mut LightingSetup)) -> Option<Undo> {
        let mode = self.edited.get();
        self.change(|cfg| lighting_model::edit_setup(cfg, mode, edit))
    }

    /// "Save as preset…" in the Themes tab: the preset row's own Save as new, brought into
    /// view (the reference design's).
    pub(crate) fn save_as_preset(&self) {
        self.flush();
        self.presets.save_as_new();
        bring_into_view(&self.presets.widget);
    }

    /// Shows `theme` on the edited mode now (a gallery card), with what the mode last had for
    /// it. Returns the Undo for a toast.
    pub(crate) fn choose_theme(&self, theme: &Theme) -> Option<Undo> {
        let mode = self.edited.get();
        self.change(|cfg| lighting_model::choose_theme(cfg, mode, theme))
    }

    /// Changes the edited mode's setup soon: a slider's edit, the latest per `key`, written
    /// once the slider has rested [`APPLY_DEBOUNCE_MS`] (the reference design writes the lights
    /// when a
    /// slider is let go), or sooner by [`Page::flush`]. The preview and the controls show it
    /// at once.
    pub(crate) fn edit_later(
        &self,
        key: &'static str,
        light: Option<PreviewLight>,
        edit: impl Fn(&mut LightingSetup) + 'static,
    ) {
        let mode = self.edited.get();
        {
            let mut pending = self.pending.borrow_mut();
            pending.retain(|p| !(p.mode == mode && p.key == key));
            pending.push(PendingEdit {
                mode,
                key,
                light,
                edit: Rc::new(edit),
            });
        }
        (self.debounce)();
        self.show_setup();
    }

    /// Writes the slider edits still waiting. The wait's own timer then finds nothing.
    fn flush(&self) {
        let edits = std::mem::take(&mut *self.pending.borrow_mut());
        if edits.is_empty() {
            return;
        }
        self.change(move |cfg| {
            for PendingEdit { mode, edit, .. } in edits {
                // A slider edit offers no Undo.
                let _ = lighting_model::edit_setup(cfg, mode, |setup| edit(setup));
            }
        });
    }

    /// An Fn backlight key changes the keyboard level of the mode the lights show: a
    /// brightness drag of that mode's keyboard still waiting to be written is older than the
    /// key's level, and is dropped rather than written over it. So is a drag of its bar
    /// while the two are synced, since that drag sets the keyboard too. Slider edits are only
    /// ever waiting for the edited mode, whose setup the preview shows.
    fn drop_waiting_keyboard_drag(&self) {
        let shown =
            frame_model::lights_mode(self.active(), self.edited.get(), self.show_on_laptop.get());
        let synced = self.preview.brightness_synced();
        self.pending.borrow_mut().retain(|p| {
            let keyboard = p.key == brightness_key(PreviewLight::Keyboard)
                || (synced && p.key == brightness_key(PreviewLight::Bar));
            !(Some(p.mode) == shown && keyboard)
        });
    }

    /// The Fn keys saved a keyboard level: a drag that started since the key is dropped too,
    /// and the page shows the saved level.
    fn fn_level_saved(&self) {
        self.drop_waiting_keyboard_drag();
        self.refresh();
    }

    /// Offers `token` as Undo in a toast saying `text`.
    pub(crate) fn offer_undo(self: &Rc<Self>, text: &str, token: Undo) {
        let weak = Rc::downgrade(self);
        let token = RefCell::new(Some(token));
        crate::ui::toasts::show_undo(text, move || {
            let (Some(page), Some(token)) = (weak.upgrade(), token.borrow_mut().take()) else {
                return;
            };
            page.change(|cfg| lighting_model::undo(cfg, token));
        });
    }

    fn preset_action(self: &Rc<Self>, action: PresetAction) {
        let mode = self.edited.get();
        let failed = |what: &str, e: lighting_model::PresetError| {
            crate::hardware::applog::error(&format!("lighting page: {what}: {e:?}"));
        };
        match action {
            PresetAction::Load(name) => {
                match self.change(|cfg| lighting_model::load(cfg, mode, &name)) {
                    Some(Ok(token)) => {
                        self.offer_undo(&frame_model::toast_loaded(&name, mode), token)
                    }
                    Some(Err(e)) => failed("load", e),
                    None => {}
                }
            }
            PresetAction::Save => {
                let saved = self.change(|cfg| {
                    let before = cfg.clone();
                    let name = cfg.lighting_modes.get(&mode).and_then(|l| l.preset.clone());
                    lighting_model::save(cfg, mode).map(|token| {
                        let also = frame_model::modes_changed(&before, cfg, mode);
                        (token, name.unwrap_or_default(), also)
                    })
                });
                match saved {
                    Some(Ok((token, name, also))) => {
                        self.offer_undo(&frame_model::toast_saved(&name, &also), token)
                    }
                    Some(Err(e)) => failed("save", e),
                    None => {}
                }
            }
            PresetAction::SaveAsNew(requested) => {
                match self.change(|cfg| lighting_model::save_as_new(cfg, mode, &requested)) {
                    Some(Ok((name, token))) => {
                        self.offer_undo(&frame_model::toast_saved_as(&name, mode), token)
                    }
                    Some(Err(e)) => failed("save as new", e),
                    None => {}
                }
            }
            PresetAction::Discard => {
                let discarded = self.change(|cfg| {
                    let name = cfg.lighting_modes.get(&mode).and_then(|l| l.preset.clone());
                    lighting_model::discard(cfg, mode)
                        .map(|token| (token, name.unwrap_or_default()))
                });
                match discarded {
                    Some(Ok((token, name))) => {
                        self.offer_undo(&frame_model::toast_discarded(&name), token)
                    }
                    Some(Err(e)) => failed("discard", e),
                    None => {}
                }
            }
            PresetAction::Rename { from, to } => {
                if let Some(Err(e)) = self.change(|cfg| lighting_model::rename(cfg, &from, &to)) {
                    failed("rename", e);
                }
            }
            PresetAction::Delete(name) => {
                let deleted = self.change(|cfg| {
                    let holders = in_page_order(lighting_model::modes_holding(cfg, &name));
                    lighting_model::delete(cfg, &name).map(|token| (token, holders))
                });
                match deleted {
                    Some(Ok((token, holders))) => {
                        self.offer_undo(&frame_model::toast_deleted(&name, &holders), token)
                    }
                    Some(Err(e)) => failed("delete", e),
                    None => {}
                }
            }
        }
    }
}

/// What the render harness (`lighting::render`) needs of a page.
#[cfg(test)]
impl Page {
    /// A page for `machine`, not the app's (it is not where a power-mode change, the Fn keys
    /// or a resume reach).
    pub(crate) fn for_render(machine: Machine) -> Rc<Page> {
        Page::new(machine)
    }

    pub(crate) fn root_widget(&self) -> gtk::Widget {
        self.root.clone().upcast()
    }

    /// The Themes tab's gallery (the timing harness).
    pub(crate) fn gallery_for_timing(&self) -> Rc<crate::ui::lighting::theme_gallery::Gallery> {
        self.themes.gallery_for_timing()
    }

    /// The preview card (the timing harness).
    pub(crate) fn preview_for_render(&self) -> Rc<PreviewCard> {
        self.preview.clone()
    }

    /// Runs the preview's looks `seconds` on.
    pub(crate) fn run_preview_for_render(&self, seconds: f64) {
        self.preview.run_for_render(seconds);
    }

    /// Picks `light` in the preview's switcher, as a click on it does.
    pub(crate) fn select_light(&self, light: PreviewLight) {
        self.preview.select_for_render(light);
    }

    /// Opens `tab`, as a click on it does.
    pub(crate) fn open_tab(&self, tab: Tab) {
        match tab {
            Tab::Control => self.tabs.control.set_active(true),
            Tab::Themes => self.tabs.themes.set_active(true),
        }
    }
}

/// Runs `change`, a narrow breakpoint's re-layout of the page `scope`, with the keyboard
/// focus kept on its widget, as the wide layout's switch keeps it (`Arrangement::set`): the
/// breakpoint bin hides the page for a frame while it switches, GTK then moves a focus
/// inside it elsewhere, and libadwaita gives it back on the next frame, often to the page's
/// first control. A focus on the mode selector, whose buttons are put back
/// in place or swapped for its dropdown, is let go first and goes to what stands for it
/// after (`ModeSelect::focus_after_layout`).
fn keeping_focus(scope: &gtk::Widget, modes: &ModeSelect, change: impl FnOnce()) {
    let root = scope.root();
    let focus = root
        .as_ref()
        .and_then(|root| root.focus())
        .filter(|focus| focus == scope || focus.is_ancestor(scope));
    let in_modes = focus
        .as_ref()
        .is_some_and(|focus| focus.is_ancestor(&modes.widget));
    if let (Some(root), true) = (&root, in_modes) {
        root.set_focus(None::<&gtk::Widget>);
    }
    change();
    if let Some(focus) = focus {
        let target = if in_modes {
            modes.focus_after_layout(&focus)
        } else {
            focus
        };
        columns::take_focus(&target);
    }
}

/// The key of a light's brightness edit (`Page::edit_later`).
fn brightness_key(light: PreviewLight) -> &'static str {
    match light {
        PreviewLight::Keyboard => "brightness.keyboard",
        PreviewLight::Bar => "brightness.bar",
        PreviewLight::Logo => "brightness.logo",
    }
}

/// Scrolls the page so `widget` shows, when it is out of view: its top to the top (the
/// reference design's `scrollIntoView`, without the smooth scroll). What scrolls is the scrolled
/// window
/// around it: the one column, or on a wide page its own column; the band across the top of a
/// wide page is always in view.
pub(crate) fn bring_into_view(widget: &impl IsA<gtk::Widget>) {
    let Some(scroller) = widget
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
    else {
        return;
    };
    let Some(bounds) = widget.compute_bounds(&scroller) else {
        return;
    };
    let (top, height) = (f64::from(bounds.y()), f64::from(bounds.height()));
    let view = f64::from(scroller.height());
    if top + height < 80.0 || top > view - 80.0 {
        let adjustment = scroller.vadjustment();
        adjustment.set_value(adjustment.value() + top);
    }
}

/// `modes` in the order the page lists them.
fn in_page_order(mut modes: Vec<PowerProfile>) -> Vec<PowerProfile> {
    modes.sort_by_key(|m| MODES.iter().position(|p| p == m));
    modes
}

/// Title over one of the page's groups ("When idle").
pub(crate) fn group_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("lighting-group-title");
    label.set_xalign(0.0);
    label.set_halign(gtk::Align::Start);
    label
}

/// A boxed group. Rows appended to it get the 1-px rule between them from
/// CSS, which skips hidden rows, so hiding a row never leaves a double line.
pub(crate) fn group() -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
    group.add_css_class("lighting-group");
    group
}

/// One row of a group: title over an optional subtitle, with room on the
/// right for the caller's control. Returns the row and the subtitle label,
/// which is hidden while it has no text.
pub(crate) fn row(title: &str, sub: Option<&str>) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    row.add_css_class("lighting-row");

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("lighting-row-title");
    title_label.set_xalign(0.0);
    title_label.set_wrap(true);
    let sub_label = gtk::Label::new(None);
    sub_label.add_css_class("lighting-row-sub");
    sub_label.set_xalign(0.0);
    sub_label.set_wrap(true);
    set_text_or_hide(&sub_label, sub.unwrap_or(""));
    text.append(&title_label);
    text.append(&sub_label);
    row.append(&text);
    (row, sub_label)
}

/// An empty label still takes its padding, so it is shown only with text.
pub(crate) fn set_text_or_hide(label: &gtk::Label, text: &str) {
    label.set_text(text);
    label.set_visible(!text.is_empty());
}

/// Writes the page's slider edits still waiting, before a power-mode change replaces the
/// setup they would have changed.
pub(crate) fn flush_pending_edits() {
    if let Some(page) = page() {
        page.flush();
    }
}

/// An Fn backlight key was pressed and its level will be saved (`fn_brightness`'s step and
/// read-back routes): a keyboard brightness drag still waiting in the slider's debounce is
/// dropped now, before it could be written over the key's level while the step or the read
/// is under way.
pub(crate) fn fn_key_pressed() {
    if let Some(page) = page() {
        page.drop_waiting_keyboard_drag();
    }
}

/// After the Fn backlight keys' level was saved into a mode's setup
/// (`fn_brightness`): the page, when built, shows it on that mode's slider. Nothing is
/// written. The level is newer than a keyboard brightness drag of that mode still waiting to
/// be written, which is dropped, or it would write its older level over the key's.
pub(crate) fn refresh_brightness() {
    if let Some(page) = page() {
        page.fn_level_saved();
    }
}

/// After the Fn backlight keys' level was saved into the setup the lights show, while its
/// light bar follows the keyboard (`fn_brightness`): the bar's new level is put on the bar.
/// The lights get that setup again, on a worker, which writes only what changed, the bar
/// here (`lighting::apply_mode`). Through the page when it is built, which knows the mode
/// "Show on my laptop" puts on the lights; otherwise the active mode's.
pub(crate) fn bar_follows_fn_level() {
    if let Some(page) = page() {
        page.apply_lights();
        return;
    }
    if exiting() {
        return;
    }
    let Some(active) = profile::get_current_profile() else {
        return;
    };
    let change = lighting::mode_changed(active);
    crate::ui::background::run(
        move || lighting::apply_mode(change, "Fn brightness"),
        |_| lighting::record_written(),
    );
}

/// The desktop's reduced-motion preference changed (`window`): the preview and the theme
/// cards hold still or move again, and the lights get their setups again, since reduced
/// motion swaps the busiest looks (`Lights::looks`). Without the page, the active mode's
/// setup is applied as it is, on a worker: no mode changed.
pub(crate) fn motion_preference_changed() {
    if let Some(page) = page() {
        page.preview.motion_changed();
        page.refresh();
        page.apply_lights();
        return;
    }
    if exiting() {
        return;
    }
    let Some(active) = profile::get_current_profile() else {
        return;
    };
    let change = lighting::mode_changed(active);
    crate::ui::background::run(
        move || lighting::apply_mode(change, "reduced motion"),
        |_| lighting::record_written(),
    );
}

thread_local! {
    /// The renderer's lost bar has been seen, and the lights applied again for it
    /// ([`check_bar_lost`]).
    static BAR_LOST_SEEN: Cell<bool> = const { Cell::new(false) };
}

/// From the window's tick: once a direct colour frame to the light bar has failed, the
/// renderer no longer draws there (`lighting_renderer::Status::bar_lost`), and the bar's
/// looks resolve to their firmware fallbacks (`lighting::lights`). Then, once, the lights
/// get their setups again, so the bar shows that fallback in place of the last frame drawn,
/// and the page, when built, follows. Without the page, the active mode's setup is applied
/// on a worker, as for [`motion_preference_changed`].
pub(crate) fn check_bar_lost() {
    if BAR_LOST_SEEN.with(Cell::get)
        || !crate::hardware::lighting_renderer::renderer()
            .status()
            .bar_lost
    {
        return;
    }
    BAR_LOST_SEEN.with(|seen| seen.set(true));
    if let Some(page) = page() {
        page.refresh();
        page.apply_lights();
        return;
    }
    if exiting() {
        return;
    }
    let Some(active) = profile::get_current_profile() else {
        return;
    };
    let change = lighting::mode_changed(active);
    crate::ui::background::run(
        move || lighting::apply_mode(change, "light bar"),
        |_| lighting::record_written(),
    );
}

/// The desktop's contrast preference changed (`window`): the page's own drawing is drawn
/// again with it.
pub(crate) fn contrast_preference_changed() {
    if let Some(page) = page() {
        crate::ui::window::queue_draw_recursive(page.root.upcast_ref());
    }
}

thread_local! {
    /// Set when the app's exit begins ([`at_exit`]).
    static EXITING: Cell<bool> = const { Cell::new(false) };
}

fn exiting() -> bool {
    EXITING.with(Cell::get)
}

/// How long the exit waits for the active mode's lighting to go back on.
const EXIT_APPLY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// The first step of the app's exit (`connect_shutdown`): from now on the page starts no
/// apply, and "Show on my laptop", if on, ends here, in order: the active mode's lighting is
/// put back synchronously, under the applies' lock with a change taken now (an apply already
/// writing finishes first, one still waiting gives up), at most [`EXIT_APPLY_TIMEOUT`],
/// before the lighting renderer stops and the keyboard's timer is handed back, and what it
/// wrote is recorded, so the exit relight puts back the active mode's looks.
pub(crate) fn at_exit() {
    EXITING.with(|flag| flag.set(true));
    let Some(page) = page() else {
        return;
    };
    if !page.show_on_laptop.replace(false) {
        return;
    }
    let Some(active) = profile::get_current_profile() else {
        return;
    };
    let change = lighting::mode_changed(active);
    let (done_tx, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done_tx.send(lighting::apply_mode(change, "exit"));
    });
    match done.recv_timeout(EXIT_APPLY_TIMEOUT) {
        Ok(_) => lighting::record_written(),
        Err(_) => crate::hardware::applog::error(
            "exit: the active mode's lighting was not put back within 2 s after Show on my laptop",
        ),
    }
}

/// The machine is back from a suspend: "Show on my laptop" ends, and the resume restore
/// puts the active mode's lighting back, on every light the preview changed too
/// (`lighting::restore_saved`).
pub(crate) fn machine_resumed() {
    if let Some(page) = page() {
        page.end_show_on_laptop(false);
    }
}

/// Puts `mode`'s lighting on the lights when the power mode changes, including from the
/// physical mode key, and the page, when built, follows. "Show on my laptop" ends first
/// (this apply replaces what it showed), and a slider edit still waiting is written. The
/// lighting is applied on a worker (`lighting::apply_mode`); a change taken later
/// supersedes it. What it wrote is recorded back here, on the UI thread
/// (`lighting::record_written`), before the page follows.
pub fn apply_lighting_for_mode(mode: PowerProfile) {
    if let Some(page) = page() {
        page.end_show_on_laptop(false);
    }
    flush_pending_edits();
    let change = lighting::mode_changed(mode);
    crate::ui::background::run(
        move || lighting::apply_mode(change, "mode change"),
        move |applied| {
            lighting::record_written();
            if !applied {
                return;
            }
            if let Some(page) = page() {
                page.refresh();
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine with no light the app drives gets the empty state: the page's title and the
    /// one line saying no lighting was found, and nothing to edit. In a window never shown.
    #[test]
    fn a_machine_without_lights_gets_the_empty_state() {
        crate::ui::gtk_test::run("lighting_page", || {
            let widget = empty_state();
            let mut texts = Vec::new();
            let mut controls = 0;
            let mut stack = vec![widget.clone()];
            while let Some(w) = stack.pop() {
                if let Some(label) = w.downcast_ref::<gtk::Label>() {
                    texts.push(label.text().to_string());
                }
                if w.is::<gtk::Button>() || w.is::<gtk::Scale>() || w.is::<gtk::Switch>() {
                    controls += 1;
                }
                let mut child = w.first_child();
                while let Some(c) = child {
                    child = c.next_sibling();
                    stack.push(c);
                }
            }
            texts.sort();
            let mut wanted = vec![
                crate::i18n::t("lighting").to_string(),
                crate::i18n::t("lighting_no_devices").to_string(),
            ];
            wanted.sort();
            assert_eq!(texts, wanted);
            assert_eq!(controls, 0);
        });
    }

    /// A tab switch shows the new tab's pane at once: a transition would draw the old pane
    /// (the Themes tab's cards) over the new one while it fades or slides.
    #[test]
    fn a_tab_switch_has_no_transition() {
        crate::ui::gtk_test::run("lighting_page", || {
            let tabs = Tabs::new();
            assert_eq!(
                tabs.stack.transition_type(),
                gtk::StackTransitionType::None
            );
        });
    }

    /// Crossing a narrow breakpoint keeps the keyboard focus: on the same
    /// mode button, on the dropdown that takes the buttons' place and back on the chosen
    /// mode's button, and on any other control of the page. In a window never shown.
    #[test]
    fn the_focus_stays_across_the_narrow_breakpoints() {
        crate::ui::gtk_test::run("lighting_page", focus_across_narrow_breakpoints);
    }

    fn focus_across_narrow_breakpoints() {
        let modes = ModeSelect::new(Rc::new(|_| {}));
        let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page.append(&modes.widget);
        let other = gtk::Button::with_label("Save");
        page.append(&other);
        let window = gtk::Window::new();
        window.set_child(Some(&page));
        let focus = || gtk::prelude::RootExt::focus(&window);
        let scope: &gtk::Widget = page.upcast_ref();
        let relayout = |layout| keeping_focus(scope, &modes, || modes.set_layout(layout));

        // A mode button: the same one in three over two, which re-attaches the buttons.
        modes.update(&[], None, PowerProfile::Performance);
        let buttons: Vec<gtk::Widget> = {
            let mut found = Vec::new();
            let mut child = modes.widget.first_child();
            while let Some(widget) = child {
                if let Some(grid) = widget.downcast_ref::<gtk::Grid>() {
                    let mut cell = grid.first_child();
                    while let Some(button) = cell {
                        found.push(button.clone());
                        cell = button.next_sibling();
                    }
                }
                child = widget.next_sibling();
            }
            found
        };
        let eco = buttons
            .iter()
            .find(|b| b.has_css_class("mode-eco"))
            .expect("Eco's button")
            .clone();
        assert!(eco.grab_focus());
        relayout(ModeLayout::ThreeTwo);
        assert_eq!(focus(), Some(eco.clone()), "the same button");
        // The dropdown takes the buttons' place, and gives the focus back to the chosen
        // mode's button.
        relayout(ModeLayout::Dropdown);
        let in_dropdown = focus().expect("a focus");
        assert!(
            in_dropdown.ancestor(gtk::DropDown::static_type()).is_some()
                || in_dropdown.is::<gtk::DropDown>()
        );
        relayout(ModeLayout::Row);
        let performance = buttons
            .iter()
            .find(|b| b.has_css_class("mode-performance"))
            .expect("Performance's button");
        assert_eq!(focus().as_ref(), Some(performance), "the chosen mode's");
        // Any other control keeps it.
        assert!(other.grab_focus());
        for layout in [ModeLayout::ThreeTwo, ModeLayout::Dropdown, ModeLayout::Row] {
            relayout(layout);
            assert_eq!(focus(), Some(other.clone().upcast()), "{layout:?}");
        }
        window.destroy();
    }
}
