use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::{Rc, Weak};

use crate::config::{self, FanPlan};
use crate::hardware::profile::PowerProfile;
use crate::hardware::{fan, sensors};
use crate::ui::background;
use crate::ui::fan_control_model::{self as model, PlanKind, MODES};
use crate::ui::fan_curve_widget::{self, FanCurveView};
use crate::ui::mode_bar::ModeBar;

/// Starting points for someone who does not want to drag six bars. With
/// Silent's two leading zeros the fans are the firmware's below 55 C
/// whatever state they are in (`fan::curve_zero_region_top_c`), and they
/// stay the firmware's up to 65 C once the firmware already has them
/// (`fan::curve_resume_c`).
const PRESET_SILENT: [u8; 6] = [0, 0, 30, 45, 65, 100];
const PRESET_AGGRESSIVE: [u8; 6] = [40, 55, 70, 85, 100, 100];

/// Where the Fixed slider starts for a mode that has no fixed speed yet.
/// The old custom slider's default, and deliberately not 0: a fixed 0 is a
/// firmware handoff (see `fan::plan_target`), so landing there by default
/// would label a firmware-driven machine as running one fixed speed.
const DEFAULT_FIXED_PERCENT: u8 = 50;

/// Space around the page, the same as the Lighting page's.
const PAGE_MARGIN: i32 = 24;
/// The tallest the fan cards grow on a tall window; the curve chart takes
/// the rest.
const CARDS_MAX_HEIGHT: i32 = 300;

/// What refresh last put on screen: the mode being edited, its plan, the
/// machine's active mode, and every mode's plan for the mode bar's captions.
type Shown = (String, FanPlan, Option<String>, Vec<FanPlan>);

/// Everything a refresh touches. One struct so the handlers can hold a single
/// `Weak` and nothing has to be cloned five ways.
struct Page {
    /// Mode id whose plan the controls below are editing.
    editing: RefCell<String>,
    /// While false the mode bar follows the machine's active mode, so opening
    /// the page always edits what is running. Picking a mode pins it, and
    /// leaving the page unpins it again: the pin is for the visit, so the next
    /// one starts by following the machine rather than on whatever mode was
    /// last inspected.
    pinned: Cell<bool>,
    /// Set while a refresh is pushing config into widgets whose handlers
    /// would otherwise write it straight back.
    syncing: Cell<bool>,
    /// What refresh last put on screen. A tick that matches this changes
    /// nothing, so it must not rewrite widget state - a refresh that fires
    /// regardless is how a page ends up overwriting a value the user is in
    /// the middle of entering.
    shown: RefCell<Option<Shown>>,
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
    /// Each mode's plan as this visit first showed it, which Discard restores.
    baselines: RefCell<model::Baselines>,
    /// The steps a drag or nudge has reached, or the speed the slider has
    /// reached, but not yet saved, and whether a save is already scheduled.
    /// A drag crosses a 5% snap many times a second and `save_app_config`
    /// rewrites the whole file, so writing on each one both wastes work and
    /// widens the window where a crash truncates the config.
    pending_curve: RefCell<Option<[u8; 6]>>,
    pending_fixed: Cell<Option<u8>>,
    commit_queued: Cell<bool>,
    /// Whether Curve and Fixed exist here, for the mode bar's captions: a
    /// mode saved as Curve runs as Automatic without PWM, so it says so.
    fan_pwm: bool,
    mode_bar: ModeBar,
    plan_buttons: Vec<(gtk::Button, PlanKind)>,
    plan_desc: gtk::Label,
    fixed_box: gtk::Box,
    fixed_scale: gtk::Scale,
    fixed_value: gtk::Label,
    curve_box: gtk::Box,
    curve_view: FanCurveView,
    band_label: gtk::Label,
    step_value: gtk::Label,
    /// The three curve presets with the steps each writes, so a refresh can
    /// light whichever one the current curve matches.
    presets: Vec<(gtk::Button, [u8; 6])>,
    /// The "Changed" tag, shown exactly when Discard is.
    changed: gtk::Label,
    discard: gtk::Button,
    /// The divider between CoolBoost and Discard, shown only with Discard.
    discard_sep: gtk::Separator,
    toasts: adw::ToastOverlay,
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

/// The config to modify, or an error when it exists but could not be read.
///
/// `load_app_config` answers `Unreadable` with defaults, and saving those
/// back would replace every lighting scheme, game profile and macro still
/// in the file. The reconciler already refuses to do that (see the comment
/// on its migration branch); a plan button must refuse too.
fn config_to_modify() -> Result<config::AppConfig, String> {
    match config::load_app_config_source() {
        config::AppConfigSource::Loaded(cfg) => Ok(cfg),
        config::AppConfigSource::Missing => Ok(config::AppConfig::default()),
        config::AppConfigSource::Unreadable(error) => Err(error),
    }
}

/// Binds `plan` to `mode_id`. Intent only: the reconciler applies it.
fn write_plan(mode_id: &str, plan: FanPlan) -> Result<(), String> {
    let mut cfg = config_to_modify()?;
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
    let mut cfg = config_to_modify()?;
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

/// The line under the chart: which band is selected and its speed.
fn sync_step_row(page: &Rc<Page>) {
    let index = page.curve_view.selected();
    let steps = page.curve_view.steps();
    page.band_label.set_text(&model::band_label(index));
    page.step_value
        .set_text(&format!("{} %", steps.get(index).copied().unwrap_or(0)));
}

/// Writes whatever a drag, a nudge or the slider left pending, then shows
/// the result. Called by the debounce timer, and first thing by anything
/// that is about to read the plan or change which mode is edited, so a
/// pending write never lands late on top of a newer choice or another mode.
fn flush_pending(page: &Rc<Page>) {
    page.commit_queued.set(false);
    // Taken out and the borrow dropped before `write_curve`, which saves
    // config, and `report`, which reaches back into the page.
    let curve = page.pending_curve.borrow_mut().take();
    let fixed = page.pending_fixed.take();
    if curve.is_none() && fixed.is_none() {
        return;
    }
    let mode_id = page.editing.borrow().clone();
    if let Some(steps) = curve {
        report(page, write_curve(&mode_id, steps));
    }
    if let Some(percent) = fixed {
        report(page, write_plan(&mode_id, FanPlan::Fixed { percent }));
    }
    // What was just saved is what the widgets already show, so this cannot
    // fight a drag still in progress; it updates Discard and the captions.
    refresh(page);
}

/// One save when the edit settles, not one per 5% it crosses. The last
/// value wins: a later change overwrites the pending one while the single
/// scheduled commit is still waiting.
fn queue_commit(page: &Rc<Page>) {
    if page.commit_queued.get() {
        return;
    }
    page.commit_queued.set(true);
    let weak: Weak<Page> = Rc::downgrade(page);
    glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
        let Some(page) = weak.upgrade() else { return };
        flush_pending(&page);
    });
}

/// Pushes config into every control. The only place that decides what the
/// page shows, so the mode bar, the buttons and the editors cannot disagree.
fn refresh(page: &Rc<Page>) {
    let cfg = config::load_app_config();
    let editing = page.editing.borrow().clone();
    let plan = plan_of(&cfg, &editing);
    // What the reconciler will run here, so the buttons, the editor shown and
    // the mode bar caption all agree on a machine without PWM.
    let kind = model::caption_kind(plan, page.fan_pwm);
    let active = crate::hardware::profile::get_current_profile();
    let active_id = active.map(|p| p.to_id().to_string());
    let plans: Vec<FanPlan> = MODES.iter().map(|m| plan_of(&cfg, m.to_id())).collect();

    // Before the early return: the first time a visit shows a mode is what
    // Discard puts back, and that can be a tick that changes nothing else.
    page.baselines.borrow_mut().note(&editing, plan);

    // Nothing to do when the screen already says this. The follow timer calls
    // in every three seconds whether anything moved or not, and pushing values
    // back into the slider on each of those ticks would fight the user's
    // hands. Any real change - a mode, a plan button, a step edit, the mode
    // key - lands in one of these, so the comparison lets it through.
    let state = (editing.clone(), plan, active_id, plans.clone());
    if page.shown.borrow().as_ref() == Some(&state) {
        return;
    }

    let dirty = page.baselines.borrow().discard_to(&editing, plan).is_some();
    page.changed.set_visible(dirty);
    page.discard.set_visible(dirty);
    page.discard_sep.set_visible(dirty);

    let fan_pwm = page.fan_pwm;
    page.mode_bar.update(
        PowerProfile::from_id(&editing).unwrap_or(PowerProfile::Balanced),
        active,
        &|mode| {
            let i = MODES.iter().position(|m| *m == mode).unwrap_or(0);
            model::caption_kind(plans[i], fan_pwm).label().to_string()
        },
    );

    for (button, button_kind) in &page.plan_buttons {
        button.remove_css_class("accent-button");
        button.remove_css_class("secondary-button");
        button.add_css_class(if *button_kind == kind {
            "accent-button"
        } else {
            "secondary-button"
        });
    }

    page.plan_desc.set_text(match plan {
        // A fixed 0 is not a speed: plan_target hands the fans to the
        // firmware, because a manual pwm of 0 is the EC floor, not off.
        FanPlan::Fixed { percent: 0 } if kind == PlanKind::Fixed => {
            crate::i18n::t("fan_plan_fixed_zero_desc")
        }
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
    // Light the preset this curve matches, and none of them once the steps
    // have been dragged into something the user made themselves.
    for (button, preset) in &page.presets {
        button.remove_css_class("accent-button");
        button.remove_css_class("secondary-button");
        button.add_css_class(if kind == PlanKind::Curve && *preset == steps {
            "accent-button"
        } else {
            "secondary-button"
        });
    }

    // Unconditional, and never left at the slider's own 0 minimum: a click
    // on Fixed reads this value straight back out, and `FanPlan::Fixed` with
    // a percent of 0 is a firmware handoff, not one fixed speed.
    let fixed = match plan {
        FanPlan::Fixed { percent } => percent,
        _ => DEFAULT_FIXED_PERCENT,
    };
    page.syncing.set(true);
    page.fixed_scale.set_value(f64::from(fixed));
    page.curve_view.set_steps(steps);
    page.syncing.set(false);
    page.fixed_value.set_text(&format!("{fixed} %"));
    sync_step_row(page);

    *page.shown.borrow_mut() = Some(state);
}

/// A write that worked shows nothing: the page already shows the new plan,
/// and the reconciler, not the page, owns the hardware. Only a failure
/// takes the status line, and it stays until the next write or mode pick.
fn report(page: &Rc<Page>, result: Result<(), String>) {
    match result {
        Ok(()) => clear_status(page),
        Err(error) => {
            page.status.set_text(&error);
            page.status.add_css_class("status-error");
            page.status.set_visible(true);
        }
    }
}

/// Empties the status line and hides it: `.status-label`'s padding keeps
/// an empty one about 46 px tall.
fn clear_status(page: &Rc<Page>) {
    page.status.set_text("");
    page.status.remove_css_class("status-error");
    page.status.set_visible(false);
}

/// Puts the remembered plan back and offers to undo that.
fn discard(page: &Rc<Page>) {
    flush_pending(page);
    let mode_id = page.editing.borrow().clone();
    let cfg = config::load_app_config();
    let current = plan_of(&cfg, &mode_id);
    let target = page.baselines.borrow().discard_to(&mode_id, current);
    let Some(target) = target else { return };
    if let FanPlan::Curve { steps } = target {
        remember_curve(page, &mode_id, steps);
    }
    let result = write_plan(&mode_id, target);
    let written = result.is_ok();
    report(page, result);
    refresh(page);
    // A failed write already shows its error; a toast saying "discarded"
    // would contradict it.
    if !written {
        return;
    }

    let toast = adw::Toast::new(&crate::i18n::tf(
        "fan_discarded_toast",
        &[&mode_label(&mode_id)],
    ));
    toast.set_button_label(Some(crate::i18n::t("fan_undo")));
    let weak: Weak<Page> = Rc::downgrade(page);
    toast.connect_button_clicked(move |_| {
        let Some(page) = weak.upgrade() else { return };
        flush_pending(&page);
        if let FanPlan::Curve { steps } = current {
            remember_curve(&page, &mode_id, steps);
        }
        report(&page, write_plan(&mode_id, current));
        refresh(&page);
    });
    page.toasts.add_toast(toast);
}

/// A label for the fixed-width "30 %" readouts, so the row beside it does
/// not move as the number changes length.
fn value_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_width_chars(5);
    label.set_xalign(1.0);
    label.add_css_class("control-label");
    label
}

pub fn build() -> gtk::Widget {
    let caps = crate::hardware::capabilities::get();
    let cfg = config::load_app_config();

    // ---- title row: CoolBoost, the "Changed" tag and Discard on the right ----
    let title = gtk::Label::new(Some(crate::i18n::t("fan_control")));
    // No page on this branch has a large heading class of its own, so the
    // page title uses libadwaita's stock title-1.
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    title_row.append(&title);

    // CoolBoost is a firmware boost, not a plan and not per mode, so it sits
    // with the page title rather than under "Fans for".
    if caps.ec {
        let cb_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let desc = crate::i18n::t("fan_coolboost_desc");
        cb_box.set_tooltip_text(Some(desc));
        let cb_label = gtk::Label::new(Some("CoolBoost™"));
        let cb_switch = gtk::Switch::new();
        cb_switch.set_valign(gtk::Align::Center);
        cb_switch.set_sensitive(false);
        cb_switch.update_property(&[gtk::accessible::Property::Description(desc)]);
        cb_switch.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            cb_label.upcast_ref()
        ])]);
        cb_box.append(&cb_label);
        cb_box.append(&cb_switch);
        title_row.append(&cb_box);

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

    let discard_sep = gtk::Separator::new(gtk::Orientation::Vertical);
    discard_sep.set_visible(false);
    let changed_tag = gtk::Label::new(Some(crate::i18n::t("fan_changed")));
    changed_tag.add_css_class("fan-tag");
    changed_tag.add_css_class("fan-tag-warn");
    changed_tag.set_valign(gtk::Align::Center);
    changed_tag.set_visible(false);
    let discard_button = gtk::Button::with_label(crate::i18n::t("fan_discard_changes"));
    discard_button.add_css_class("secondary-button");
    discard_button.set_valign(gtk::Align::Center);
    discard_button.set_visible(false);
    // Without CoolBoost there is nothing to divide, so the separator stays
    // out of the row and its visibility changes go nowhere.
    if caps.ec {
        title_row.append(&discard_sep);
    }
    title_row.append(&changed_tag);
    title_row.append(&discard_button);

    // ---- plan selector ----
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
    let plan_buttons: Vec<(gtk::Button, PlanKind)> = kinds
        .into_iter()
        .map(|kind| {
            let button = gtk::Button::with_label(kind.label());
            button.add_css_class("secondary-button");
            (button, kind)
        })
        .collect();

    let plan_desc = gtk::Label::new(None);
    plan_desc.add_css_class("info-note");
    plan_desc.set_halign(gtk::Align::Start);
    plan_desc.set_xalign(0.0);
    plan_desc.set_wrap(true);
    plan_desc.set_hexpand(true);

    // ---- Fixed ----
    let fixed_box = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    fixed_box.set_visible(false);
    let fixed_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    fixed_scale.set_hexpand(true);
    fixed_scale.set_round_digits(0);
    fixed_scale.set_draw_value(false);
    crate::ui::scroll_guard::redirect_scroll_to_page(&fixed_scale);
    // Every move the user makes lands on the 5 % grid, the same steps the
    // curve uses; `change-value` is where GTK lets the requested value be
    // corrected before it is applied.
    fixed_scale.connect_change_value(|scale, _, value| {
        scale.set_value(f64::from(model::snap_pct(value)));
        glib::Propagation::Stop
    });
    let fixed_value = value_label();
    let fixed_label = gtk::Label::new(Some(crate::i18n::t("fan_fixed_percent")));
    fixed_label.add_css_class("control-label");
    fixed_box.append(&fixed_label);
    fixed_box.append(&fixed_scale);
    fixed_box.append(&fixed_value);

    // ---- Curve ----
    let seed = match plan_of(&cfg, PowerProfile::Balanced.to_id()) {
        FanPlan::Curve { steps } => steps,
        _ => cfg.fan_curve_points,
    };
    let curve_view = fan_curve_widget::build(seed);

    let band_label = gtk::Label::new(None);
    band_label.add_css_class("info-text-dim");
    band_label.set_width_chars(12);
    band_label.set_xalign(0.0);
    let step_value = value_label();
    let lower = gtk::Button::with_label("−");
    lower.add_css_class("secondary-button");
    lower.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
        "fan_step_lower",
    ))]);
    let raise = gtk::Button::with_label("+");
    raise.add_css_class("secondary-button");
    raise.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
        "fan_step_raise",
    ))]);

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

    let step_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    step_row.append(&band_label);
    step_row.append(&lower);
    step_row.append(&step_value);
    step_row.append(&raise);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    step_row.append(&spacer);
    let presets_label = gtk::Label::new(Some(crate::i18n::t("fan_curve_presets")));
    presets_label.add_css_class("info-text-dim");
    step_row.append(&presets_label);
    for (button, _) in &preset_buttons {
        button.add_css_class("secondary-button");
        step_row.append(button);
    }

    let curve_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    curve_box.add_css_class("card");
    curve_box.set_visible(false);
    let curve_inner = gtk::Box::new(gtk::Orientation::Vertical, 8);
    curve_inner.set_margin_top(12);
    curve_inner.set_margin_bottom(12);
    curve_inner.set_margin_start(16);
    curve_inner.set_margin_end(16);
    // Takes what the cards leave of a tall window, above its own minimum.
    curve_view.widget.set_vexpand(true);
    curve_inner.append(&curve_view.widget);
    curve_inner.append(&step_row);
    curve_box.append(&curve_inner);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");
    status.set_visible(false);

    let toasts = adw::ToastOverlay::new();

    // ---- the page state ----
    let initial = crate::hardware::profile::get_current_profile().map_or_else(
        || PowerProfile::Balanced.to_id().to_string(),
        |p| p.to_id().to_string(),
    );
    // Cyclic because the mode bar takes its handler when it is built, and
    // the handler needs the page that holds the bar.
    let page = Rc::new_cyclic(|weak: &Weak<Page>| {
        let weak = weak.clone();
        let mode_bar = ModeBar::new(&MODES, move |mode| {
            let Some(page) = weak.upgrade() else { return };
            flush_pending(&page);
            *page.editing.borrow_mut() = mode.to_id().to_string();
            page.pinned.set(true);
            clear_status(&page);
            refresh(&page);
        });
        Page {
            editing: RefCell::new(initial),
            pinned: Cell::new(false),
            syncing: Cell::new(false),
            shown: RefCell::new(None),
            curve_memory: RefCell::new(Vec::new()),
            baselines: RefCell::new(model::Baselines::default()),
            pending_curve: RefCell::new(None),
            pending_fixed: Cell::new(None),
            commit_queued: Cell::new(false),
            fan_pwm: caps.fan_pwm,
            mode_bar,
            plan_buttons,
            plan_desc,
            fixed_box,
            fixed_scale,
            fixed_value,
            curve_box,
            curve_view,
            band_label,
            step_value,
            presets: preset_buttons,
            changed: changed_tag,
            discard: discard_button,
            discard_sep,
            toasts,
            status,
        }
    });

    // ---- layout, one column top to bottom ----
    let page_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page_box.set_margin_top(PAGE_MARGIN);
    page_box.set_margin_bottom(PAGE_MARGIN);
    page_box.set_margin_start(PAGE_MARGIN);
    page_box.set_margin_end(PAGE_MARGIN);
    page_box.append(&title_row);

    let fans_for = gtk::Label::new(Some(crate::i18n::t("fan_fans_for")));
    fans_for.add_css_class("control-label");
    fans_for.set_halign(gtk::Align::Start);
    page_box.append(&fans_for);
    page.mode_bar
        .widget
        .update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            fans_for.upcast_ref()
        ])]);
    page_box.append(&page.mode_bar.widget);

    let header = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    header.add_css_class("fan-rpm");
    header.set_halign(gtk::Align::End);
    page_box.append(&header);

    // Fan cards - same faceted-card shape as Temperatures: name as the
    // card's own title, RPM + temp on the left, the animated fan gauge on
    // the right. Side by side, so the plan editor fits below them.
    let fans_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    fans_box.set_homogeneous(true);
    // Spare window height is shared between the cards, up to
    // CARDS_MAX_HEIGHT, and the curve chart. Without the curve showing
    // nothing else wants it, and a clamp still expanding would centre the
    // cards in it, so the cards then keep their natural height and the
    // spare height stays empty below.
    let cards_clamp = adw::Clamp::builder()
        .orientation(gtk::Orientation::Vertical)
        .maximum_size(CARDS_MAX_HEIGHT)
        .tightening_threshold(240)
        .child(&fans_box)
        .build();
    cards_clamp.set_vexpand(page.curve_box.is_visible());
    {
        let cards_clamp = cards_clamp.clone();
        page.curve_box
            .connect_visible_notify(move |curve| cards_clamp.set_vexpand(curve.is_visible()));
    }
    page_box.append(&cards_clamp);

    let plan_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    plan_row.set_margin_top(6);
    let plan_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    plan_group.add_css_class("linked");
    plan_group.set_valign(gtk::Align::Center);
    for (button, _) in &page.plan_buttons {
        plan_group.append(button);
    }
    plan_row.append(&plan_group);
    plan_row.append(&page.plan_desc);
    page_box.append(&plan_row);

    if !caps.fan_pwm {
        let note = gtk::Label::new(Some(crate::i18n::t("fan_no_pwm_note")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Start);
        note.set_xalign(0.0);
        note.set_wrap(true);
        page_box.append(&note);
    }

    page_box.append(&page.curve_box);
    page_box.append(&page.fixed_box);
    page_box.append(&page.status);

    // ---- wire it up ----
    // Leaving the page releases the pin, so the next visit edits the mode the
    // machine is actually in, and ends the visit Discard works within: the
    // baselines go, and `shown` too, or the first refresh of the next visit
    // would match it and return before noting anything.
    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page_box.connect_unmap(move |_| {
            let Some(page) = weak.upgrade() else { return };
            page.pinned.set(false);
            page.baselines.borrow_mut().clear();
            *page.shown.borrow_mut() = None;
        });
    }
    // And coming back notes the baselines at once rather than on the next
    // three-second tick, which an edit made in between would beat.
    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page_box.connect_map(move |_| {
            let Some(page) = weak.upgrade() else { return };
            // Reopen on the mode in force, not the one inspected last time,
            // without waiting up to three seconds for the follow timer.
            if !page.pinned.get() {
                if let Some(active) = crate::hardware::profile::get_current_profile() {
                    let moved = *page.editing.borrow() != active.to_id();
                    if moved {
                        flush_pending(&page);
                        *page.editing.borrow_mut() = active.to_id().to_string();
                    }
                }
            }
            refresh(&page);
        });
    }

    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page.discard.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            discard(&page);
        });
    }

    for (button, kind) in &page.plan_buttons {
        let weak: Weak<Page> = Rc::downgrade(&page);
        let kind = *kind;
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            flush_pending(&page);
            let mode_id = page.editing.borrow().clone();
            let result = match kind {
                PlanKind::Automatic => write_plan(&mode_id, FanPlan::Automatic),
                PlanKind::Max => write_plan(&mode_id, FanPlan::Max),
                PlanKind::Fixed => write_plan(
                    &mode_id,
                    FanPlan::Fixed {
                        percent: page.fixed_scale.value().round() as u8,
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
        page.fixed_scale.connect_value_changed(move |scale| {
            let Some(page) = weak.upgrade() else { return };
            if page.syncing.get() {
                return;
            }
            let percent = model::snap_pct(scale.value());
            page.fixed_value.set_text(&format!("{percent} %"));
            // Debounced like the curve: a slider drag fires many changes.
            page.pending_fixed.set(Some(percent));
            queue_commit(&page);
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
            // The readout keeps tracking the drag; only the save waits.
            sync_step_row(&page);
            *page.pending_curve.borrow_mut() = Some(steps);
            queue_commit(&page);
        });
    }
    {
        let weak: Weak<Page> = Rc::downgrade(&page);
        page.curve_view.set_on_select(move |_| {
            let Some(page) = weak.upgrade() else { return };
            sync_step_row(&page);
        });
    }

    // Minus and plus move the selected bar one notch through the chart's own
    // change path, so they reach the `set_on_change` handler above exactly
    // as a drag or Up/Down does.
    for (button, up) in [(&lower, false), (&raise, true)] {
        let weak: Weak<Page> = Rc::downgrade(&page);
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            page.curve_view.nudge_selected(up);
        });
    }

    for (button, steps) in &page.presets {
        let weak: Weak<Page> = Rc::downgrade(&page);
        let steps = *steps;
        button.connect_clicked(move |_| {
            let Some(page) = weak.upgrade() else { return };
            flush_pending(&page);
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
                    let moved = *page.editing.borrow() != active.to_id();
                    if moved {
                        // A pending edit belongs to the mode it was made on,
                        // so it is saved there before `editing` moves on.
                        flush_pending(&page);
                        *page.editing.borrow_mut() = active.to_id().to_string();
                    }
                }
            }
            refresh(&page);
            glib::ControlFlow::Continue
        });
    }

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
    crate::ui::ticks::while_mapped(&page_anim.clone(), 60, move || {
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

    // The page fills the content area, as the Lighting page does; a small
    // window or a large font still scrolls rather than clipping.
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&page_box));
    page.toasts.set_child(Some(&scroll));
    page.toasts.clone().upcast()
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
    card.widget.set_size_request(-1, 162);
    card.widget.set_hexpand(true);
    // faceted_card's background and content vexpand, and GTK passes that up
    // through every parent, so without this the cards took all spare window
    // height and pushed the plan editor off screen. Explicit false stops it;
    // the clamp around the cards row decides how tall they get.
    card.widget.set_vexpand(false);

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

    // The minimum size; on a taller card the gauge takes its share of the
    // width and the full height, and draw_animated_fan scales to the
    // smaller of the two.
    let gauge = gtk::DrawingArea::new();
    gauge.set_size_request(118, 118);
    gauge.set_hexpand(true);
    gauge.set_vexpand(true);

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
