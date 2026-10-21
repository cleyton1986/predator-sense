//! What the Fan Control page decides, kept apart from GTK so it can be
//! tested without a display: which plan Discard puts back, how a step moves
//! when nudged, and what each temperature band is called.

use crate::config::FanPlan;
use crate::hardware::fan;
use crate::hardware::profile::PowerProfile;

/// The five modes in the order the Lighting page lists them, so the two
/// pages' mode bars read the same.
pub const MODES: [PowerProfile; 5] = [
    PowerProfile::Quiet,
    PowerProfile::Eco,
    PowerProfile::Balanced,
    PowerProfile::Performance,
    PowerProfile::Turbo,
];

/// One nudge of a curve step, and the grid every step snaps to. The chart's
/// drag uses the same 5 % snap (`fan_curve_geometry::pct_at_y`).
pub const STEP_PCT: u8 = 5;

/// Which of the four plan buttons a plan lights up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanKind {
    Automatic,
    Curve,
    Fixed,
    Max,
}

impl PlanKind {
    pub fn of(plan: FanPlan) -> Self {
        match plan {
            FanPlan::Automatic => PlanKind::Automatic,
            FanPlan::Curve { .. } => PlanKind::Curve,
            FanPlan::Fixed { .. } => PlanKind::Fixed,
            FanPlan::Max => PlanKind::Max,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PlanKind::Automatic => crate::i18n::t("automatic"),
            PlanKind::Curve => crate::i18n::t("fan_plan_curve"),
            PlanKind::Fixed => crate::i18n::t("fan_plan_fixed"),
            PlanKind::Max => crate::i18n::t("max"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            PlanKind::Automatic => crate::i18n::t("fan_plan_automatic_desc"),
            PlanKind::Curve => crate::i18n::t("fan_plan_curve_desc"),
            PlanKind::Fixed => crate::i18n::t("fan_plan_fixed_desc"),
            PlanKind::Max => crate::i18n::t("fan_plan_max_desc"),
        }
    }
}

/// Each mode's plan as it was the first time the page showed it during this
/// visit: what Discard puts back. A visit ends when the page is unmapped
/// (`clear`), so the next visit's Discard starts from what is saved then.
#[derive(Default)]
pub struct Baselines(Vec<(String, FanPlan)>);

impl Baselines {
    /// Remembers `plan` for `mode_id` unless this visit already has one: the
    /// first sight wins, later edits never move the baseline.
    pub fn note(&mut self, mode_id: &str, plan: FanPlan) {
        if !self.0.iter().any(|(id, _)| id == mode_id) {
            self.0.push((mode_id.to_string(), plan));
        }
    }

    /// The plan Discard would restore, or `None` when there is nothing to
    /// discard (no baseline yet, or the plan is back where it started).
    pub fn discard_to(&self, mode_id: &str, current: FanPlan) -> Option<FanPlan> {
        self.0
            .iter()
            .find(|(id, _)| id == mode_id)
            .map(|(_, plan)| *plan)
            .filter(|plan| *plan != current)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// `steps` with step `index` moved one notch up or down, landing on the 5 %
/// grid: a value off the grid (hand-edited config) goes to the next grid
/// line in that direction rather than keeping its offset. An index past the
/// end changes nothing.
pub fn nudge(mut steps: [u8; 6], index: usize, up: bool) -> [u8; 6] {
    if let Some(value) = steps.get_mut(index) {
        // A hand-edited config can hold a step above 100; clamp first so the
        // arithmetic below cannot overflow a u8.
        *value = (*value).min(100);
        let below = *value - *value % STEP_PCT;
        *value = if up {
            (below + STEP_PCT).min(100)
        } else if below < *value {
            below
        } else {
            value.saturating_sub(STEP_PCT)
        };
    }
    steps
}

/// The bar the arrow keys select next, stopping at either end.
pub fn moved(index: usize, right: bool) -> usize {
    if right {
        (index + 1).min(fan::FAN_CURVE_BREAKPOINTS_C.len())
    } else {
        index.saturating_sub(1)
    }
}

/// The temperature band step `index` covers, for the line under the chart.
pub fn band_label(index: usize) -> String {
    let edges = fan::FAN_CURVE_BREAKPOINTS_C;
    let c = |t: f64| format!("{}", t as i32);
    match index {
        0 => crate::i18n::tf("fan_band_below", &[&c(edges[0])]),
        i if i >= edges.len() => crate::i18n::tf("fan_band_above", &[&c(edges[edges.len() - 1])]),
        i => crate::i18n::tf("fan_band_between", &[&c(edges[i - 1]), &c(edges[i])]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mode_never_shown_has_nothing_to_discard() {
        let b = Baselines::default();
        assert_eq!(b.discard_to("quiet", FanPlan::Max), None);
    }

    #[test]
    fn an_unchanged_plan_has_nothing_to_discard() {
        let mut b = Baselines::default();
        b.note("quiet", FanPlan::Automatic);
        assert_eq!(b.discard_to("quiet", FanPlan::Automatic), None);
    }

    #[test]
    fn a_changed_plan_discards_to_the_first_one_seen() {
        let mut b = Baselines::default();
        b.note("quiet", FanPlan::Automatic);
        b.note("quiet", FanPlan::Max); // a later refresh must not move it
        assert_eq!(b.discard_to("quiet", FanPlan::Fixed { percent: 50 }), Some(FanPlan::Automatic));
    }

    #[test]
    fn undo_after_a_discard_offers_the_discard_again() {
        let mut b = Baselines::default();
        b.note("quiet", FanPlan::Automatic);
        let edited = FanPlan::Fixed { percent: 50 };
        // Discard writes the baseline back, Undo writes the edit back.
        let restored = b.discard_to("quiet", edited).unwrap();
        assert_eq!(b.discard_to("quiet", restored), None);
        assert_eq!(b.discard_to("quiet", edited), Some(FanPlan::Automatic));
    }

    #[test]
    fn modes_keep_separate_baselines() {
        let mut b = Baselines::default();
        b.note("quiet", FanPlan::Automatic);
        b.note("turbo", FanPlan::Max);
        assert_eq!(b.discard_to("turbo", FanPlan::Automatic), Some(FanPlan::Max));
        assert_eq!(b.discard_to("quiet", FanPlan::Automatic), None);
    }

    #[test]
    fn clearing_starts_a_new_visit() {
        let mut b = Baselines::default();
        b.note("quiet", FanPlan::Automatic);
        b.clear();
        b.note("quiet", FanPlan::Max);
        assert_eq!(b.discard_to("quiet", FanPlan::Automatic), Some(FanPlan::Max));
    }

    #[test]
    fn nudge_moves_by_five_and_stops_at_the_ends() {
        let s = [0, 5, 50, 95, 100, 100];
        assert_eq!(nudge(s, 0, false)[0], 0);
        assert_eq!(nudge(s, 1, false)[1], 0);
        assert_eq!(nudge(s, 2, true)[2], 55);
        assert_eq!(nudge(s, 3, true)[3], 100);
        assert_eq!(nudge(s, 4, true)[4], 100);
    }

    #[test]
    fn nudge_snaps_an_off_grid_value_in_the_direction_asked() {
        let s = [33, 0, 0, 0, 0, 0];
        assert_eq!(nudge(s, 0, true)[0], 35);
        assert_eq!(nudge(s, 0, false)[0], 30);
    }

    #[test]
    fn nudge_clamps_a_value_above_100() {
        let s = [255, 0, 0, 0, 0, 0];
        assert_eq!(nudge(s, 0, true)[0], 100);
        assert_eq!(nudge(s, 0, false)[0], 95);
    }

    #[test]
    fn nudge_past_the_end_changes_nothing() {
        let s = [10, 20, 30, 40, 50, 60];
        assert_eq!(nudge(s, 6, true), s);
    }

    #[test]
    fn arrows_stop_at_the_first_and_last_bar() {
        assert_eq!(moved(0, false), 0);
        assert_eq!(moved(0, true), 1);
        assert_eq!(moved(5, true), 5);
    }

    #[test]
    fn every_band_has_a_label_naming_its_edges() {
        assert_eq!(band_label(0), "below 45 °C");
        assert_eq!(band_label(1), "45 to 55 °C");
        assert_eq!(band_label(5), "85 °C and up");
    }
}
