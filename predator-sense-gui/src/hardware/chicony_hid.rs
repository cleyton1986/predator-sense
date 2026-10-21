//! The PH16-71's Chicony keyboard (04F2:0117) through the hidraw node of its
//! lighting interface, opened once and kept open.
//!
//! That interface is the one whose report descriptor starts `06 02 ff`
//! (vendor page 0xFF02, USB interface 3). The installer's udev rule gives the
//! user of the active local session read and write on that node (uaccess) and
//! on no other, so the GUI can drive the keyboard without the privileged
//! helper and its authentication dialog. Without that rule the user cannot
//! open it; `keyboard_rgb` then falls back to the helper, which opens the same
//! node as root.
//!
//! Finding the node and the transfers on it (each report behind a 0, as
//! hidapi sends them) are the `predator_sense_hidraw` crate's, shared with
//! the helper; what goes in the reports is
//! `predator_sense_protocol::keyboard`'s. What is the GUI's own is keeping the
//! node open and coming back to it, below.
//!
//! The node is recreated when the device re-enumerates (a USB reset, or
//! usbhid unbound and bound again), and the old descriptor then answers
//! `ENODEV`. A transfer that meets that finds the node again, opens it and
//! runs once more, whole, so a sequence of reports never lands half on the
//! old node and half on the new one.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use predator_sense_hidraw::HidrawDevice;
use predator_sense_protocol::keyboard::FEATURE_LEN;

/// How long after a failed open the node is not looked for again. For a
/// node that never opened, [`Keyboard::is_writable`] answers false that
/// long: short enough that a node made writable while the app runs (the
/// udev rule installed or triggered) is picked up within seconds, long
/// enough that asking before every write costs nothing. For one that has
/// opened, calls fail at once that long instead of scanning sysfs each time.
pub const RECHECK_AFTER: Duration = Duration::from_secs(5);

/// One open node: the transfer the controller takes.
pub trait Device {
    /// One 8-byte feature report.
    fn send_feature(&mut self, report: &[u8; FEATURE_LEN]) -> io::Result<()>;
}

/// Finds and opens the node. The real one is [`Hidraw`]; tests use a fake.
pub trait Node {
    type Device: Device;
    fn open(&self) -> io::Result<Self::Device>;
}

/// The real node: found under the hidraw class in sysfs, opened under /dev.
#[derive(Debug, Clone, Copy)]
pub struct Hidraw {
    class: &'static str,
    dev: &'static str,
}

impl Hidraw {
    pub const SYSTEM: Self = Self {
        class: predator_sense_hidraw::SYSFS_CLASS,
        dev: predator_sense_hidraw::DEV_DIR,
    };

    /// Where the lighting interface's node is now, if the keyboard is there
    /// ([`predator_sense_hidraw::find_in`]).
    pub fn find(&self) -> Option<PathBuf> {
        predator_sense_hidraw::find_in(Path::new(self.class), Path::new(self.dev))
    }
}

impl Node for Hidraw {
    type Device = HidrawDevice;

    fn open(&self) -> io::Result<HidrawDevice> {
        // The machine running the tests may well have this keyboard, and a
        // user in `input`: no test may reach it.
        if cfg!(test) {
            return Err(io::Error::other("tests never open a real hidraw node"));
        }
        let path = self.find().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Chicony keyboard lighting interface (04f2:0117, 06 02 ff) not found",
            )
        })?;
        HidrawDevice::open_found(&path, Path::new(self.class))
    }
}

impl Device for HidrawDevice {
    fn send_feature(&mut self, report: &[u8; FEATURE_LEN]) -> io::Result<()> {
        HidrawDevice::send_feature(self, report)
    }
}

/// Whether `error` says the node has gone.
fn is_gone(error: &io::Error) -> bool {
    error.raw_os_error() == Some(libc::ENODEV)
}

/// The keyboard, opened on first use and kept open.
pub struct Keyboard<N: Node> {
    node: N,
    device: Option<N::Device>,
    /// The node has opened at least once in this process.
    opened_before: bool,
    /// When the last open failed, and how. `None` once one succeeds.
    open_failed: Option<(Instant, String)>,
}

impl<N: Node> Keyboard<N> {
    pub const fn new(node: N) -> Self {
        Self {
            node,
            device: None,
            opened_before: false,
            open_failed: None,
        }
    }

    /// Whether this process writes the keyboard through its node.
    ///
    /// True once the node has opened in this process, whatever happens to it
    /// after: a node that goes away and comes back (a re-enumeration) is
    /// still this user's, and an open that fails while it comes back (before
    /// udev has given this user access, say) is the call's error to report, not
    /// a reason to send the call to the helper, whose session can bring up an
    /// authentication dialog. Before that, true when the node opens now; a
    /// refused open is not tried again for [`RECHECK_AFTER`].
    pub fn is_writable(&mut self) -> bool {
        self.is_writable_at(Instant::now())
    }

    /// [`Self::is_writable`] at `now`.
    pub fn is_writable_at(&mut self, now: Instant) -> bool {
        if self.device.is_some() || self.opened_before {
            return true;
        }
        !self.refused_lately(now) && self.open_at(now).is_ok()
    }

    /// An open failed less than [`RECHECK_AFTER`] before `now`.
    fn refused_lately(&self, now: Instant) -> bool {
        self.open_failed
            .as_ref()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) < RECHECK_AFTER)
    }

    /// Finds the node and opens it, closing any node open before. Reached
    /// through [`Self::is_writable_at`] and the transfers, never on its own,
    /// so the back-off always applies.
    fn open_at(&mut self, now: Instant) -> io::Result<()> {
        self.device = None;
        match self.node.open() {
            Ok(device) => {
                self.device = Some(device);
                self.opened_before = true;
                self.open_failed = None;
                Ok(())
            }
            Err(error) => {
                self.open_failed = Some((now, error.to_string()));
                Err(error)
            }
        }
    }

    /// Opens the node for a transfer, unless an open failed within
    /// [`RECHECK_AFTER`]: the transfer then fails at once with that failure,
    /// so a node that is gone is not looked for on every call.
    fn open_for_transfer(&mut self, now: Instant) -> io::Result<()> {
        if self.refused_lately(now) {
            let reason = self.open_failed.as_ref().map_or("", |(_, error)| error);
            return Err(io::Error::other(format!(
                "keyboard node not open, the last try failed: {reason}"
            )));
        }
        self.open_at(now)
    }

    /// One try at `transfer`, opening the node first if none is open.
    fn attempt<T>(
        &mut self,
        transfer: &mut impl FnMut(&mut N::Device) -> io::Result<T>,
    ) -> io::Result<T> {
        if self.device.is_none() {
            self.open_for_transfer(Instant::now())?;
        }
        let Some(device) = self.device.as_mut() else {
            return Err(io::Error::other("keyboard node not open"));
        };
        let result = transfer(device);
        if result.is_err() {
            self.device = None;
        }
        result
    }

    /// Runs `transfer` on the open node, opening it first if none is. When
    /// the node has gone (`ENODEV`), finds and opens it again and runs the
    /// whole of `transfer` once more. Any failure closes the node, so the
    /// next call finds and opens it afresh.
    pub fn run<T>(
        &mut self,
        mut transfer: impl FnMut(&mut N::Device) -> io::Result<T>,
    ) -> io::Result<T> {
        match self.attempt(&mut transfer) {
            Err(error) if is_gone(&error) => self.attempt(&mut transfer),
            other => other,
        }
    }

    /// One feature report.
    pub fn send_feature(&mut self, report: &[u8; FEATURE_LEN]) -> io::Result<()> {
        self.run(|device| device.send_feature(report))
    }

    /// Feature reports in order, as one transfer.
    pub fn send_features(&mut self, reports: &[[u8; FEATURE_LEN]]) -> io::Result<()> {
        self.run(|device| {
            reports
                .iter()
                .try_for_each(|report| device.send_feature(report))
        })
    }
}

/// A scripted keyboard for tests, shared with `keyboard_rgb`'s.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// One transfer that reached a fake node, with the number of the open
    /// that node came from (1 for the first).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Sent {
        Feature(u32, [u8; FEATURE_LEN]),
    }

    #[derive(Debug, Default)]
    pub struct Script {
        pub sent: Vec<Sent>,
        pub opens: u32,
        /// Every open fails with `EACCES`: a user the node is not given to.
        pub locked: bool,
        /// errno for the next opens, in order, before `locked` is looked at.
        pub open_errors: VecDeque<i32>,
        /// errno for the next transfers, in order; 0 lets one through.
        pub transfer_errors: VecDeque<i32>,
    }

    #[derive(Debug, Clone, Default)]
    pub struct FakeNode(pub Rc<RefCell<Script>>);

    #[derive(Debug)]
    pub struct FakeDevice {
        open: u32,
        script: Rc<RefCell<Script>>,
    }

    impl FakeNode {
        pub fn locked() -> Self {
            let node = Self::default();
            node.0.borrow_mut().locked = true;
            node
        }

        pub fn sent(&self) -> Vec<Sent> {
            self.0.borrow().sent.clone()
        }

        pub fn opens(&self) -> u32 {
            self.0.borrow().opens
        }

        pub fn fail_transfers(&self, errnos: &[i32]) {
            self.0.borrow_mut().transfer_errors.extend(errnos);
        }
    }

    impl Node for FakeNode {
        type Device = FakeDevice;

        fn open(&self) -> io::Result<FakeDevice> {
            let mut script = self.0.borrow_mut();
            script.opens += 1;
            if let Some(errno) = script.open_errors.pop_front() {
                return Err(io::Error::from_raw_os_error(errno));
            }
            if script.locked {
                return Err(io::Error::from_raw_os_error(libc::EACCES));
            }
            Ok(FakeDevice {
                open: script.opens,
                script: Rc::clone(&self.0),
            })
        }
    }

    impl FakeDevice {
        fn transfer(&self, sent: Sent) -> io::Result<()> {
            let mut script = self.script.borrow_mut();
            match script.transfer_errors.pop_front() {
                Some(errno) if errno != 0 => Err(io::Error::from_raw_os_error(errno)),
                _ => {
                    script.sent.push(sent);
                    Ok(())
                }
            }
        }
    }

    impl Device for FakeDevice {
        fn send_feature(&mut self, report: &[u8; FEATURE_LEN]) -> io::Result<()> {
            self.transfer(Sent::Feature(self.open, *report))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeNode, Sent};
    use super::*;

    const A: [u8; 8] = [1, 0, 0, 0, 0, 0, 0, 0xfe];
    const B: [u8; 8] = [2, 0, 0, 0, 0, 0, 0, 0xfd];
    const C: [u8; 8] = [3, 0, 0, 0, 0, 0, 0, 0xfc];

    fn keyboard() -> (Keyboard<FakeNode>, FakeNode) {
        let node = FakeNode::default();
        (Keyboard::new(node.clone()), node)
    }

    fn errno(error: &io::Error) -> Option<i32> {
        error.raw_os_error()
    }

    #[test]
    fn the_node_is_opened_once_and_kept_open() {
        let (mut keyboard, node) = keyboard();
        for report in [A, B, C] {
            keyboard.send_feature(&report).unwrap();
        }
        assert_eq!(node.opens(), 1);
        assert_eq!(
            node.sent(),
            [
                Sent::Feature(1, A),
                Sent::Feature(1, B),
                Sent::Feature(1, C)
            ]
        );
    }

    #[test]
    fn a_node_that_has_gone_is_found_again_and_the_transfer_repeated() {
        let (mut keyboard, node) = keyboard();
        node.fail_transfers(&[libc::ENODEV]);
        keyboard.send_feature(&A).unwrap();
        assert_eq!(node.opens(), 2);
        assert_eq!(node.sent(), [Sent::Feature(2, A)]);
    }

    #[test]
    fn a_node_that_has_gone_is_reopened_only_once() {
        let (mut keyboard, node) = keyboard();
        node.fail_transfers(&[libc::ENODEV, libc::ENODEV]);
        let error = keyboard.send_feature(&A).unwrap_err();
        assert_eq!(errno(&error), Some(libc::ENODEV));
        assert_eq!(node.opens(), 2);
        assert!(node.sent().is_empty());
    }

    #[test]
    fn a_reopened_transfer_runs_whole_on_the_new_node() {
        // Two of three reports reached the old node; all three go again, so
        // the new one never gets the tail of a sequence on its own.
        let (mut keyboard, node) = keyboard();
        node.fail_transfers(&[0, libc::ENODEV]);
        keyboard.send_features(&[A, B, C]).unwrap();
        assert_eq!(
            node.sent(),
            [
                Sent::Feature(1, A),
                Sent::Feature(2, A),
                Sent::Feature(2, B),
                Sent::Feature(2, C),
            ]
        );
    }

    #[test]
    fn other_failures_are_not_repeated_but_the_next_call_reopens() {
        let (mut keyboard, node) = keyboard();
        node.fail_transfers(&[libc::EIO]);
        let error = keyboard.send_feature(&A).unwrap_err();
        assert_eq!(errno(&error), Some(libc::EIO));
        assert_eq!(node.opens(), 1);
        keyboard.send_feature(&B).unwrap();
        assert_eq!(node.opens(), 2, "a failure closes the node");
        assert_eq!(node.sent(), [Sent::Feature(2, B)]);
    }

    #[test]
    fn a_node_that_cannot_be_opened_fails_the_transfer() {
        let node = FakeNode::locked();
        let mut keyboard = Keyboard::new(node.clone());
        let error = keyboard.send_feature(&A).unwrap_err();
        assert_eq!(errno(&error), Some(libc::EACCES));
        assert!(node.sent().is_empty());
    }

    #[test]
    fn a_writable_node_is_opened_by_asking_and_kept() {
        let (mut keyboard, node) = keyboard();
        let now = Instant::now();
        assert!(keyboard.is_writable_at(now));
        assert!(keyboard.is_writable_at(now));
        keyboard.send_feature(&A).unwrap();
        assert_eq!(node.opens(), 1);
        assert_eq!(node.sent(), [Sent::Feature(1, A)]);
    }

    #[test]
    fn a_refused_open_is_tried_again_only_after_a_few_seconds() {
        let node = FakeNode::locked();
        let mut keyboard = Keyboard::new(node.clone());
        let t0 = Instant::now();
        assert!(!keyboard.is_writable_at(t0));
        assert!(!keyboard.is_writable_at(t0 + Duration::from_secs(4)));
        assert_eq!(node.opens(), 1, "not asked again yet");
        assert!(!keyboard.is_writable_at(t0 + RECHECK_AFTER));
        assert_eq!(node.opens(), 2);
        // The udev rule installed or triggered while the app runs.
        node.0.borrow_mut().locked = false;
        assert!(!keyboard.is_writable_at(t0 + RECHECK_AFTER + Duration::from_secs(1)));
        assert!(keyboard.is_writable_at(t0 + RECHECK_AFTER * 2));
        assert_eq!(node.opens(), 3);
    }

    #[test]
    fn a_node_closed_by_a_failure_is_opened_again_by_the_next_transfer() {
        let (mut keyboard, node) = keyboard();
        let now = Instant::now();
        assert!(keyboard.is_writable_at(now));
        node.fail_transfers(&[libc::EIO]);
        assert!(keyboard.send_feature(&A).is_err());
        // Still the node's call, at once, with no back-off: only a failed
        // open starts one.
        assert!(keyboard.is_writable_at(now));
        keyboard.send_feature(&B).unwrap();
        assert_eq!(node.opens(), 2);
        assert_eq!(node.sent(), [Sent::Feature(2, B)]);
    }

    /// A keyboard whose node has opened, gone (ENODEV) and failed to open
    /// again with `errno`.
    fn reopen_failed(errno: i32) -> (Keyboard<FakeNode>, FakeNode) {
        let (mut keyboard, node) = keyboard();
        assert!(keyboard.is_writable());
        node.fail_transfers(&[libc::ENODEV]);
        node.0.borrow_mut().open_errors.push_back(errno);
        let error = keyboard.send_feature(&A).unwrap_err();
        assert_eq!(errno_of(&error), Some(errno));
        assert_eq!(node.opens(), 2);
        (keyboard, node)
    }

    fn errno_of(error: &io::Error) -> Option<i32> {
        error.raw_os_error()
    }

    #[test]
    fn a_node_that_opened_once_stays_writable_when_a_reopen_fails() {
        // The node coming back after a re-enumeration, before udev gave this
        // user access: still this user's node, not a case for the helper.
        for errno in [libc::EACCES, libc::ENOENT, libc::ENODEV] {
            let (mut keyboard, _node) = reopen_failed(errno);
            assert!(keyboard.is_writable(), "{errno}");
            assert!(keyboard.is_writable_at(Instant::now() + RECHECK_AFTER * 10));
        }
    }

    #[test]
    fn a_failed_reopen_is_reported_and_not_retried_within_the_back_off() {
        let (mut keyboard, node) = reopen_failed(libc::EACCES);
        let error = keyboard.send_feature(&B).unwrap_err();
        assert!(error.to_string().contains("not open"), "{error}");
        assert_eq!(node.opens(), 2, "no open tried within the back-off");
        assert!(node.sent().is_empty());
    }

    #[test]
    fn a_failed_reopen_is_retried_after_the_back_off() {
        let (mut keyboard, node) = reopen_failed(libc::EACCES);
        // As if RECHECK_AFTER had passed since the failure.
        if let Some((at, _)) = keyboard.open_failed.as_mut() {
            *at -= RECHECK_AFTER;
        }
        keyboard.send_feature(&B).unwrap();
        assert_eq!(node.opens(), 3);
        assert_eq!(node.sent(), [Sent::Feature(3, B)]);
    }

    #[test]
    fn a_node_that_never_opened_is_not_writable() {
        // The helper's case: this process could never open it.
        let node = FakeNode::locked();
        let mut keyboard = Keyboard::new(node.clone());
        assert!(!keyboard.is_writable());
        assert!(!keyboard.is_writable_at(Instant::now() + RECHECK_AFTER * 10));
    }

    #[test]
    fn only_enodev_counts_as_the_node_gone() {
        assert!(is_gone(&io::Error::from_raw_os_error(libc::ENODEV)));
        for errno in [libc::EIO, libc::EPIPE, libc::EACCES, libc::ENOENT] {
            assert!(!is_gone(&io::Error::from_raw_os_error(errno)), "{errno}");
        }
        assert!(!is_gone(&io::Error::other("short write")));
    }

    #[test]
    fn tests_never_open_a_real_node() {
        // The guard's own refusal, not whatever the real node would answer:
        // a machine with the udev rule installed would open it.
        let error = Hidraw::SYSTEM.open().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "tests never open a real hidraw node");
    }
}
