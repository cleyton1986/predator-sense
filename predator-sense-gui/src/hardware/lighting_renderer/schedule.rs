//! When and how the renderer draws each light: a pure decision from what
//! the lights are asked to show and everything that pauses or stops drawing, tested row by
//! row below.
//!
//! Rows, per light, first match wins:
//!
//! | # | condition | keyboard | bar |
//! |---|---|---|---|
//! | 1 | no app-drawn look on the light | None | None |
//! | 2 | the light cannot be written (keyboard hidraw node, bar zone node) | None | None |
//! | 3 | the machine is suspending or resuming | None | None |
//! | 4 | the app's idle has the light blanked | None | None |
//! | 5 | user Pause, or on battery with "Pause animations on battery" | Upload | Write |
//! | 6 | a still look (a per-key picture, Tide at Breathing 0) | Upload | Write |
//! | 7 | one picture at a changing brightness (Tide breathing) | BrightnessOnly | Stream(5) |
//! | 8 | anything else | Stream(30) | Stream(5) |
//!
//! None writes nothing (and the light keeps whatever it shows); Upload and Write make the
//! light show the look's current frame once, so a paused theme holds its frame; Stream
//! writes changed frames at that rate; BrightnessOnly uploads the picture once and then
//! sends only the brightness commit, at the keyboard rate. On the bar a breathing picture
//! streams: the engine eases zone lights over 50 ms, so a zone frame is not exactly the
//! picture times a level, and every bar write is a whole frame anyway.
//!
//! Reduced motion (GTK's `gtk-interface-reduced-motion`) changes no row: the looks run at
//! half speed ([`Plan::rate`]).

/// Keyboard frames a second while streaming.
pub const KEYBOARD_FPS: u32 = 30;
/// Light bar frames a second while streaming: each is a colour pickup by the EC, which
/// raises SMIs of its own, so as few as still look smooth on three zones.
pub const BAR_FPS: u32 = 5;
/// How fast the looks run with reduced motion on: half speed.
pub const REDUCED_MOTION_RATE: f64 = 0.5;

/// How a look's frames change over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// Every frame is the same: a per-key picture, or Tide at Breathing 0.
    Still,
    /// One picture at a changing brightness: Tide breathing.
    Breathing,
    /// Frames of their own.
    Animated,
}

/// What the schedule needs to know about one light's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LookInfo {
    pub motion: Motion,
}

impl LookInfo {
    pub const fn new(motion: Motion) -> LookInfo {
        LookInfo { motion }
    }
}

/// Everything the decision depends on.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct State {
    /// The app-drawn look on the keyboard (the active mode's setup, or the one "Show on my
    /// laptop" shows), as the renderer has created it.
    pub keyboard: Option<LookInfo>,
    pub bar: Option<LookInfo>,
    /// This user can write the keyboard's hidraw node.
    pub keyboard_writable: bool,
    /// The bar's zone node is there.
    pub bar_node: bool,
    /// The app's idle has the keyboard blanked.
    pub keyboard_blanked: bool,
    /// The app's idle has the bar blanked.
    pub bar_blanked: bool,
    /// The machine is suspending or resuming: from a resume until its restore has run.
    pub suspended: bool,
    pub on_battery: bool,
    /// "Pause animations on battery" (`AppConfig::pause_animations_on_battery`).
    pub pause_on_battery: bool,
    /// GTK's `gtk-interface-reduced-motion` asks for less motion.
    pub reduced_motion: bool,
    /// The user pressed Pause.
    pub user_paused: bool,
}

impl State {
    /// Looks hold their current frame: the user paused them, or the machine is on battery
    /// with "Pause animations on battery" on.
    pub fn holds(&self) -> bool {
        self.user_paused || self.paused_on_battery()
    }

    /// The battery is what holds the looks (the page says "Paused on battery").
    pub fn paused_on_battery(&self) -> bool {
        self.on_battery && self.pause_on_battery
    }
}

/// What the renderer does on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardPlan {
    /// Writes nothing.
    None,
    /// Shows the current frame once, then writes nothing.
    Upload,
    /// Writes changed frames at this many a second.
    Stream(u32),
    /// Shows the picture once, then sends changed brightness commits at the keyboard rate.
    BrightnessOnly,
}

/// What the renderer does on the light bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarPlan {
    /// Writes nothing.
    None,
    /// Shows the current frame once, then writes nothing.
    Write,
    /// Writes changed frames at this many a second.
    Stream(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub keyboard: KeyboardPlan,
    pub bar: BarPlan,
    /// Seconds of the looks' time per second of real time.
    pub rate: f64,
}

impl Plan {
    /// Some light's look moves: its effect is stepped.
    #[cfg(test)]
    pub fn steps(&self) -> bool {
        self.keyboard_steps() || self.bar_steps()
    }

    pub fn keyboard_steps(&self) -> bool {
        matches!(
            self.keyboard,
            KeyboardPlan::Stream(_) | KeyboardPlan::BrightnessOnly
        )
    }

    pub fn bar_steps(&self) -> bool {
        matches!(self.bar, BarPlan::Stream(_))
    }
}

/// How one light is drawn, before the light's own plan type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    Nothing,
    Once,
    Brightness,
    Frames,
}

fn way(look: Option<LookInfo>, writable: bool, blanked: bool, s: &State) -> Way {
    let Some(look) = look else {
        return Way::Nothing;
    };
    if !writable || s.suspended || blanked {
        return Way::Nothing;
    }
    if s.holds() {
        return Way::Once;
    }
    match look.motion {
        Motion::Still => Way::Once,
        Motion::Breathing => Way::Brightness,
        Motion::Animated => Way::Frames,
    }
}

/// The plan for `s` (the table in the module doc).
pub fn schedule(s: &State) -> Plan {
    let keyboard = match way(s.keyboard, s.keyboard_writable, s.keyboard_blanked, s) {
        Way::Nothing => KeyboardPlan::None,
        Way::Once => KeyboardPlan::Upload,
        Way::Brightness => KeyboardPlan::BrightnessOnly,
        Way::Frames => KeyboardPlan::Stream(KEYBOARD_FPS),
    };
    let bar = match way(s.bar, s.bar_node, s.bar_blanked, s) {
        Way::Nothing => BarPlan::None,
        Way::Once => BarPlan::Write,
        Way::Brightness | Way::Frames => BarPlan::Stream(BAR_FPS),
    };
    Plan {
        keyboard,
        bar,
        rate: if s.reduced_motion {
            REDUCED_MOTION_RATE
        } else {
            1.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STILL: LookInfo = LookInfo::new(Motion::Still);
    const BREATHING: LookInfo = LookInfo::new(Motion::Breathing);
    const ANIMATED: LookInfo = LookInfo::new(Motion::Animated);

    /// Both lights drawable, plugged in, nothing paused.
    fn ready(keyboard: Option<LookInfo>, bar: Option<LookInfo>) -> State {
        State {
            keyboard,
            bar,
            keyboard_writable: true,
            bar_node: true,
            pause_on_battery: true,
            ..State::default()
        }
    }

    fn plans(s: &State) -> (KeyboardPlan, BarPlan) {
        let plan = schedule(s);
        (plan.keyboard, plan.bar)
    }

    #[test]
    fn row_1_no_look_draws_nothing() {
        let s = ready(None, None);
        assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::None));
        assert!(!schedule(&s).steps());
    }

    #[test]
    fn row_2_a_light_that_cannot_be_written_draws_nothing() {
        let mut s = ready(Some(ANIMATED), Some(ANIMATED));
        s.keyboard_writable = false;
        assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::Stream(BAR_FPS)));
        s.keyboard_writable = true;
        s.bar_node = false;
        assert_eq!(
            plans(&s),
            (KeyboardPlan::Stream(KEYBOARD_FPS), BarPlan::None)
        );
    }

    #[test]
    fn row_3_nothing_is_written_while_suspending_or_resuming() {
        for look in [STILL, BREATHING, ANIMATED] {
            let mut s = ready(Some(look), Some(look));
            s.suspended = true;
            assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::None), "{look:?}");
            // Suspended outranks a pause: a held frame is not written either.
            s.user_paused = true;
            assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::None), "{look:?}");
        }
    }

    #[test]
    fn row_4_a_blanked_light_is_not_written_and_the_other_goes_on() {
        let mut s = ready(Some(ANIMATED), Some(ANIMATED));
        s.keyboard_blanked = true;
        assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::Stream(BAR_FPS)));
        s.bar_blanked = true;
        assert_eq!(plans(&s), (KeyboardPlan::None, BarPlan::None));
        s.keyboard_blanked = false;
        assert_eq!(
            plans(&s),
            (KeyboardPlan::Stream(KEYBOARD_FPS), BarPlan::None)
        );
        // Blanked outranks a pause too: the idle blank must stay dark.
        s.user_paused = true;
        assert_eq!(plans(&s), (KeyboardPlan::Upload, BarPlan::None));
    }

    #[test]
    fn row_5_a_pause_holds_the_current_frame() {
        for look in [STILL, BREATHING, ANIMATED] {
            let mut s = ready(Some(look), Some(look));
            s.user_paused = true;
            assert_eq!(
                plans(&s),
                (KeyboardPlan::Upload, BarPlan::Write),
                "{look:?}"
            );
            assert!(!schedule(&s).steps());
        }
    }

    #[test]
    fn row_5_on_battery_the_looks_hold_only_with_the_setting_on() {
        let mut s = ready(Some(ANIMATED), Some(BREATHING));
        s.on_battery = true;
        assert_eq!(plans(&s), (KeyboardPlan::Upload, BarPlan::Write));
        assert!(s.paused_on_battery());
        s.pause_on_battery = false;
        assert_eq!(
            plans(&s),
            (KeyboardPlan::Stream(KEYBOARD_FPS), BarPlan::Stream(BAR_FPS))
        );
        assert!(!s.paused_on_battery());
        // The setting alone does nothing on mains.
        s.on_battery = false;
        s.pause_on_battery = true;
        assert_eq!(
            plans(&s),
            (KeyboardPlan::Stream(KEYBOARD_FPS), BarPlan::Stream(BAR_FPS))
        );
    }

    #[test]
    fn row_6_a_still_look_is_written_once() {
        let s = ready(Some(STILL), Some(STILL));
        assert_eq!(plans(&s), (KeyboardPlan::Upload, BarPlan::Write));
        assert!(!schedule(&s).steps());
    }

    #[test]
    fn row_7_a_breathing_picture_sends_brightness_on_the_keyboard_and_streams_on_the_bar() {
        let s = ready(Some(BREATHING), Some(BREATHING));
        assert_eq!(
            plans(&s),
            (KeyboardPlan::BrightnessOnly, BarPlan::Stream(BAR_FPS))
        );
        assert!(schedule(&s).keyboard_steps() && schedule(&s).bar_steps());
    }

    #[test]
    fn row_8_everything_else_streams_at_30_and_5() {
        let s = ready(Some(ANIMATED), Some(ANIMATED));
        assert_eq!(plans(&s), (KeyboardPlan::Stream(30), BarPlan::Stream(5)));
        assert_eq!(schedule(&s).rate, 1.0);
    }

    #[test]
    fn reduced_motion_halves_the_speed_and_changes_no_row() {
        for look in [None, Some(STILL), Some(BREATHING), Some(ANIMATED)] {
            let mut s = ready(look, look);
            let normal = schedule(&s);
            s.reduced_motion = true;
            let reduced = schedule(&s);
            assert_eq!(
                (reduced.keyboard, reduced.bar),
                (normal.keyboard, normal.bar),
                "{look:?}"
            );
            assert_eq!(reduced.rate, REDUCED_MOTION_RATE);
        }
    }

    /// Every combination of the inputs, held to the table: no combination of the switches
    /// gives a plan the table does not.
    #[test]
    fn every_combination_follows_the_table() {
        let looks = [None, Some(STILL), Some(BREATHING), Some(ANIMATED)];
        for keyboard in looks {
            for bar in looks {
                for bits in 0u32..(1 << 9) {
                    let bit = |i: u32| bits & (1 << i) != 0;
                    let s = State {
                        keyboard,
                        bar,
                        keyboard_writable: bit(0),
                        bar_node: bit(1),
                        keyboard_blanked: bit(2),
                        bar_blanked: bit(3),
                        suspended: bit(4),
                        on_battery: bit(5),
                        pause_on_battery: bit(6),
                        reduced_motion: bit(7),
                        user_paused: bit(8),
                    };
                    let plan = schedule(&s);
                    let held = s.user_paused || (s.on_battery && s.pause_on_battery);
                    let expect = |look: Option<LookInfo>, writable: bool, blanked: bool| match look
                    {
                        None => 0,
                        Some(_) if !writable || s.suspended || blanked => 0,
                        Some(_) if held => 1,
                        Some(l) => match l.motion {
                            Motion::Still => 1,
                            Motion::Breathing => 2,
                            Motion::Animated => 3,
                        },
                    };
                    let kb = match plan.keyboard {
                        KeyboardPlan::None => 0,
                        KeyboardPlan::Upload => 1,
                        KeyboardPlan::BrightnessOnly => 2,
                        KeyboardPlan::Stream(fps) => {
                            assert_eq!(fps, KEYBOARD_FPS);
                            3
                        }
                    };
                    let br = match plan.bar {
                        BarPlan::None => 0,
                        BarPlan::Write => 1,
                        BarPlan::Stream(fps) => {
                            assert_eq!(fps, BAR_FPS);
                            3
                        }
                    };
                    assert_eq!(
                        kb,
                        expect(keyboard, s.keyboard_writable, s.keyboard_blanked),
                        "{s:?}"
                    );
                    // The bar streams a breathing picture.
                    let bar_expect = match expect(bar, s.bar_node, s.bar_blanked) {
                        2 => 3,
                        other => other,
                    };
                    assert_eq!(br, bar_expect, "{s:?}");
                    let rate = if s.reduced_motion { 0.5 } else { 1.0 };
                    assert_eq!(plan.rate, rate);
                }
            }
        }
    }
}
