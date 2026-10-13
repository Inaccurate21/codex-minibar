//! Where Explorer's own taskbar buttons are, read through UI Automation.
//!
//! Windows 11 does not reserve taskbar space for anyone else, so the strip
//! measures the free span between the app buttons and its anchor, compacts
//! widgets that do not fit and then drops those that still do not, the way
//! the notification area overflows. UI
//! Automation calls into Explorer synchronously; they run on their own MTA
//! thread because the host thread shares Explorer's input queue and must
//! never wait on it.

use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use windows::Win32::{
    Foundation::HWND,
    System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx},
    UI::Accessibility::{
        CUIAutomation, IUIAutomation, TreeScope_Descendants, UIA_AutomationIdPropertyId,
        UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
    },
};

/// Screen rectangle: left, top, right, bottom.
pub(super) type Bounds = (i32, i32, i32, i32);

/// Explorer's buttons, in screen pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Obstacles {
    /// Bounds of Start, Search, Task View and the app buttons together.
    pub(super) cluster: Option<Bounds>,
    /// The Windows widgets button at the leading edge.
    pub(super) widgets: Option<Bounds>,
}

fn union(a: Option<Bounds>, b: Bounds) -> Bounds {
    match a {
        Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        None => b,
    }
}

const INTERVAL: Duration = Duration::from_secs(2);

static LATEST: Mutex<Option<Obstacles>> = Mutex::new(None);
static WANTED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

/// Starts or pauses the measurements; the worker starts on first use.
pub(super) fn set_wanted(wanted: bool) {
    WANTED.store(wanted, Ordering::SeqCst);
    if !wanted {
        if let Ok(mut latest) = LATEST.lock() {
            *latest = None;
        }
        return;
    }
    if !STARTED.swap(true, Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("taskbar-obstacles".into())
            .spawn(run);
        if spawned.is_err() {
            STARTED.store(false, Ordering::SeqCst);
        }
    }
}

pub(super) fn latest() -> Option<Obstacles> {
    LATEST.lock().ok().and_then(|latest| *latest)
}

fn run() {
    let automation: Option<IUIAutomation> = unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
    };
    let Some(automation) = automation else {
        return;
    };
    loop {
        if WANTED.load(Ordering::SeqCst) {
            let measured = measure(&automation);
            if let Ok(mut latest) = LATEST.lock() {
                *latest = measured;
            }
        }
        std::thread::sleep(INTERVAL);
    }
}

fn measure(automation: &IUIAutomation) -> Option<Obstacles> {
    unsafe {
        let class = "Shell_TrayWnd\0".encode_utf16().collect::<Vec<_>>();
        let taskbar = windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(
            class.as_ptr(),
            std::ptr::null(),
        );
        if taskbar.is_null() {
            return None;
        }
        let taskbar = HWND(taskbar);
        let cache = automation.CreateCacheRequest().ok()?;
        for property in [
            UIA_ClassNamePropertyId,
            UIA_AutomationIdPropertyId,
            UIA_BoundingRectanglePropertyId,
        ] {
            cache.AddProperty(property).ok().ok()?;
        }
        let root = automation.ElementFromHandle(taskbar).ok()?;
        let condition = automation.CreateTrueCondition().ok()?;
        let elements = root
            .FindAllBuildCache(TreeScope_Descendants, &condition, &cache)
            .ok()?;
        let mut obstacles = Obstacles::default();
        for index in 0..elements.Length().unwrap_or(0) {
            let Ok(element) = elements.GetElement(index) else {
                continue;
            };
            let class = String::from_utf16_lossy(&element.CachedClassName().unwrap_or_default());
            let id = String::from_utf16_lossy(&element.CachedAutomationId().unwrap_or_default());
            let Ok(rect) = element.CachedBoundingRectangle() else {
                continue;
            };
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue;
            }
            let bounds = (rect.left, rect.top, rect.right, rect.bottom);
            match classify(&class, &id) {
                Some(Kind::Widgets) => obstacles.widgets = Some(union(obstacles.widgets, bounds)),
                Some(Kind::Cluster) => obstacles.cluster = Some(union(obstacles.cluster, bounds)),
                None => {}
            }
        }
        Some(obstacles)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Widgets,
    Cluster,
}

fn classify(class: &str, id: &str) -> Option<Kind> {
    if id == "WidgetsButton" {
        return Some(Kind::Widgets);
    }
    let cluster = class == "Taskbar.TaskListButtonAutomationPeer"
        || class == "ToggleButton"
        || matches!(
            id,
            "StartButton" | "SearchButton" | "TaskViewButton" | "SearchBoxButton"
        );
    cluster.then_some(Kind::Cluster)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_buttons_are_classified() {
        assert_eq!(
            classify("Taskbar.TaskListButtonAutomationPeer", ""),
            Some(Kind::Cluster)
        );
        assert_eq!(classify("ToggleButton", "StartButton"), Some(Kind::Cluster));
        assert_eq!(
            classify("ToggleButton", "WidgetsButton"),
            Some(Kind::Widgets)
        );
        assert_eq!(classify("SystemTray.NormalButton", ""), None);
        assert_eq!(classify("Taskbar.TaskbarFrameAutomationPeer", ""), None);
    }
}
