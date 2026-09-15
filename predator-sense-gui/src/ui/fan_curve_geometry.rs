//! Pure geometry for the fan curve chart: percent to pixel, pixel to step,
//! and the temperature axis. Split from the widget so every conversion the
//! drawing and the dragging depend on is unit tested without a display.
//!
//! The axis runs 35 C to 95 C because that makes all six of the curve's bands
//! exactly ten degrees wide (`<45`, `45..55`, `55..65`, `65..75`, `75..85`,
//! `>=85`), so a column's width on screen is the band's real width and the
//! temperature marker lands in the band actually in force. The first and last
//! bands are open-ended in the code, so anything off the axis clamps.

/// The six editable percentages, matching `config::fan_curve_points`.
pub const STEP_COUNT: usize = 6;

/// Left edge of the temperature axis, in Celsius.
pub const AXIS_MIN_C: f64 = 35.0;
/// Right edge of the temperature axis, in Celsius.
pub const AXIS_MAX_C: f64 = 95.0;

/// Room for the percent labels down the left side.
pub const MARGIN_LEFT: f64 = 34.0;
pub const MARGIN_RIGHT: f64 = 8.0;
pub const MARGIN_TOP: f64 = 10.0;
/// Room for the temperature labels along the bottom.
pub const MARGIN_BOTTOM: f64 = 20.0;

/// A percent drag snaps to this, so dragging and the spin buttons cannot
/// disagree about which values are reachable.
const SNAP_PCT: f64 = 5.0;

/// The drawable chart area inside the widget's margins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plot {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The plot for a widget of this size. Never negative: a widget narrower than
/// its own margins gets an empty plot, which every function below treats as
/// "nothing to draw" rather than dividing by it.
pub fn plot_for(width: f64, height: f64) -> Plot {
    Plot {
        x: MARGIN_LEFT,
        y: MARGIN_TOP,
        w: (width - MARGIN_LEFT - MARGIN_RIGHT).max(0.0),
        h: (height - MARGIN_TOP - MARGIN_BOTTOM).max(0.0),
    }
}

fn column_width(plot: Plot) -> f64 {
    plot.w / STEP_COUNT as f64
}

/// The `(left, right)` pixel bounds of one step's column.
pub fn column(plot: Plot, step: usize) -> (f64, f64) {
    let width = column_width(plot);
    let left = plot.x + width * step as f64;
    (left, left + width)
}

/// Which step a horizontal position belongs to, clamped to the ends so a drag
/// that leaves the widget keeps editing the step it started on.
pub fn step_at_x(plot: Plot, x: f64) -> usize {
    let width = column_width(plot);
    if width <= 0.0 {
        return 0;
    }
    let index = ((x - plot.x) / width).floor();
    if index < 0.0 {
        return 0;
    }
    (index as usize).min(STEP_COUNT - 1)
}

/// The top of the bar for `pct`: 100% is the top of the plot, 0% the bottom.
pub fn y_for_pct(plot: Plot, pct: u8) -> f64 {
    plot.y + plot.h * (1.0 - f64::from(pct.min(100)) / 100.0)
}

/// The percent a vertical position means, clamped to 0..=100 and snapped to
/// the same five-percent grid the spin buttons step by.
pub fn pct_at_y(plot: Plot, y: f64) -> u8 {
    if plot.h <= 0.0 {
        return 0;
    }
    let raw = (1.0 - (y - plot.y) / plot.h) * 100.0;
    let snapped = (raw / SNAP_PCT).round() * SNAP_PCT;
    snapped.clamp(0.0, 100.0) as u8
}

/// Where a temperature sits on the axis, clamped to the plot.
pub fn x_for_temp(plot: Plot, temp_c: f64) -> f64 {
    let span = AXIS_MAX_C - AXIS_MIN_C;
    let fraction = ((temp_c - AXIS_MIN_C) / span).clamp(0.0, 1.0);
    plot.x + plot.w * fraction
}

/// `steps` with the step under `(x, y)` set to what that height means.
pub fn edited(mut steps: [u8; STEP_COUNT], plot: Plot, x: f64, y: f64) -> [u8; STEP_COUNT] {
    steps[step_at_x(plot, x)] = pct_at_y(plot, y);
    steps
}

/// Where the shaded firmware region ends, given `fan::curve_resume_c`.
///
/// Below that temperature a curve with a zero first step leaves the fans to
/// the firmware, which is a real behaviour of the reconciler and not a drawing
/// flourish: without showing it, a user reads the zero step as "off up to
/// 45 C" when it is actually "the firmware's up to the top of the first
/// non-zero band".
pub fn firmware_region_end_x(plot: Plot, resume_c: Option<f64>) -> Option<f64> {
    resume_c.map(|resume| x_for_temp(plot, resume))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Plot {
        // 6 columns of 100px, 200px tall.
        Plot { x: 0.0, y: 0.0, w: 600.0, h: 200.0 }
    }

    #[test]
    fn the_plot_sits_inside_the_widget_margins() {
        let p = plot_for(600.0 + MARGIN_LEFT + MARGIN_RIGHT, 200.0 + MARGIN_TOP + MARGIN_BOTTOM);
        assert_eq!(p.x, MARGIN_LEFT);
        assert_eq!(p.y, MARGIN_TOP);
        assert_eq!(p.w, 600.0);
        assert_eq!(p.h, 200.0);
    }

    #[test]
    fn a_widget_smaller_than_its_margins_gets_an_empty_plot() {
        let p = plot_for(4.0, 4.0);
        assert_eq!(p.w, 0.0);
        assert_eq!(p.h, 0.0);
    }

    #[test]
    fn the_columns_tile_the_plot_exactly() {
        let p = plot();
        assert_eq!(column(p, 0), (0.0, 100.0));
        assert_eq!(column(p, 5), (500.0, 600.0));
    }

    #[test]
    fn a_point_in_a_column_selects_that_step() {
        let p = plot();
        assert_eq!(step_at_x(p, 50.0), 0);
        assert_eq!(step_at_x(p, 250.0), 2);
        assert_eq!(step_at_x(p, 599.0), 5);
    }

    #[test]
    fn a_point_outside_the_plot_selects_the_nearest_step() {
        let p = plot();
        assert_eq!(step_at_x(p, -80.0), 0);
        assert_eq!(step_at_x(p, 9_000.0), 5);
    }

    #[test]
    fn full_speed_is_the_top_of_the_plot_and_zero_is_the_bottom() {
        let p = plot();
        assert_eq!(y_for_pct(p, 100), 0.0);
        assert_eq!(y_for_pct(p, 0), 200.0);
        assert_eq!(y_for_pct(p, 50), 100.0);
    }

    #[test]
    fn a_y_position_reads_back_as_the_percent_it_was_drawn_at() {
        let p = plot();
        for pct in [0u8, 25, 50, 75, 100] {
            assert_eq!(pct_at_y(p, y_for_pct(p, pct)), pct);
        }
    }

    #[test]
    fn a_drag_past_the_plot_clamps_to_zero_and_a_hundred() {
        let p = plot();
        assert_eq!(pct_at_y(p, -400.0), 100);
        assert_eq!(pct_at_y(p, 400.0), 0);
    }

    #[test]
    fn a_dragged_percent_snaps_to_the_same_five_the_spin_buttons_use() {
        let p = plot();
        // 63% of the way up the plot.
        assert_eq!(pct_at_y(p, 200.0 - 0.63 * 200.0), 65);
        assert_eq!(pct_at_y(p, 200.0 - 0.62 * 200.0), 60);
    }

    #[test]
    fn the_axis_spans_the_temperatures_the_curve_actually_switches_at() {
        let p = plot();
        assert_eq!(x_for_temp(p, AXIS_MIN_C), 0.0);
        assert_eq!(x_for_temp(p, AXIS_MAX_C), 600.0);
        // Every breakpoint the curve steps at is a column boundary, so the
        // drawing cannot disagree with `fan::fan_curve_pct`.
        for (i, &c) in crate::hardware::fan::FAN_CURVE_BREAKPOINTS_C.iter().enumerate() {
            assert_eq!(x_for_temp(p, c), column(p, i).1);
        }
    }

    #[test]
    fn a_temperature_off_the_axis_clamps_to_the_plot() {
        let p = plot();
        assert_eq!(x_for_temp(p, 12.0), 0.0);
        assert_eq!(x_for_temp(p, 120.0), 600.0);
    }

    #[test]
    fn an_edit_changes_only_the_step_it_lands_on() {
        let p = plot();
        let steps = [25u8, 35, 50, 65, 80, 100];
        let after = edited(steps, p, 250.0, y_for_pct(p, 20));
        assert_eq!(after, [25, 35, 20, 65, 80, 100]);
    }

    #[test]
    fn the_firmware_region_ends_where_the_curve_takes_over() {
        let p = plot();
        assert_eq!(firmware_region_end_x(p, None), None);
        assert_eq!(firmware_region_end_x(p, Some(55.0)), Some(x_for_temp(p, 55.0)));
    }
}
