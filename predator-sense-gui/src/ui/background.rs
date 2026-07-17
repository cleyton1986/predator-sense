//! Small bridge for blocking work that must not run on GTK's main thread.

use gtk4::glib;
use std::cell::RefCell;
use std::sync::mpsc;
use std::time::Duration;

/// Runs `work` on a worker thread and delivers its result on the GTK thread.
///
/// GTK objects are not `Send`, so `on_done` is registered locally up front
/// and may safely capture widgets. The worker sends only the `Send` result.
pub fn run<T, F, D>(work: F, on_done: D)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let (tx, rx) = mpsc::channel::<T>();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });

    let on_done = RefCell::new(Some(on_done));
    glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
        Ok(result) => {
            if let Some(deliver) = on_done.borrow_mut().take() {
                deliver(result);
            }
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });
}
