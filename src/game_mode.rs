//! Game mode: backs background work off while a game (or any full-screen
//! app) runs, or always when the user asks for it.
//!
//! While active, provider polls and usage scans stretch their intervals
//! ([`stretch_limit_interval`], [`stretch_usage_interval`]), the process runs
//! at below-normal priority with EcoQoS, the idle bridge tick slows down and
//! unsolicited notifications are held back. A card announces each change
//! unless `notifications.game_mode_changes` is off.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
use std::time::Duration;

use crate::settings::GameMode;

/// How often Automatic looks for a full-screen app. The query is a cheap
/// shell call, and a few seconds of delay is invisible next to a game launch.
const DETECTION_INTERVAL: Duration = Duration::from_secs(5);

/// Polls back off by this factor, never faster than the floors and never
/// slower than the ceilings below (or the configured interval, if longer),
/// so a long session still sees its quota move.
const STRETCH_FACTOR: u32 = 4;
const LIMIT_INTERVAL_FLOOR: Duration = Duration::from_secs(5 * 60);
const LIMIT_INTERVAL_CEILING: Duration = Duration::from_secs(60 * 60);
const USAGE_INTERVAL_FLOOR: Duration = Duration::from_secs(30 * 60);
const USAGE_INTERVAL_CEILING: Duration = Duration::from_secs(2 * 60 * 60);

static MODE: AtomicU8 = AtomicU8::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Set on every activation change until the bridge resends poll intervals.
static TRANSITION: AtomicBool = AtomicBool::new(false);
static DETECTOR_STARTED: AtomicBool = AtomicBool::new(false);
/// Mirrors `notifications.game_mode_changes`.
static NOTIFY: AtomicBool = AtomicBool::new(true);
/// False until the first evaluation, which reflects the state at launch
/// rather than a change, so it never shows a card.
static EVALUATED: AtomicBool = AtomicBool::new(false);
/// Serializes state changes so the process priority matches [`ACTIVE`].
static APPLY_LOCK: Mutex<()> = Mutex::new(());

fn mode() -> GameMode {
    GameMode::ALL
        .get(usize::from(MODE.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

/// Applies the settings immediately; called on startup and every commit.
pub fn apply(mode: GameMode, notify: bool) {
    NOTIFY.store(notify, Ordering::Relaxed);
    MODE.store(mode.index() as u8, Ordering::Relaxed);
    if mode == GameMode::Automatic && !DETECTOR_STARTED.swap(true, Ordering::AcqRel) {
        let spawned = std::thread::Builder::new()
            .name("game-mode-detector".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(DETECTION_INTERVAL);
                    if self::mode() == GameMode::Automatic {
                        refresh();
                    }
                }
            });
        if let Err(error) = spawned {
            DETECTOR_STARTED.store(false, Ordering::Release);
            crate::logger::info(format!("Game mode detector failed to start: {error}"));
        }
    }
    refresh();
}

/// Re-evaluates whether game mode is active and applies a change.
fn refresh() {
    let active = match mode() {
        GameMode::Off => false,
        GameMode::Always => true,
        GameMode::Automatic => full_screen_app_running(),
    };
    let _guard = APPLY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let first = !EVALUATED.swap(true, Ordering::AcqRel);
    if ACTIVE.swap(active, Ordering::AcqRel) == active {
        return;
    }
    TRANSITION.store(true, Ordering::Release);
    set_background_priority(active);
    crate::logger::info(if active {
        "Game mode on: background work slowed down"
    } else {
        "Game mode off: background work back to normal"
    });
    if NOTIFY.load(Ordering::Relaxed) && !first {
        crate::notifications::show_game_mode_changed(active);
    }
}

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// True once after each activation change; the bridge then resends the
/// stretched (or restored) poll intervals to every worker.
pub fn take_transition() -> bool {
    TRANSITION.swap(false, Ordering::AcqRel)
}

/// The quota poll interval to use right now for a configured one.
pub fn stretch_limit_interval(configured: Duration) -> Duration {
    stretch(configured, LIMIT_INTERVAL_FLOOR, LIMIT_INTERVAL_CEILING)
}

/// The usage scan interval to use right now for a configured one.
pub fn stretch_usage_interval(configured: Duration) -> Duration {
    stretch(configured, USAGE_INTERVAL_FLOOR, USAGE_INTERVAL_CEILING)
}

fn stretch(configured: Duration, floor: Duration, ceiling: Duration) -> Duration {
    if is_active() {
        stretched(configured, floor, ceiling)
    } else {
        configured
    }
}

fn stretched(configured: Duration, floor: Duration, ceiling: Duration) -> Duration {
    configured
        .saturating_mul(STRETCH_FACTOR)
        .max(floor)
        .min(ceiling.max(configured))
}

#[cfg(windows)]
fn full_screen_app_running() -> bool {
    use windows_sys::Win32::UI::Shell::{
        QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
        SHQueryUserNotificationState,
    };
    let mut state = 0;
    let status = unsafe { SHQueryUserNotificationState(&mut state) };
    status == 0
        && matches!(
            state,
            QUNS_BUSY | QUNS_RUNNING_D3D_FULL_SCREEN | QUNS_PRESENTATION_MODE
        )
}

#[cfg(not(windows))]
fn full_screen_app_running() -> bool {
    false
}

/// Below-normal priority keeps the app off the game's cores; EcoQoS lets
/// Windows run its threads on efficient cores at low clocks.
#[cfg(windows)]
fn set_background_priority(background: bool) {
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, NORMAL_PRIORITY_CLASS,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE, ProcessPowerThrottling, SetPriorityClass,
        SetProcessInformation,
    };
    let process = unsafe { GetCurrentProcess() };
    let class = if background {
        BELOW_NORMAL_PRIORITY_CLASS
    } else {
        NORMAL_PRIORITY_CLASS
    };
    if unsafe { SetPriorityClass(process, class) } == 0 {
        crate::logger::info("Game mode could not change the process priority");
    }
    // An empty control mask hands throttling back to the system's own policy.
    let throttling = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: if background {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        },
        StateMask: if background {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        },
    };
    let applied = unsafe {
        SetProcessInformation(
            process,
            ProcessPowerThrottling,
            (&raw const throttling).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    };
    if applied == 0 {
        crate::logger::info("Game mode could not change EcoQoS");
    }
}

#[cfg(not(windows))]
fn set_background_priority(_background: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stretched_intervals_stay_between_floor_and_ceiling() {
        let limits = |minutes: u64| {
            stretched(
                Duration::from_secs(minutes * 60),
                LIMIT_INTERVAL_FLOOR,
                LIMIT_INTERVAL_CEILING,
            )
        };
        assert_eq!(limits(0), LIMIT_INTERVAL_FLOOR);
        assert_eq!(limits(10), Duration::from_secs(40 * 60));
        assert_eq!(limits(30), LIMIT_INTERVAL_CEILING);
        // A configured interval above the ceiling is never shortened.
        assert_eq!(limits(300), Duration::from_secs(300 * 60));
        assert_eq!(
            stretched(
                Duration::from_secs(5 * 60),
                USAGE_INTERVAL_FLOOR,
                USAGE_INTERVAL_CEILING
            ),
            USAGE_INTERVAL_FLOOR
        );
    }

    #[test]
    fn mode_round_trips_through_its_index() {
        for mode in GameMode::ALL {
            assert_eq!(GameMode::from_index(mode.index()), mode);
        }
    }
}
