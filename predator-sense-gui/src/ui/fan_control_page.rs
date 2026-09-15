use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::{Rc, Weak};

use crate::config::{self, FanPlan};
use crate::hardware::profile::PowerProfile;
use crate::hardware::{fan, sensors};
use crate::ui::background;
use crate::ui::fan_curve_widget::{self, FanCurveView};

/// The five power modes a plan can be bound to, in the order the rest of the
/// app lists them.
const MODES: [PowerProfile; 5] = [
    PowerProfile::Eco,
    PowerProfile::Quiet,
    PowerProfile::Balanced,
    PowerProfile::Performance,
    PowerProfile::Turbo,
];

/// Starting points for someone who does not want to drag six bars. Silent
/// leans on the firmware below 65 C, which is where the reconciler hands
/// the fans back with this curve's two leading zeros (see
/// `fan::curve_resume_c`).
const PRESET_SILENT: [u8; 6] = [0, 0, 30, 45, 65, 100];
const PRESET_AGGRESSIVE: [u8; 6] = [40, 55, 70, 85, 100, 100];

/// Where the Fixed spin starts for a mode that has no fixed speed yet.
/// The old custom slider's default, and deliberately not 0: a fixed 0 is a
/// firmware handoff (see `fan::plan_target`), so landing there by default
/// would label a firmware-driven machine as running one fixed speed.
const DEFAULT_FIXED_PERCENT: u8 = 50;

/// Which of the four plan buttons a plan lights up.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlanKind {
    Automatic,
    Curve,
    Fixed,
    Max,
}

impl PlanKind {
    fn of(plan: FanPlan) -> Self {
        match plan {
            FanPlan::Automatic => PlanKind::Automatic,
            FanPlan::Curve { .. } => PlanKind::Curve,
            FanPlan::Fixed { .. } => PlanKind::Fixed,
            FanPlan::Max => PlanKind::Max,
        }
    }

    fn label(self) -> &'static str {
        match self {
            PlanKind::Automatic => crate::i18n::t("automatic"),
            PlanKind::Curve => crate::i18n::t("fan_plan_curve"),
            PlanKind::Fixed => crate::i18n::t("fan_plan_fixed"),
            PlanKind::Max => crate::i18n::t("max"),
        }
    }

    fn description(self) -> &'static str {
        match self {
            PlanKind::Automatic => crate::i18n::t("fan_plan_automatic_desc"),
            PlanKind::Curve => crate::i18n::t("fan_plan_curve_desc"),
            PlanKind::Fixed => crate::i18n::t("fan_plan_fixed_desc"),
            PlanKind::Max => crate::i18n::t("fan_plan_max_desc"),
        }
    }
}

/// Everything a refresh touches. One struct so the handlers can hold a single
/// `Weak` and nothing has to be cloned five ways.
struct Page {
    /// Mode id whose plan the controls below are editing.
    editing: RefCell<String>,
    /// While false the chips follow the machine's active mode, so opening the
    /// page always edits what is running. A chip click pins it.
    pinned: Cell<bool>,
    /// Set while a refresh is pushing config into widgets whose handlers
    /// would otherwise write it straight back.
    syncing: Cell<bool>,
    /// What refresh last put on screen: the mode being edited, its plan, and
    /// the machine's active mode. A tick that matches this changes nothing, so
    /// it must not rewrite widget state - a refresh that fires regardless is
    /// how a page ends up overwriting a value the user is in the middle of
    /// entering.
    shown: RefCell<Option<(String, FanPlan, Option<String>)>>,
    /// Each mode's last seen curve, so a mode's own steps survive a trip
    /// through another plan. The global `fan_curve_points` is only the seed
    /// for a mode that has never had a curve; without this, clicking
    /// Automatic and then Curve again replaced this mode's curve with
    /// whichever one was edited last, anywhere.
    ///
    /// Scoped to the session on purpose. Across a restart a mode's persisted
    /// `FanPlan` is whatever it was, so there is no earlier curve to lose:
    /// this only has to cover the window where the mode is parked on another
    /// plan and its own steps exist nowhere else.
    curve_memory: RefCell<Vec<(String, [u8; 6])>>,
    chips: Vec<(gtk::Button, String)>,
    active_label: gtk::Label,
    plan_buttons: Vec<(gtk::Button, PlanKind)>,
    plan_title: gtk::Label,
    plan_desc: gtk::Label,
    fixed_box: gtk::Box,
    fixed_spin: gtk::SpinButton,
    curve_box: gtk::Box,
    curve_view: FanCurveView,
    spins: Vec<gtk::SpinButton>,
    status: gtk::Label,
}

/// The plan bound to a mode id, with `Automatic` for a mode that has none -
/// the same fallback `fan::plan_for` uses.
fn plan_of(cfg: &config::AppConfig, mode_id: &str) -> FanPlan {
    fan::plan_for_id(&cfg.fan_plans, mode_id).unwrap_or(FanPlan::Automatic)
}

/// Notes `steps` as `mode_id`'s curve for the rest of the session.
fn remember_curve(page: &Rc<Page>, mode_id: &str, steps: [u8; 6]) {
    let mut memory = page.curve_memory.borrow_mut();
    match memory.iter_mut().find(|(id, _)| id == mode_id) {
        Some((_, remembered)) => *remembered = steps,
        None => memory.push((mode_id.to_string(), steps)),
    }
}

/// `mode_id`'s curve as last seen, or `None` if this session never saw one.
fn recall_curve(page: &Rc<Page>, mode_id: &str) -> Option<[u8; 6]> {
    page.curve_memory
        .borrow()
        .iter()
        .find(|(id, _)| id == mode_id)
        .map(|(_, steps)| *steps)
}

/// Binds `plan` to `mode_id`. Intent only: the reconciler applies it.
fn write_plan(mode_id: &str, plan: FanPlan) -> Result<(), String> {
    let mut cfg = config::load_app_config();
    cfg.fan_plans = fan::with_plan(&cfg.fan_plans, mode_id, plan);
    config::save_app_config(&cfg)
}

/// Binds a curve and remembers its steps globally.
///
/// `fan_curve_points` no longer drives the fans - the steps inside the mode's
/// own `FanPlan::Curve` do - but it is still the seed for a mode that has
/// never had a curve, so the last curve edited anywhere is where the next
/// mode starts rather than the hardcoded default.
fn write_curve(mode_id: &str, steps: [u8; 6]) -> Result<(), String> {
    let mut cfg = config::load_app_config();
    cfg.fan_plans = fan::with_plan(&cfg.fan_plans, mode_id, FanPlan::Curve { steps });
    cfg.fan_curve_points = steps;
    config::save_app_config(&cfg)
}

/// The translated name of a mode id, empty for an id that is not one of the
/// five. Owned rather than `&'static str` because `PowerProfile::label` ties
/// its result to the borrow of the profile it was called on.
fn mode_label(mode_id: &str) -> String {
    MODES
        .iter()
        .find(|profile| profile.to_id() == mode_id)
        .map_or_else(String::new, |profile| profile.label().to_string())
}

/// Pushes config into every control. The only place that decides what the
/// page shows, so the chips, the buttons and the editors cannot disagree.
fn refresh(page: &Rc<Page>) {
    let cfg = config::load_app_config();
    let editing = page.editing.borrow().clone();
    let plan = plan_of(&cfg, &editing);
    let kind = PlanKind::of(plan);
    let active_id = crate::hardware::profile::get_current_profile().map(|p| p.to_id().to_string());

    // Nothing to do when the screen already says this. The follow timer calls
    // in every three seconds whether anything moved or not, and pushing values
    // back into the spin buttons on each of those ticks would fight the user's
    // hands. Any real change - a chip, a plan button, a step edit, the mode
    // key - lands in one of these three, so the comparison lets it through.
    let state = (editing.clone(), plan, active_id.clone());
    if page.shown.borrow().as_ref() == Some(&state) {
        return;
    }

    for (button, id) in &page.chips {
        button.remove_css_class("accent-button");
        button.remove_css_class("secondary-button");
        button.add_css_class(if *id == editing {
            "accent-button"
        } else {
            "secondary-button"
        });
    }
    for (button, button_kind) in &page.plan_buttons {
        button.remove_css_class("accent-button");
        button.remove_css_class("secondary-button");
        button.add_css_class(if *button_kind == kind {
            "accent-button"
        } else {
            "secondary-button"
        });
    }

    let editing_label = mode_label(&editing);
    page.plan_title
        .set_text(&crate::i18n::tf("fan_plan_for", &[&editing_label]));
    // Which mode the reconciler is actually driving, which is not necessarily
    // the one the controls below are editing.
    page.active_label.set_text(&match active_id.as_deref() {
        Some(id) => crate::i18n::tf("fan_active_mode_is", &[&mode_label(id)]),
        None => String::new(),
    });
    page.plan_desc.set_text(match plan {
        // A fixed 0 is not a speed: plan_target hands the fans to the
        // firmware, because a manual pwm of 0 is the EC floor, not off.
        FanPlan::Fixed { percent: 0 } => crate::i18n::t("fan_plan_fixed_zero_desc"),
        _ => kind.description(),
    });
    page.fixed_box.set_visible(kind == PlanKind::Fixed);
    page.curve_box.set_visible(kind == PlanKind::Curve);

    let steps = match plan {
        FanPlan::Curve { steps } => {
            remember_curve(page, &editing, steps);
            steps
        }
        // What clicking Curve would restore for THIS mode, not what some
        // other mode was last set to.
        _ => recall_curve(page, &editing).unwrap_or(cfg.fan_curve_points),
    };
    page.syncing.set(true);
    // Unconditional, and never left at the spin's own 0 minimum: a click on
    // Fixed reads this value straight back out, and `FanPlan::Fixed` with a
    // percent of 0 is a firmware handoff, not one fixed speed.
    page.fixed_spin.set_value(f64::from(match plan {
        FanPlan::Fixed { percent } => percent,
        _ => DEFAULT_FIXED_PERCENT,
    }));
    page.curve_view.set_steps(steps);
    for (spin, &pct) in page.spins.iter().zip(steps.iter()) {
        spin.set_value(f64::from(pct));
    }
    page.syncing.set(false);

    *page.shown.borrow_mut() = Some(state);
}

/// Says what a write did, including the case that confuses people most:
/// editing a mode you are not currently in.
fn report(page: &Rc<Page>, result: Result<(), String>) {
    let editing = page.editing.borrow().clone();
    let active = crate::hardware::profile::get_current_profile().map(|p| p.to_id().to_string());
    match result {
        Ok(()) => {
            let text = if active.as_deref() == Some(editing.as_str()) {
                crate::i18n::t("fan_plan_applying").to_string()
            } else {
                crate::i18n::tf("fan_plan_saved_other_mode", &[&mode_label(&editing)])
            };
            page.status.set_text(&text);
            page.status.remove_css_class("status-error");
            page.status.add_css_class("status-success");
        }
        Err(error) => {
            page.status.set_text(&error);
            page.status.remove_css_class("status-success");
            page.status.add_css_class("status-error");
        }
    }
}

pub fn build() -> gtk::Box {
    let page_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page_box.set_margin_top(14);
    page_box.set_margin_bottom(10);
    page_box.set_margin_start(20);
    page_box.set_margin_end(20);

    let caps = crate::hardware::capabilities::get();
    let cfg = config::load_app_config();

    let header = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    header.add_css_class("fan-rpm");
    header.set_halign(gtk::Align::End);
    page_box.append(&header);

    let title = gtk::Label::new(Some(crate::i18n::t("fan_title")));
    title.add_css_class("section-title");
    title.set_halign(gtk::Align::Center);
    page_box.append(&title);

    // ---- mode chips ----
    let editing_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    editing_row.set_halign(gtk::Align::Center);
    editing_row.set_margin_top(6);
    let editing_label = gtk::Label::new(Some(crate::i18n::t("fan_editing_mode")));
    editing_label.add_css_class("info-text-dim");
    editing_row.append(&editing_label);

    let mut chips: Vec<(gtk::Button, String)> = Vec::new();
    for profile in MODES {
        let button = gtk::Button::with_label(profile.label());
        button.add_css_class("secondary-button");
        editing_row.append(&button);
        chips.push((button, profile.to_id().to_string()));
    }
    // Which mode is running, at the end of the row the chips are in, because
    // the two are easy to confuse: a chip click changes what is being edited,
    // never what the machine is doing.
    let active_label = gtk::Label::new(None);
    active_label.add_css_class("info-text-dim");
    active_label.set_margin_start(6);
    editing_row.append(&active_label);
    page_box.append(&editing_row);

    // ---- plan selector ----
    let plan_title = gtk::Label::new(None);
    plan_title.add_css_class("control-label");
    plan_title.set_halign(gtk::Align::Center);
    plan_title.set_margin_top(10);
    page_box.append(&plan_title);

    let plan_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    plan_row.set_halign(gtk::Align::Center);
    plan_row.set_margin_top(4);

    // Curve and Fixed need per-fan PWM. Without it the EC has only its own
    // Auto and Max presets, so offering the other two would be offering
    // something `fan::plan_for_hardware` immediately resolves away.
    let kinds: Vec<PlanKind> = if caps.fan_pwm {
        vec![
            PlanKind::Automatic,
            PlanKind::Curve,
            PlanKind::Fixed,
            PlanKind::Max,
        ]
    } else {
        vec![PlanKind::Automatic, PlanKind::Max]
    };
    let mut plan_buttons: Vec<(gtk::Button, PlanKind)> = Vec::new();
    for kind in kinds {
        let button = gtk::Button::with_label(kind.label());
        button.add_css_class("secondary-button");
        plan_row.append(&button);
        plan_buttons.push((button, kind));
    }
    page_box.append(&plan_row);

    let plan_desc = gtk::Label::new(None);
    plan_desc.add_css_class("info-note");
    plan_desc.set_halign(gtk::Align::Center);
    plan_desc.set_wrap(true);
    plan_desc.set_justify(gtk::Justification::Center);
    page_box.append(&plan_desc);

    // ---- Fixed ----
    let fixed_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fixed_box.set_halign(gtk::Align::Center);
    fixed_box.set_margin_top(6);
    let fixed_label = gtk::Label::new(Some(crate::i18n::t("fan_fixed_percent")));
    fixed_label.add_css_class("info-text-dim");
    let fixed_spin = gtk::SpinButton::with_range(0.0, 100.0, 5.0);
    fixed_box.append(&fixed_label);
    fixed_box.append(&fixed_spin);
    fixed_box.set_visible(false);
    page_box.append(&fixed_box);

    // ---- Curve ----
    let curve_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    curve_box.set_margin_top(6);
    curve_box.set_visible(false);
    let seed = match plan_of(&cfg, MODES[2].to_id()) {
        FanPlan::Curve { steps } => steps,
        _ => cfg.fan_curve_points,
    };
    let curve_view = fan_curve_widget::build(seed);
    curve_box.append(&curve_view.widget);

    let steps_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    steps_row.set_halign(gtk::Align::Center);
    const BREAKPOINT_LABELS: [&str; 6] =
        ["< 45°C", "< 55°C", "< 65°C", "< 75°C", "< 85°C", "≥ 85°C"];
    let mut spins: Vec<gtk::SpinButton> = Vec::new();
    for (i, text) in BREAKPOINT_LABELS.iter().enumerate() {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let label = gtk::Label::new(Some(text));
        label.add_css_class("info-text-dim");
        let spin = gtk::SpinButton::with_range(0.0, 100.0, 5.0);
        spin.set_value(f64::from(seed[i]));
        column.append(&label);
        column.append(&spin);
        steps_row.append(&column);
        spins.push(spin);
    }
    curve_box.append(&steps_row);

    let presets_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    presets_row.set_halign(gtk::Align::Center);
    presets_row.set_margin_top(4);
    let presets_label = gtk::Label::new(Some(crate::i18n::t("fan_curve_presets")));
    presets_label.add_css_class("info-text-dim");
    presets_row.append(&presets_label);
    let preset_buttons: Vec<(gtk::Button, [u8; 6])> = vec![
        (
            gtk::Button::with_label(crate::i18n::t("fan_curve_preset_silent")),
            PRESET_SILENT,
        ),
        (
            gtk::Button::with_label(crate::i18n::t("fan_curve_preset_balanced")),
            fan::DEFAULT_FAN_CURVE,
        ),
        (
            gtk::Button::with_label(crate::i18n::t("fan_curve_preset_aggressive")),
            PRESET_AGGRESSIVE,
        ),
    ];
    for (button, _) in &preset_buttons {
        button.add_css_class("secondary-button");
        presets_row.append(button);
    }
    curve_box.append(&presets_row);
    page_box.append(&curve_box);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");
    page_box.append(&status);

    if !caps.fan_pwm {
        let note = gtk::Label::new(Some(crate::i18n::t("fan_no_pwm_note")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Center);
        note.set_justify(gtk::Justification::Center);
        note.set_wrap(true);
        page_box.append(&note);
    }

    // ---- CoolBoost, its own section: a firmware boost that is not a plan ----
    if caps.ec {
        let cb_title = gtk::Label::new(Some("CoolBoost™"));
        cb_title.add_css_class("section-title");
        cb_title.set_halign(gtk::Align::Center);
        cb_title.set_margin_top(12);
        page_box.append(&cb_title);

        let cb_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        cb_row.set_halign(gtk::Align::Center);
        let cb_desc = gtk::Label::new(Some(crate::i18n::t("fan_coolboost_desc")));
        cb_desc.add_css_class("info-note");
        cb_desc.set_wrap(true);
        let cb_switch = gtk::Switch::new();
        cb_switch.set_valign(gtk::Align::Center);
        cb_switch.set_sensitive(false);
        cb_row.append(&cb_desc);
        cb_row.append(&cb_switch);
        page_box.append(&cb_row);

        // The EC read costs roughly 150 ms through the helper, so it happens
        // on a worker and the switch stays insensitive until it answers.
        // Connecting the handler only after `set_active` is what keeps the
        // read from writing the value straight back.
        background::run(fan::get_coolboost, move |enabled| {
            cb_switch.set_active(enabled);
            cb_switch.connect_state_set(move |_, enabled| {
                let _ = fan::set_coolboost(enabled);
                let mut c = config::load_app_config();
                c.coolboost_enabled = enabled;
                let _ = config::save_app_config(&c);
                glib::Propagation::Proceed
            });
            cb_switch.set_sensitive(true);
        });
    }

    // ---- wire it up ----
    let initial = crate::hardware::profile::get_current_profile()
        .map_or_else(|| MODES[2].to_id().to_string(), |p| p.to_id().to_string());
    let page = Rc::new(Page {
        editing: RefCell::new(initial),
        pinned: Cell::new(false),
        syncing: Cell::new(false),
        shown: RefCell::new(None),
        curve_memory: RefCell::new(Vec::new()),
        chips,
        active_label,
        plan_buttons,
        plan_title,
        plan_desc,
        fixed_box,
        fixed_spin,
        curve_box,
        curve_view,
        spins,
        status,
    });

    for (button, mode_id) in &page.chips {
        let weak: Weak<Page> = Rc::downgrade(&page);
        let mode_id = mode_id.clone();
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            *page.editing.borrow_mut() = mode_id.clone();
            page.pinned.set(true);
            page.status.set_text("");
            refresh(&page);
        });
    }

    for (button, kind) in &page.plan_buttons {
        let weak: Weak<Page> = Rc::downgrade(&page);
        let kind = *kind;
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            let mode_id = page.editing.borrow().clone();
            let result = match kind {
                PlanKind::Automatic => write_plan(&mode_id, FanPlan::Automatic),
                PlanKind::Max => write_plan(&mode_id, FanPlan::Max),
                PlanKind::Fixed => write_plan(
                    &mode_id,
                    FanPlan::Fixed {
                        percent: page.fixed_spin.value() as u8,
                    },
                ),
                // Seeded from whatever the chart is showing, which `refresh`
                // set from this mode's own curve or from the last curve
                // edited anywhere.
                PlanKind::Curve => write_curve(&mode_id, page.curve_view.steps()),
            };
            report(&page, result);
            refresh(&page);
        });
    }

    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page.fixed_spin.connect_value_changed(move |spin| {
            let Some(page) = weak.upgrade() else { return };
            if page.syncing.get() {
                return;
            }
            let mode_id = page.editing.borrow().clone();
            let result = write_plan(
                &mode_id,
                FanPlan::Fixed {
                    percent: spin.value() as u8,
                },
            );
            report(&page, result);
        });
    }

    for (i, spin) in page.spins.iter().enumerate() {
        let weak: Weak<Page> = Rc::downgrade(&page);
        spin.connect_value_changed(move |spin| {
            let Some(page) = weak.upgrade() else { return };
            if page.syncing.get() {
                return;
            }
            let mut steps = page.curve_view.steps();
            steps[i] = spin.value() as u8;
            let mode_id = page.editing.borrow().clone();
            let result = write_curve(&mode_id, steps);
            page.curve_view.set_steps(steps);
            report(&page, result);
        });
    }

    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page.curve_view.set_on_change(move |steps| {
            let Some(page) = weak.upgrade() else { return };
            debug_assert!(
                !page.syncing.get(),
                "the chart fired on_change during a refresh: fan_curve_widget::set_steps \
                 must never invoke the callback"
            );
            let mode_id = page.editing.borrow().clone();
            let result = write_curve(&mode_id, steps);
            page.syncing.set(true);
            for (spin, &pct) in page.spins.iter().zip(steps.iter()) {
                spin.set_value(f64::from(pct));
            }
            page.syncing.set(false);
            report(&page, result);
        });
    }

    for (button, steps) in preset_buttons {
        let weak: Weak<Page> = Rc::downgrade(&page);
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            let mode_id = page.editing.borrow().clone();
            let result = write_curve(&mode_id, steps);
            report(&page, result);
            refresh(&page);
        });
    }

    refresh(&page);

    // Follow the machine: the power mode changes from the Modes page, the
    // physical Predator key and the AI assistant, and a plan changes from the
    // Performance/Turbo setting in Settings. Re-reading on a timer is the same
    // approach the page used before, minus the EC round trip it no longer
    // needs: the plan is in config, not the hardware.
    //
    // This timer is also the one owner of the `Rc<Page>`: every handler above
    // is attached to a widget the `Page` itself holds, so those close over a
    // `Weak` to avoid a reference cycle. Something has to keep the strong
    // reference or `build()` returning would drop the last one and leave every
    // `upgrade()` failing, which is a page that draws but does nothing. The
    // timer already holds the page box for its `is_mapped()` check, so it does
    // not outlive anything it did not already keep alive.
    {
        let page = page.clone();
        let page_widget = page_box.clone();
        glib::timeout_add_seconds_local(3, move || {
            if !crate::app_state::is_window_visible() || !page_widget.is_mapped() {
                return glib::ControlFlow::Continue;
            }
            if !page.pinned.get() {
                if let Some(active) = crate::hardware::profile::get_current_profile() {
                    if *page.editing.borrow() != active.to_id() {
                        *page.editing.borrow_mut() = active.to_id().to_string();
                    }
                }
            }
            refresh(&page);
            glib::ControlFlow::Continue
        });
    }

    // Fan cards - same faceted-card shape as Temperatures: name as the
    // card's own title, RPM + temp on the left, the animated fan gauge
    // alone (unchanged drawing, no text baked into the canvas anymore) on
    // the right. Stacked one above the other, not side by side.
    let fans_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    fans_box.set_halign(gtk::Align::Fill);
    fans_box.set_margin_top(10);

    let rotation = Rc::new(RefCell::new(0.0f64));
    let cpu_rpm = Rc::new(RefCell::new(0u32));
    let gpu_rpm = Rc::new(RefCell::new(0u32));
    let cpu_temp = Rc::new(RefCell::new(50.0f64));
    let gpu_temp = Rc::new(RefCell::new(45.0f64));

    let accent = crate::ui::brand_theme::accent().bright;

    let (cpu_card, cpu_rpm_l, cpu_temp_l) = build_fan_card("CPU", accent);
    {
        let rot = rotation.clone();
        let rpm = cpu_rpm.clone();
        cpu_card.gauge.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *rpm.borrow());
        });
    }

    let (gpu_card, gpu_rpm_l, gpu_temp_l) = build_fan_card("GPU", accent);
    {
        let rot = rotation.clone();
        let rpm = gpu_rpm.clone();
        gpu_card.gauge.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *rpm.borrow());
        });
    }

    let cpu_fan_da = cpu_card.gauge.clone();
    let gpu_fan_da = gpu_card.gauge.clone();

    fans_box.append(&cpu_card.widget);
    fans_box.append(&gpu_card.widget);
    page_box.append(&fans_box);

    // Spin rate for the gauges. The sensor timer below recomputes it from the
    // plan of the mode that is actually running; the animation timer only
    // reads it, so the 60 ms tick never loads config or touches the EC.
    let anim_speed = Rc::new(Cell::new(0.10f64));

    // Animation timer (~30fps)
    let cpu_da = cpu_fan_da.clone();
    let gpu_da = gpu_fan_da.clone();
    let rot_c = rotation.clone();
    let anim_speed_anim = anim_speed.clone();

    let page_anim = page_box.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
        if !crate::app_state::is_window_visible() || !page_anim.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        let speed = anim_speed_anim.get();
        let mut r = rot_c.borrow_mut();
        *r += speed;
        if *r > 2.0 * PI {
            *r -= 2.0 * PI;
        }
        drop(r);
        cpu_da.queue_draw();
        gpu_da.queue_draw();
        glib::ControlFlow::Continue
    });

    // Sensor update (every 2s, pausado quando window oculto)
    let cr2 = cpu_rpm.clone();
    let gr2 = gpu_rpm.clone();
    let ct2 = cpu_temp.clone();
    let gt2 = gpu_temp.clone();
    let page_sense = page_box.clone();
    let weak_sensors: Weak<Page> = Rc::downgrade(&page);
    let anim_speed_sense = anim_speed.clone();
    glib::timeout_add_seconds_local(2, move || {
        if !crate::app_state::is_window_visible() || !page_sense.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        let data = sensors::read_all_sensors();
        if let Some(r) = data.cpu_fan_rpm {
            *cr2.borrow_mut() = r;
            cpu_rpm_l.set_text(&r.to_string());
        }
        if let Some(r) = data.gpu_fan_rpm {
            *gr2.borrow_mut() = r;
            gpu_rpm_l.set_text(&r.to_string());
        }
        if let Some(t) = data.cpu_temp {
            *ct2.borrow_mut() = t;
            cpu_temp_l.set_text(&format!("{}°C", t as i32));
        }
        if let Some(t) = data.gpu_temp {
            *gt2.borrow_mut() = t;
            gpu_temp_l.set_text(&format!("{}°C", t as i32));
        }

        // The chart's marker and the fan animation both want to know what is
        // actually running, which is the active mode's plan - not the mode
        // being edited above.
        let live = fan::curve_input_temp(data.cpu_temp, data.gpu_temp);
        if let Some(page) = weak_sensors.upgrade() {
            page.curve_view.set_temp(live);
        }
        let cfg = config::load_app_config();
        let running = fan::plan_for(
            crate::hardware::profile::get_current_profile(),
            &cfg.fan_plans,
        );
        // Spin rate follows the plan, not just the raw RPM: Max should read as
        // visibly fast and Automatic as visibly gentle, even though real RPM
        // alone doesn't always make that contrast obvious (Automatic can
        // already be spinning at several thousand RPM under load). Fixed and
        // Curve keep the RPM-proportional feel, since there the speed is
        // something the user chose.
        anim_speed_sense.set(match running {
            FanPlan::Max => 0.85,
            FanPlan::Automatic => 0.10,
            FanPlan::Fixed { percent } => (f64::from(percent) / 100.0).clamp(0.05, 1.0) * 0.5,
            FanPlan::Curve { .. } => {
                let avg = f64::from(*cr2.borrow() + *gr2.borrow()) / 2.0;
                (avg / 6000.0).clamp(0.05, 1.0) * 0.5
            }
        });
        glib::ControlFlow::Continue
    });

    page_box
}

struct FanCard {
    widget: gtk::Widget,
    gauge: gtk::DrawingArea,
}

/// Builds one fan's card - same faceted-card shape as Temperatures: name +
/// big RPM value + temp on the left, the animated gauge alone (no text
/// baked into its own canvas anymore) on the right. Returns the card plus
/// the two labels the caller keeps updating from the sensor timer.
fn build_fan_card(name: &str, accent: (f64, f64, f64)) -> (FanCard, gtk::Label, gtk::Label) {
    // `name` ("CPU"/"GPU") is the card's own built-in title now, not a
    // label inside the content - one less line competing for space with
    // the bigger RPM/temp text below.
    let card = crate::ui::faceted_card::build(accent, Some(name));
    card.widget.set_size_request(460, 230);
    card.widget.set_hexpand(true);
    card.content.set_valign(gtk::Align::Center);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 6);
    info.set_valign(gtk::Align::Center);
    info.set_hexpand(true);

    let rpm_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    rpm_row.set_halign(gtk::Align::Start);
    let rpm_l = gtk::Label::new(Some("--"));
    rpm_l.add_css_class("fan-card-value");
    let rpm_unit = gtk::Label::new(Some("RPM"));
    rpm_unit.add_css_class("fan-card-unit");
    rpm_unit.set_valign(gtk::Align::End);
    rpm_row.append(&rpm_l);
    rpm_row.append(&rpm_unit);

    let temp_l = gtk::Label::new(Some("--°C"));
    temp_l.add_css_class("fan-card-temp");
    temp_l.set_halign(gtk::Align::Start);

    info.append(&rpm_row);
    info.append(&temp_l);

    let gauge = gtk::DrawingArea::new();
    gauge.set_size_request(210, 210);

    card.content.append(&info);
    card.content.append(&gauge);

    (
        FanCard {
            widget: card.widget,
            gauge,
        },
        rpm_l,
        temp_l,
    )
}

/// Draws one ring of thin, curved slivers at `rot` - either flat-colored
/// (motion-blur echoes behind the live position) or filled with `grad` (the
/// live sliver set). Each sliver sweeps from `inner_r` to `outer_r`, curling
/// forward by `curl` radians so the whole ring reads as a spiral, with an
/// open center (no hub disc) between the slivers and the middle of the icon.
///
/// This is our own parametric shape (hub-edge arc -> bezier out to the rim
/// -> rim arc back -> bezier back to the hub-edge, each sliver curled
/// relative to the last) - see the note on `draw_animated_fan` below for why
/// that distinction matters here.
#[allow(clippy::too_many_arguments)]
fn draw_blade_ring(
    cr: &gtk4::cairo::Context,
    cx: f64,
    cy: f64,
    inner_r: f64,
    mid_r: f64,
    outer_r: f64,
    sliver_count: u32,
    rot: f64,
    grad: Option<&gtk4::cairo::RadialGradient>,
    flat: (f64, f64, f64, f64),
) {
    let sector = 2.0 * PI / sliver_count as f64;
    let width = sector * 0.24;
    let curl = 0.75;

    for i in 0..sliver_count {
        let a0 = rot + i as f64 * sector;
        let a1 = a0 + width;
        // Same curl offset on both edges: a ribbon of constant angular width
        // that sweeps forward as it extends outward, not a wedge that flares
        // wider toward the rim - that flare was what made the first attempt
        // read as a solid glowing ring instead of separate curved slivers.
        let outer_a0 = a0 + curl;
        let outer_a1 = a1 + curl;

        cr.new_sub_path();
        cr.arc(cx, cy, inner_r, a0, a1);
        cr.curve_to(
            cx + mid_r * a1.cos(),
            cy + mid_r * a1.sin(),
            cx + mid_r * outer_a1.cos(),
            cy + mid_r * outer_a1.sin(),
            cx + outer_r * outer_a1.cos(),
            cy + outer_r * outer_a1.sin(),
        );
        cr.arc_negative(cx, cy, outer_r, outer_a1, outer_a0);
        cr.curve_to(
            cx + mid_r * outer_a0.cos(),
            cy + mid_r * outer_a0.sin(),
            cx + mid_r * a0.cos(),
            cy + mid_r * a0.sin(),
            cx + inner_r * a0.cos(),
            cy + inner_r * a0.sin(),
        );
        cr.close_path();

        match grad {
            Some(g) => {
                let _ = cr.set_source(g);
            }
            None => cr.set_source_rgba(flat.0, flat.1, flat.2, flat.3),
        }
        let _ = cr.fill();
    }
}

/// Animated CPU/GPU fan gauge: a spiral of thin, curved slivers around an
/// open center, spinning at a rate proportional to real RPM, plus a short
/// motion-blur trail at higher speed.
///
/// The dark-teal/bright-teal/dark-teal gradient stops (`#002e40`/`#006476`)
/// match Acer's own real fan icon from the official PredatorSense app
/// (`fan_large_nb.svg`) - reusing a color recipe is not the same as reusing
/// art, so only the two hex values are borrowed here. The sliver shapes
/// themselves are generated by `draw_blade_ring`'s own bezier math, drawn
/// from scratch - Acer's icon is a single hand-authored illustration path
/// (24 fixed slivers, exact coordinates), which is exactly the kind of
/// copyrighted vector artwork this project does not trace or ship.
fn draw_animated_fan(cr: &gtk4::cairo::Context, w: f64, h: f64, rotation: f64, rpm: u32) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    // Proportional to the canvas, not fixed px - the fixed 68.0/16.0 this
    // used to be (sized for the old 150x150 canvas) stayed that size no
    // matter how much bigger the `DrawingArea` itself grew, which is why
    // enlarging the gauge alone didn't enlarge the fan drawn inside it.
    let outer_r = (w.min(h) / 2.0) * 0.85;
    let inner_r = outer_r * (16.0 / 68.0);
    let mid_r = inner_r + (outer_r - inner_r) * 0.5;

    let intensity = if rpm > 0 {
        (rpm as f64 / 5000.0).clamp(0.2, 1.0)
    } else {
        0.2
    };
    let sliver_count = 18;

    // One radial gradient, centered on the icon, colors every sliver by its
    // own position in it: dark near the open center, bright around
    // mid-radius, dark again at the tips - the same read as the reference,
    // achieved by where each sliver sits in the field rather than by any
    // per-sliver coloring of our own.
    let grad = gtk4::cairo::RadialGradient::new(cx, cy, inner_r * 0.3, cx, cy, outer_r);
    grad.add_color_stop_rgba(0.0, 0.0, 0.180, 0.251, 0.0);
    grad.add_color_stop_rgba(0.4, 0.0, 0.180, 0.251, 0.35 + intensity * 0.25);
    grad.add_color_stop_rgba(
        0.7,
        0.0,
        0.45 + intensity * 0.35,
        0.52 + intensity * 0.4,
        0.85,
    );
    grad.add_color_stop_rgba(1.0, 0.0, 0.180, 0.251, 0.55);

    // Motion-blur trail: faded echoes just behind the live position, denser
    // (relatively) at higher RPM since the live sliver fill also brightens.
    for trail in 1..=2 {
        let trail_rot = rotation - trail as f64 * 0.09;
        draw_blade_ring(
            cr,
            cx,
            cy,
            inner_r,
            mid_r,
            outer_r,
            sliver_count,
            trail_rot,
            None,
            (0.0, 0.55, 0.62, intensity * 0.10 / trail as f64),
        );
    }
    draw_blade_ring(
        cr,
        cx,
        cy,
        inner_r,
        mid_r,
        outer_r,
        sliver_count,
        rotation,
        Some(&grad),
        (0.0, 0.0, 0.0, 0.0),
    );
}
