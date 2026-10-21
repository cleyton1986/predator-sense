//! The tests that need GTK run on one thread of their own. GTK starts once per process and
//! only on the thread that started it, while the test harness runs each test on a thread of
//! its own, so a second test calling `gtk::init` would panic. Nothing is ever shown on screen:
//! the widgets are built, measured and allocated off screen.

use gtk4 as gtk;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

type Job = Box<dyn FnOnce() + Send>;
type Done = Sender<thread::Result<()>>;

/// The GTK thread's queue; `None` without a display.
fn queue() -> &'static Option<Mutex<Sender<(Job, Done)>>> {
    static QUEUE: OnceLock<Option<Mutex<Sender<(Job, Done)>>>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        let (jobs, inbox) = mpsc::channel::<(Job, Done)>();
        let (started, ready) = mpsc::channel();
        thread::spawn(move || {
            let up = gtk::init().is_ok();
            let _ = started.send(up);
            if up {
                for (job, done) in inbox {
                    let _ = done.send(panic::catch_unwind(AssertUnwindSafe(job)));
                }
            }
        });
        ready.recv().unwrap_or(false).then(|| Mutex::new(jobs))
    })
}

/// Runs `job` on the GTK thread and waits for it; a panic in it fails the calling test.
/// Without a display it says so under `name` and passes.
pub fn run(name: &str, job: impl FnOnce() + Send + 'static) {
    let Some(jobs) = queue() else {
        eprintln!("{name}: no display, so the GTK test did not run");
        return;
    };
    let (done, result) = mpsc::channel();
    jobs.lock()
        .unwrap_or_else(|e| e.into_inner())
        .send((Box::new(job), done))
        .expect("the GTK thread runs for the whole test run");
    if let Err(cause) = result.recv().expect("the GTK thread answers") {
        panic::resume_unwind(cause);
    }
}
