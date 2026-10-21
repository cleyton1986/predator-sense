//! Small bridge for blocking work that must not run on GTK's main thread.

use gtk4::glib;

/// Runs `work` on a thread of its own and delivers its result on the GTK
/// thread.
///
/// GTK objects are not `Send`, so `on_done` stays on the GTK thread and may
/// safely capture widgets. The worker sends only the `Send` result. If `work`
/// panics, `on_done` is not called.
///
/// The result arrives as a main-loop source once it is ready: one wakeup.
/// This used to poll for it every 50 ms, twenty wakeups a second for as long
/// as any job ran. A thread per call rather than a shared pool, because most
/// callers can wait for minutes (a helper call behind an authentication
/// dialog, a switch behind queued ones, a network request), and a few of
/// those would hold up everything else on a small pool, GTK's own loaders
/// included.
pub fn run<T, F, D>(work: F, on_done: D)
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    // Only ever touched again on the GTK thread, inside the source below.
    let on_done = glib::thread_guard::ThreadGuard::new(on_done);
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        // A timeout source on the default main context, which only the GTK
        // thread dispatches (`MainContext::invoke` could run it here).
        glib::timeout_add_once(std::time::Duration::ZERO, move || {
            let on_done = on_done.into_inner();
            if let Ok(result) = result {
                on_done(result);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    #[test]
    fn the_result_arrives_on_the_gtk_thread() {
        crate::ui::gtk_test::run("background result", result_arrives_on_the_gtk_thread);
    }

    #[test]
    fn a_panicking_job_never_calls_on_done() {
        crate::ui::gtk_test::run("background panic", || {
            let called = Rc::new(Cell::new(false));
            {
                let called = called.clone();
                super::run(|| panic!("a job that fails"), move |()| called.set(true));
            }
            let context = gtk4::glib::MainContext::default();
            let deadline = Instant::now() + Duration::from_millis(300);
            while Instant::now() < deadline {
                context.iteration(false);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(!called.get());
        });
    }

    fn result_arrives_on_the_gtk_thread() {
        {
            let gtk_thread = std::thread::current().id();
            let got = Rc::new(Cell::new(None));
            {
                let got = got.clone();
                super::run(
                    move || std::thread::current().id() != gtk_thread,
                    move |off_thread| got.set(Some((off_thread, std::thread::current().id()))),
                );
            }
            let context = gtk4::glib::MainContext::default();
            let deadline = Instant::now() + Duration::from_secs(5);
            while got.get().is_none() && Instant::now() < deadline {
                context.iteration(false);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(got.get(), Some((true, gtk_thread)));
        }
    }
}
