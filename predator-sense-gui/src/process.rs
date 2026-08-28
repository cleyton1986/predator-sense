//! Starting other processes without leaving them behind.

use std::process::Command;

/// Spawns a process and reaps it when it exits.
///
/// A `Child` dropped without being waited on stays in the process table as a
/// zombie until its parent exits, and this process is a GUI that stays open for
/// hours - one per tray start, one per temperature notification.
///
/// The `wait` goes on a thread of its own: this is called from the GTK main
/// thread, which cannot block on a child that outlives the call.
///
/// Not `signal(SIGCHLD, SIG_IGN)`, which would reap everything for free but
/// makes `Command::status()` and `Command::output()` fail with `ECHILD` instead
/// of returning the exit status - and the helper calls all over this crate
/// depend on reading it.
pub fn spawn_reaped(command: &mut Command) -> std::io::Result<u32> {
    let child = command.spawn()?;
    let pid = child.id();
    // A thread that cannot be spawned is not worth failing over: the process is
    // already running, and all that is lost is the zombie this avoids.
    let _ = std::thread::Builder::new()
        .name(format!("reap-{pid}"))
        .spawn(move || {
            let mut child = child;
            let _ = child.wait();
        });
    Ok(pid)
}
