// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com
//! Standalone monitor-host lifecycle checks. No capture or display mutation.
//! Physical multi-display/Retina hotplug still requires an isolated native run.

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    use screenpipe_engine::macos_run_loop;
    use std::time::Duration;

    let main_thread = std::thread::current().id();
    for _ in 0..3 {
        let value = macos_run_loop::run(move || {
            assert_ne!(std::thread::current().id(), main_thread);
            // Complete after the main thread has entered the run loop so the
            // registered cross-thread completion source must actually wake it.
            std::thread::sleep(Duration::from_millis(20));
            42
        })?;
        assert_eq!(value, 42);
    }
    let outside_main = std::thread::spawn(|| macos_run_loop::run(|| 0))
        .join()
        .expect("guard check must not panic");
    assert!(outside_main.is_err(), "AppKit must remain on the main thread");

    let panic = std::panic::catch_unwind(|| {
        let _ = macos_run_loop::run(|| panic!("intentional capture-host panic"));
    });
    assert!(panic.is_err(), "worker panics must propagate after cleanup");
    // The source was removed even on panic; a subsequent invocation still works.
    assert_eq!(macos_run_loop::run(|| 7)?, 7);
    println!("native capture host: completion, repeat lifecycle, main-thread guard and panic cleanup passed; physical display topology not exercised");
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn main() {}
