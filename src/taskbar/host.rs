//! The taskbar host thread: a layered child window inside `Shell_TrayWnd`.
//!
//! A message-only controller window owns the timers and wakes on published
//! messages; the visible strip is created, re-parented after an Explorer
//! restart, and destroyed as the settings require. Window procedures only
//! borrow the host state for posted messages (mouse, timers, wake-ups), so
//! sent messages that Win32 calls generate re-enter without a borrow.

use std::{
    cell::RefCell,
    collections::HashMap,
    ptr::null_mut,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    time::{Duration, Instant},
};

use chrono::Utc;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
        CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC,
        HBITMAP, HDC, HGDIOBJ, MapWindowPoints, ReleaseDC, SelectObject,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{
            ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, DispatchMessageW, FindWindowExW, FindWindowW, GW_CHILD, GWL_STYLE,
            GetClientRect, GetCursorPos, GetMessageW, GetWindow, GetWindowRect, HMENU,
            HWND_MESSAGE, HWND_TOP, IDC_ARROW, IsWindow, KillTimer, LoadCursorW, MF_SEPARATOR,
            MF_STRING, MSG, PostMessageW, RegisterClassW, SW_SHOWNA, SWP_NOACTIVATE, SWP_NOMOVE,
            SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetParent, SetTimer, SetWindowLongPtrW,
            SetWindowPos, ShowWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
            TrackPopupMenuEx, TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WM_APP,
            WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONUP, WM_TIMER, WNDCLASSW,
            WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
};

use super::{
    Action, Config, WIDGET_GAP,
    model::{self, Appearance, WidgetView},
    obstacles::Obstacles,
    render::{CARD_MARGIN, Item, Metrics, Renderer},
};
use crate::{limits::ProviderLimits, settings::TaskbarAlignment};

pub(super) enum Message {
    Config(Config),
    Limits(ProviderLimits),
}

const WM_WAKE: u32 = WM_APP + 1;
const WM_MOUSELEAVE: u32 = 0x02A3;
const TIMER_POLL: usize = 1;
const TIMER_FRAME: usize = 2;
const TIMER_CARD: usize = 3;
/// Hover time before the card opens, like a taskbar thumbnail.
const CARD_DELAY: u32 = 450;
const CARD_GAP: f32 = 8.0;
const CARD_RISE: f32 = 8.0;
const POLL: u32 = 500;
const FRAME: u32 = 15;

const MENU_OPEN: usize = 1;
const MENU_SETTINGS: usize = 2;
const MENU_HIDE: usize = 3;

/// Space between the strip and its anchor, in DIPs.
const TRAY_MARGIN: f32 = 2.0;
const START_MARGIN: f32 = 6.0;

const HOVER_IN: Duration = Duration::from_millis(100);
const HOVER_OUT: Duration = Duration::from_millis(167);
const PRESS: Duration = Duration::from_millis(50);
const RESIZE: Duration = Duration::from_millis(250);
const FADE: Duration = Duration::from_millis(167);
const PROGRESS: Duration = Duration::from_millis(500);
const CARD_IN: Duration = Duration::from_millis(167);
const CARD_OUT: Duration = Duration::from_millis(100);

static QUEUE: Mutex<Vec<Message>> = Mutex::new(Vec::new());
static CONTROLLER: AtomicIsize = AtomicIsize::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

pub(super) fn publish(message: Message) {
    let start = matches!(&message, Message::Config(config) if config.taskbar.enabled);
    if let Ok(mut queue) = QUEUE.lock() {
        // Only the newest value of each kind matters.
        queue.retain(|queued| {
            !matches!(
                (queued, &message),
                (Message::Config(_), Message::Config(_)) | (Message::Limits(_), Message::Limits(_))
            )
        });
        queue.push(message);
    }
    if start && !STARTED.swap(true, Ordering::SeqCst) {
        let spawned = std::thread::Builder::new()
            .name("taskbar-widgets".into())
            .spawn(run);
        if let Err(error) = spawned {
            STARTED.store(false, Ordering::SeqCst);
            trace(format!("Could not start the taskbar widgets: {error}"));
        }
    }
    let controller = CONTROLLER.load(Ordering::SeqCst);
    if controller != 0 {
        unsafe { PostMessageW(controller as HWND, WM_WAKE, 0, 0) };
    }
}

/// Logs to the app log, and to stderr for the visual check example.
fn trace(message: impl AsRef<str>) {
    #[cfg(debug_assertions)]
    eprintln!("taskbar: {}", message.as_ref());
    crate::logger::info(message);
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

const CONTROLLER_CLASS: &str = "CodexMinibarTaskbarController";
const WIDGET_CLASS: &str = "CodexMinibarTaskbarWidgets";
const CARD_CLASS: &str = "CodexMinibarTaskbarCard";

fn run() {
    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let controller_class = wide(CONTROLLER_CLASS);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(controller_proc),
            hInstance: instance,
            lpszClassName: controller_class.as_ptr(),
            ..std::mem::zeroed()
        });
        let widget_class = wide(WIDGET_CLASS);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(widget_proc),
            hInstance: instance,
            lpszClassName: widget_class.as_ptr(),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..std::mem::zeroed()
        });
        let card_class = wide(CARD_CLASS);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: card_class.as_ptr(),
            ..std::mem::zeroed()
        });
        let controller = CreateWindowExW(
            0,
            controller_class.as_ptr(),
            null_mut(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            null_mut(),
        );
        if controller.is_null() {
            trace("Could not create the taskbar widget controller");
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
        HOST.with(|host| *host.borrow_mut() = Some(Host::new(controller)));
        CONTROLLER.store(controller as isize, Ordering::SeqCst);
        SetTimer(controller, TIMER_POLL, POLL, None);
        with_host(|host| host.drain());

        let mut message = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn with_host<R>(f: impl FnOnce(&mut Host) -> R) -> Option<R> {
    HOST.with(|host| {
        let mut host = host.try_borrow_mut().ok()?;
        host.as_mut().map(f)
    })
}

unsafe extern "system" fn controller_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_WAKE => {
            with_host(Host::drain);
            0
        }
        WM_TIMER if wparam == TIMER_POLL => {
            with_host(Host::poll);
            0
        }
        WM_TIMER if wparam == TIMER_FRAME => {
            with_host(Host::frame);
            0
        }
        WM_TIMER if wparam == TIMER_CARD => {
            unsafe { KillTimer(hwnd, TIMER_CARD) };
            with_host(Host::open_card);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn point_from(lparam: LPARAM) -> (i32, i32) {
    (
        (lparam & 0xFFFF) as u16 as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}

unsafe extern "system" fn widget_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_MOUSEMOVE => {
            let (x, _) = point_from(lparam);
            let track = with_host(|host| host.mouse_move(x)).unwrap_or(false);
            if track {
                let mut event = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe { TrackMouseEvent(&mut event) };
            }
            0
        }
        WM_MOUSELEAVE => {
            with_host(Host::mouse_leave);
            0
        }
        WM_LBUTTONDOWN => {
            let (x, _) = point_from(lparam);
            if with_host(|host| host.press(x)).unwrap_or(false) {
                unsafe { SetCapture(hwnd) };
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, _) = point_from(lparam);
            unsafe { ReleaseCapture() };
            if let Some(Some(action)) = with_host(|host| host.release(x)) {
                super::post_action(action);
            }
            0
        }
        WM_RBUTTONUP => {
            with_host(Host::close_card);
            let light = with_host(|host| host.appearance.light).unwrap_or(false);
            if let Some(action) = context_menu(hwnd, light) {
                super::post_action(action);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

/// Native context menu, themed to match the taskbar rather than the app.
fn context_menu(owner: HWND, light: bool) -> Option<Action> {
    unsafe {
        theme_menus(owner, light);
        let menu: HMENU = CreatePopupMenu();
        if menu.is_null() {
            return None;
        }
        let open = wide(crate::i18n::tr("taskbar-menu-open"));
        let settings = wide(crate::i18n::tr("taskbar-menu-settings"));
        let hide = wide(crate::i18n::tr("taskbar-menu-hide"));
        AppendMenuW(menu, MF_STRING, MENU_OPEN, open.as_ptr());
        AppendMenuW(menu, MF_STRING, MENU_SETTINGS, settings.as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
        AppendMenuW(menu, MF_STRING, MENU_HIDE, hide.as_ptr());
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        // The menu dismisses on an outside click only while its owner's
        // top-level window is in the foreground.
        SetForegroundWindow(owner);
        let command = TrackPopupMenuEx(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
            cursor.x,
            cursor.y,
            owner,
            null_mut(),
        );
        DestroyMenu(menu);
        match command as usize {
            MENU_OPEN => Some(Action::Open {
                provider: with_host(|host| {
                    host.slots
                        .iter()
                        .find(|slot| !slot.leaving)
                        .and_then(|slot| slot.view.provider)
                })
                .flatten(),
                anchor: (cursor.x, cursor.y),
            }),
            MENU_SETTINGS => Some(Action::Settings),
            MENU_HIDE => Some(Action::Hide),
            _ => None,
        }
    }
}

/// Dark or light Win32 menus through uxtheme's unnamed exports (Windows 10
/// 1903+). Missing exports leave the system default.
fn theme_menus(owner: HWND, light: bool) {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
    type AllowDarkModeForWindow = unsafe extern "system" fn(HWND, i32) -> i32;
    type FlushMenuThemes = unsafe extern "system" fn();
    unsafe {
        let module = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if module.is_null() {
            return;
        }
        let ordinal = |number: usize| GetProcAddress(module, number as *const u8);
        if let Some(allow) = ordinal(133) {
            let allow: AllowDarkModeForWindow = std::mem::transmute(allow);
            allow(owner, i32::from(!light));
        }
        if let Some(mode) = ordinal(135) {
            let mode: SetPreferredAppMode = std::mem::transmute(mode);
            // ForceDark = 2, ForceLight = 3.
            mode(if light { 3 } else { 2 });
        }
        if let Some(flush) = ordinal(136) {
            let flush: FlushMenuThemes = std::mem::transmute(flush);
            flush();
        }
    }
}

/// Eased value moving toward a target.
#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

impl Tween {
    fn at(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            start: Instant::now(),
            duration: Duration::ZERO,
        }
    }

    fn value(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = (now.saturating_duration_since(self.start).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0.0, 1.0);
        // Fluent decelerate: fast start, gentle landing.
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.to - self.from) * eased
    }

    fn done(&self, now: Instant) -> bool {
        self.duration.is_zero() || now.saturating_duration_since(self.start) >= self.duration
    }

    fn set(&mut self, to: f32, now: Instant, duration: Duration, animate: bool) {
        if (to - self.to).abs() < 0.0005 {
            return;
        }
        if animate {
            self.from = self.value(now);
            self.start = now;
            self.duration = duration;
        } else {
            self.from = to;
            self.duration = Duration::ZERO;
        }
        self.to = to;
    }
}

struct Slot {
    view: WidgetView,
    /// Measured width in DIPs.
    full: f32,
    width: Tween,
    opacity: Tween,
    hover: Tween,
    press: Tween,
    progress: Vec<Option<Tween>>,
    leaving: bool,
}

impl Slot {
    fn tweens(&self) -> impl Iterator<Item = &Tween> {
        [&self.width, &self.opacity, &self.hover, &self.press]
            .into_iter()
            .chain(self.progress.iter().flatten())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Geometry {
    taskbar: HWND,
    scale: f32,
    height: i32,
    /// Right edge (tray) or left edge (start) of the strip, in taskbar
    /// client pixels.
    anchor: i32,
    alignment: TaskbarAlignment,
    /// Screen x of the taskbar's client origin.
    origin: i32,
}

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    size: (i32, i32),
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

impl Surface {
    fn new(size: (i32, i32)) -> Option<Self> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return None;
            }
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = size.0;
            info.bmiHeader.biHeight = -size.1;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap);
            Some(Self {
                dc,
                bitmap,
                previous,
                size,
            })
        }
    }
}

/// The hover card: a topmost layered popup above the hovered widget.
struct Card {
    window: HWND,
    surface: Option<Surface>,
    id: String,
    opacity: Tween,
    /// Screen position and pixel size of the window.
    rect: (i32, i32, i32, i32),
    closing: bool,
}

impl Drop for Card {
    fn drop(&mut self) {
        self.surface = None;
        unsafe { DestroyWindow(self.window) };
    }
}

struct Host {
    card: Option<Card>,
    /// Explorer's buttons as last measured.
    obstacles: Option<Obstacles>,
    /// Left edge of a start-aligned strip, past the widgets button.
    leading: i32,
    /// Since when a first show waits for Explorer's buttons to be measured.
    awaiting_obstacles: Option<Instant>,
    controller: HWND,
    window: HWND,
    config: Option<Config>,
    limits: ProviderLimits,
    renderer: Option<Renderer>,
    surface: Option<Surface>,
    geometry: Option<Geometry>,
    appearance: Appearance,
    slots: Vec<Slot>,
    hover: Option<String>,
    pressed: Option<String>,
    tracking: bool,
    frame_timer: bool,
    minute: i64,
    /// Widget id, x and width in DIPs of the last frame, for hit testing.
    hits: Vec<(String, f32, f32)>,
    placed: Option<(i32, i32, i32, i32)>,
    shown: bool,
}

impl Host {
    fn new(controller: HWND) -> Self {
        Self {
            card: None,
            obstacles: None,
            leading: 0,
            awaiting_obstacles: None,
            controller,
            window: null_mut(),
            config: None,
            limits: ProviderLimits::default(),
            renderer: None,
            surface: None,
            geometry: None,
            appearance: current_appearance(false),
            slots: Vec::new(),
            hover: None,
            pressed: None,
            tracking: false,
            frame_timer: false,
            minute: Utc::now().timestamp() / 60,
            hits: Vec::new(),
            placed: None,
            shown: false,
        }
    }

    fn drain(&mut self) {
        let messages = QUEUE
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default();
        if messages.is_empty() {
            return;
        }
        for message in messages {
            match message {
                Message::Config(config) => self.config = Some(config),
                Message::Limits(limits) => self.limits = limits,
            }
        }
        self.reconcile();
    }

    fn poll(&mut self) {
        let geometry = self
            .config
            .as_ref()
            .and_then(|config| locate(config.taskbar.alignment));
        let appearance = current_appearance(
            self.config
                .as_ref()
                .is_some_and(|config| config.colored_icons),
        );
        let minute = Utc::now().timestamp() / 60;
        let obstacles = super::obstacles::latest();
        let window_lost = !self.window.is_null() && unsafe { IsWindow(self.window) } == 0;
        if window_lost {
            // Explorer restarted and destroyed the child with its taskbar.
            self.window = null_mut();
            self.placed = None;
            self.shown = false;
        }
        if geometry != self.geometry
            || appearance != self.appearance
            || minute != self.minute
            || obstacles != self.obstacles
            || window_lost
            || self
                .awaiting_obstacles
                .is_some_and(|since| since.elapsed() >= OBSTACLE_WAIT)
        {
            self.minute = minute;
            self.reconcile();
        } else if !self.window.is_null() {
            self.keep_on_top();
        }
    }

    fn animate(&self) -> bool {
        crate::popup::animations_enabled() && crate::popup::system_animations_enabled()
    }

    /// Brings the strip, its slots and the window in line with the config.
    fn reconcile(&mut self) {
        let now = Instant::now();
        let colored = self
            .config
            .as_ref()
            .is_some_and(|config| config.colored_icons);
        self.appearance = current_appearance(colored);
        let enabled = self
            .config
            .as_ref()
            .is_some_and(|config| config.taskbar.enabled);
        self.geometry = if enabled {
            self.config
                .as_ref()
                .and_then(|config| locate(config.taskbar.alignment))
        } else {
            None
        };
        let views = match (&self.config, self.geometry) {
            (Some(config), Some(_)) if config.taskbar.enabled => model::resolve_visible(
                &config.taskbar.widgets,
                &crate::instances::published_enabled_providers(),
                &self.limits,
                self.appearance,
            ),
            _ => Vec::new(),
        };
        #[cfg(debug_assertions)]
        trace(format!(
            "reconcile: enabled={enabled} geometry={:?} views={}",
            self.geometry,
            views.len()
        ));
        let Some(geometry) = self.geometry else {
            self.teardown();
            return;
        };
        if views.is_empty() && self.window.is_null() {
            self.slots.clear();
            return;
        }
        if self.renderer.is_none() {
            match Renderer::new() {
                Ok(renderer) => self.renderer = Some(renderer),
                Err(error) => {
                    trace(format!("Taskbar widget renderer failed: {error}"));
                    self.teardown();
                    return;
                }
            }
        }
        let metrics = metrics_for(geometry);
        let animate = self.animate() && self.shown;
        let stamp = Utc::now();
        let mut widths = Vec::with_capacity(views.len());
        if let Some(renderer) = self.renderer.as_mut() {
            for view in &views {
                widths.push(renderer.measure(view, metrics, stamp).unwrap_or(0.0));
            }
        }
        super::obstacles::set_wanted(!views.is_empty());
        self.obstacles = super::obstacles::latest();
        // A first show waits briefly for the measurement instead of flashing
        // over Explorer's buttons.
        if self.window.is_null() && self.obstacles.is_none() && !views.is_empty() {
            let since = *self.awaiting_obstacles.get_or_insert_with(Instant::now);
            if since.elapsed() < OBSTACLE_WAIT {
                return;
            }
        }
        self.awaiting_obstacles = None;
        let (leading, available) = free_span(geometry, self.obstacles);
        self.leading = leading;
        let (views, widths) = fit(views, widths, available / metrics.scale);

        let mut previous = std::mem::take(&mut self.slots)
            .into_iter()
            .map(|slot| (slot.view.id.clone(), slot))
            .collect::<HashMap<_, _>>();
        let mut slots = Vec::with_capacity(views.len());
        for (view, full) in views.into_iter().zip(widths) {
            let progress_targets = view.rows.iter().map(|row| row.progress).collect::<Vec<_>>();
            let slot = match previous.remove(&view.id) {
                Some(mut slot) => {
                    slot.leaving = false;
                    slot.full = full;
                    slot.width.set(full, now, RESIZE, animate);
                    slot.opacity.set(1.0, now, FADE, animate);
                    slot.progress.resize(progress_targets.len(), None);
                    for (tween, target) in slot.progress.iter_mut().zip(&progress_targets) {
                        match (tween.as_mut(), target) {
                            (Some(tween), Some(target)) => {
                                tween.set(*target, now, PROGRESS, animate)
                            }
                            (None, Some(target)) => {
                                let mut fresh = Tween::at(0.0);
                                fresh.set(*target, now, PROGRESS, animate);
                                *tween = Some(fresh);
                            }
                            (_, None) => *tween = None,
                        }
                    }
                    slot.view = view;
                    slot
                }
                None => {
                    let mut width = Tween::at(if animate { 0.0 } else { full });
                    width.set(full, now, RESIZE, animate);
                    let mut opacity = Tween::at(if self.animate() { 0.0 } else { 1.0 });
                    opacity.set(1.0, now, FADE, self.animate());
                    let progress = progress_targets
                        .iter()
                        .map(|target| {
                            target.map(|target| {
                                let mut tween = Tween::at(0.0);
                                tween.set(target, now, PROGRESS, self.animate());
                                tween
                            })
                        })
                        .collect();
                    Slot {
                        view,
                        full,
                        width,
                        opacity,
                        hover: Tween::at(0.0),
                        press: Tween::at(0.0),
                        progress,
                        leaving: false,
                    }
                }
            };
            slots.push(slot);
        }
        // Widgets that went away shrink and fade where they were.
        for (_, mut slot) in previous {
            if !animate {
                continue;
            }
            slot.leaving = true;
            slot.width.set(0.0, now, RESIZE, true);
            slot.opacity.set(0.0, now, FADE, true);
            slot.hover.set(0.0, now, HOVER_OUT, true);
            let index = self
                .hits
                .iter()
                .position(|(id, _, _)| *id == slot.view.id)
                .unwrap_or(slots.len())
                .min(slots.len());
            slots.insert(index, slot);
        }
        self.slots = slots;
        if self.hover.as_ref().is_some_and(|id| {
            !self
                .slots
                .iter()
                .any(|slot| &slot.view.id == id && !slot.leaving)
        }) {
            self.hover = None;
        }
        self.render();
    }

    fn teardown(&mut self) {
        super::obstacles::set_wanted(false);
        self.obstacles = None;
        self.card = None;
        unsafe { KillTimer(self.controller, TIMER_CARD) };
        self.slots.clear();
        self.hits.clear();
        self.hover = None;
        self.pressed = None;
        self.tracking = false;
        self.surface = None;
        self.placed = None;
        self.shown = false;
        self.stop_frames();
        if !self.window.is_null() {
            let window = std::mem::replace(&mut self.window, null_mut());
            unsafe { DestroyWindow(window) };
        }
    }

    fn frame(&mut self) {
        self.render();
    }

    fn start_frames(&mut self) {
        if !self.frame_timer {
            unsafe { SetTimer(self.controller, TIMER_FRAME, FRAME, None) };
            self.frame_timer = true;
        }
    }

    fn stop_frames(&mut self) {
        if self.frame_timer {
            unsafe { KillTimer(self.controller, TIMER_FRAME) };
            self.frame_timer = false;
        }
    }

    fn keep_on_top(&self) {
        let Some(geometry) = self.geometry else {
            return;
        };
        unsafe {
            if GetWindow(geometry.taskbar, GW_CHILD) != self.window {
                SetWindowPos(
                    self.window,
                    HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
    }

    fn ensure_window(&mut self, geometry: Geometry, rect: RECT) -> bool {
        if !self.window.is_null() {
            return true;
        }
        unsafe {
            let class = wide(WIDGET_CLASS);
            let title = wide("Codex Minibar Taskbar Widgets");
            // Created top-level, then re-parented: Windows refuses a child
            // created directly under another process's window.
            let window = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                rect.left,
                rect.top,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                null_mut(),
                null_mut(),
                GetModuleHandleW(null_mut()),
                null_mut(),
            );
            if !window.is_null() {
                SetWindowLongPtrW(window, GWL_STYLE, (WS_CHILD | WS_CLIPSIBLINGS) as isize);
                if SetParent(window, geometry.taskbar).is_null() {
                    trace(format!(
                        "Could not attach the taskbar widgets: {}",
                        std::io::Error::last_os_error()
                    ));
                    DestroyWindow(window);
                    return false;
                }
                SetWindowPos(
                    window,
                    null_mut(),
                    rect.left,
                    rect.top,
                    (rect.right - rect.left).max(1),
                    (rect.bottom - rect.top).max(1),
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            if window.is_null() {
                trace(format!(
                    "Could not create the taskbar widget window: {}",
                    std::io::Error::last_os_error()
                ));
                return false;
            }
            self.window = window;
            self.placed = Some(key(rect));
            self.shown = false;
            SetWindowPos(
                window,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        true
    }

    fn render(&mut self) {
        let now = Instant::now();
        let Some(geometry) = self.geometry else {
            self.teardown();
            return;
        };
        self.slots
            .retain(|slot| !(slot.leaving && slot.width.done(now) && slot.opacity.done(now)));
        let metrics = metrics_for(geometry);
        let mut x = 0.0;
        let mut hits = Vec::with_capacity(self.slots.len());
        let mut placed = Vec::with_capacity(self.slots.len());
        for (index, slot) in self.slots.iter().enumerate() {
            let width = slot.width.value(now).max(0.0);
            let share = if slot.full > 0.0 {
                (width / slot.full).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if index > 0 {
                x += WIDGET_GAP * share;
            }
            hits.push((slot.view.id.clone(), x, width));
            placed.push(x);
            x += width;
        }
        let total = x;
        if total <= 0.5 {
            if self.slots.is_empty() {
                self.teardown();
            }
            return;
        }
        let width = (total * metrics.scale).ceil() as i32;
        let height = geometry.height;
        let rect = match geometry.alignment {
            TaskbarAlignment::Tray => {
                let right = geometry.anchor - (TRAY_MARGIN * metrics.scale).round() as i32;
                RECT {
                    left: right - width,
                    top: 0,
                    right,
                    bottom: height,
                }
            }
            TaskbarAlignment::Start => {
                let left = self.leading;
                RECT {
                    left,
                    top: 0,
                    right: left + width,
                    bottom: height,
                }
            }
        };
        // Tray alignment grows leftward: content keeps its right edge.
        let shift = match geometry.alignment {
            TaskbarAlignment::Tray => (width as f32 / metrics.scale - total).max(0.0),
            TaskbarAlignment::Start => 0.0,
        };
        if !self.ensure_window(geometry, rect) {
            return;
        }
        if self
            .surface
            .as_ref()
            .is_none_or(|surface| surface.size != (width, height))
        {
            self.surface = Surface::new((width, height));
        }
        let Some(surface) = self.surface.as_ref() else {
            return;
        };
        let progress = self
            .slots
            .iter()
            .map(|slot| {
                slot.progress
                    .iter()
                    .map(|tween| tween.map(|tween| tween.value(now)))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let items = self
            .slots
            .iter()
            .zip(&placed)
            .zip(&progress)
            .map(|((slot, x), progress)| Item {
                view: &slot.view,
                x: x + shift,
                width: slot.width.value(now).max(0.0),
                hover: slot.hover.value(now),
                press: slot.press.value(now),
                opacity: slot.opacity.value(now),
                progress,
            })
            .collect::<Vec<_>>();
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if let Err(error) = renderer.paint(
            surface.dc,
            (width, height),
            metrics,
            self.appearance,
            &items,
            Utc::now(),
        ) {
            trace(format!("Taskbar widget paint failed: {error}"));
            return;
        }
        drop(items);
        unsafe {
            if self.placed != Some(key(rect)) {
                SetWindowPos(
                    self.window,
                    null_mut(),
                    rect.left,
                    rect.top,
                    width,
                    height,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                self.placed = Some(key(rect));
            }
            let screen = GetDC(null_mut());
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                self.window,
                screen,
                null_mut(),
                &size,
                surface.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
            ReleaseDC(null_mut(), screen);
            if !self.shown {
                ShowWindow(self.window, SW_SHOWNA);
                self.shown = true;
            }
        }
        self.hits = hits
            .into_iter()
            .map(|(id, x, width)| (id, x + shift, width))
            .collect();
        self.render_card(now);
        let animating = self
            .slots
            .iter()
            .any(|slot| slot.tweens().any(|tween| !tween.done(now)))
            || self.slots.iter().any(|slot| slot.leaving)
            || self
                .card
                .as_ref()
                .is_some_and(|card| !card.opacity.done(now) || card.closing);
        if animating {
            self.start_frames();
        } else {
            self.stop_frames();
        }
    }

    fn hit(&self, x: i32) -> Option<String> {
        let scale = self.geometry?.scale;
        let x = x as f32 / scale;
        self.hits
            .iter()
            .find(|(id, left, width)| {
                x >= *left
                    && x < left + width
                    && self
                        .slots
                        .iter()
                        .any(|slot| &slot.view.id == id && !slot.leaving)
            })
            .map(|(id, _, _)| id.clone())
    }

    fn set_hover(&mut self, id: Option<String>) {
        if id == self.hover {
            return;
        }
        let now = Instant::now();
        let animate = self.animate();
        for slot in &mut self.slots {
            let target = if Some(&slot.view.id) == id.as_ref() {
                1.0
            } else {
                0.0
            };
            let duration = if target > 0.5 { HOVER_IN } else { HOVER_OUT };
            slot.hover.set(target, now, duration, animate);
        }
        let reopen = id.is_some()
            && self
                .card
                .as_ref()
                .is_some_and(|card| !card.closing && Some(&card.id) != id.as_ref());
        unsafe { KillTimer(self.controller, TIMER_CARD) };
        if id.is_none() {
            self.close_card();
        } else if reopen {
            // Moving between widgets swaps the card without the delay.
            self.hover = id;
            self.open_card();
            return;
        } else if self.card.as_ref().is_none_or(|card| card.closing) {
            unsafe { SetTimer(self.controller, TIMER_CARD, CARD_DELAY, None) };
        }
        self.hover = id;
        self.render();
    }

    fn open_card(&mut self) {
        let Some(id) = self.hover.clone() else {
            return;
        };
        if self.pressed.is_some() {
            return;
        }
        let now = Instant::now();
        let animate = self.animate();
        match self.card.as_mut() {
            Some(card) => {
                card.id = id;
                card.closing = false;
                card.opacity.set(1.0, now, CARD_IN, animate);
            }
            None => {
                let window = unsafe {
                    let class = wide(CARD_CLASS);
                    CreateWindowExW(
                        WS_EX_LAYERED
                            | WS_EX_TRANSPARENT
                            | WS_EX_TOPMOST
                            | WS_EX_TOOLWINDOW
                            | WS_EX_NOACTIVATE,
                        class.as_ptr(),
                        null_mut(),
                        WS_POPUP,
                        0,
                        0,
                        1,
                        1,
                        null_mut(),
                        null_mut(),
                        GetModuleHandleW(null_mut()),
                        null_mut(),
                    )
                };
                if window.is_null() {
                    return;
                }
                let mut opacity = Tween::at(0.0);
                opacity.set(1.0, now, CARD_IN, animate);
                self.card = Some(Card {
                    window,
                    surface: None,
                    id,
                    opacity,
                    rect: (0, 0, 0, 0),
                    closing: false,
                });
            }
        }
        self.render();
    }

    fn close_card(&mut self) {
        unsafe { KillTimer(self.controller, TIMER_CARD) };
        let now = Instant::now();
        let animate = self.animate();
        let Some(card) = self.card.as_mut() else {
            return;
        };
        if !animate {
            self.card = None;
            return;
        }
        if !card.closing {
            card.closing = true;
            card.opacity.set(0.0, now, CARD_OUT, true);
            self.start_frames();
        }
    }

    /// Draws the hover card above its widget.
    fn render_card(&mut self, now: Instant) {
        let Some(geometry) = self.geometry else {
            self.card = None;
            return;
        };
        if self
            .card
            .as_ref()
            .is_some_and(|card| card.closing && card.opacity.done(now))
        {
            self.card = None;
        }
        let Some(card) = self.card.as_ref() else {
            return;
        };
        let Some(slot) = self
            .slots
            .iter()
            .find(|slot| slot.view.id == card.id && !slot.leaving)
        else {
            self.card = None;
            return;
        };
        let Some(&(_, hit_x, hit_width)) = self.hits.iter().find(|(id, _, _)| *id == card.id)
        else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let scale = geometry.scale;
        let stamp = Utc::now();
        let Ok((width_dip, height_dip)) = renderer.card_size(&slot.view, scale, stamp) else {
            return;
        };
        let width = (width_dip * scale).ceil() as i32;
        let height = (height_dip * scale).ceil() as i32;
        let mut strip = std::mem::MaybeUninit::<RECT>::zeroed();
        unsafe { GetWindowRect(self.window, strip.as_mut_ptr()) };
        let strip = unsafe { strip.assume_init() };
        let monitor = monitor_bounds(geometry.taskbar);
        let margin = (CARD_MARGIN * scale).round() as i32;
        let gap = (CARD_GAP * scale).round() as i32;
        let center = strip.left + ((hit_x + hit_width / 2.0) * scale).round() as i32;
        let edge = (8.0 * scale).round() as i32;
        let left = (center - width / 2)
            .min(monitor.right - edge - width + margin)
            .max(monitor.left + edge - margin);
        // Above a bottom taskbar, below a top one.
        let below = strip.top < (monitor.top + monitor.bottom) / 2;
        let top = if below {
            strip.bottom + gap - margin
        } else {
            strip.top - gap - height + margin
        };
        let opacity = card.opacity.value(now);
        let rise = if below { -1.0 } else { 1.0 } * CARD_RISE * (1.0 - opacity);
        let progress = slot
            .progress
            .iter()
            .map(|tween| tween.map(|tween| tween.value(now)))
            .collect::<Vec<_>>();
        let view = slot.view.clone();
        let appearance = self.appearance;
        let Some(card) = self.card.as_mut() else {
            return;
        };
        if card
            .surface
            .as_ref()
            .is_none_or(|surface| surface.size != (width, height))
        {
            card.surface = Surface::new((width, height));
        }
        let Some(surface) = card.surface.as_ref() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if renderer
            .paint_card(
                surface.dc,
                (width, height),
                scale,
                appearance,
                &view,
                &progress,
                opacity,
                rise,
                stamp,
            )
            .is_err()
        {
            return;
        }
        unsafe {
            let screen = GetDC(null_mut());
            let position = POINT { x: left, y: top };
            let size = SIZE {
                cx: width,
                cy: height,
            };
            let source = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                card.window,
                screen,
                &position,
                &size,
                surface.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
            ReleaseDC(null_mut(), screen);
            if card.rect == (0, 0, 0, 0) {
                ShowWindow(card.window, SW_SHOWNA);
            }
            card.rect = (left, top, width, height);
        }
    }

    /// Returns whether leave tracking must be requested.
    fn mouse_move(&mut self, x: i32) -> bool {
        let id = self.hit(x);
        self.set_hover(id);
        let track = !self.tracking;
        self.tracking = true;
        track
    }

    fn mouse_leave(&mut self) {
        self.tracking = false;
        if self.pressed.is_none() {
            self.set_hover(None);
        }
    }

    fn set_press(&mut self, id: Option<&String>) {
        let now = Instant::now();
        let animate = self.animate();
        for slot in &mut self.slots {
            let target = if Some(&slot.view.id) == id { 1.0 } else { 0.0 };
            slot.press.set(target, now, PRESS, animate);
        }
        self.render();
    }

    fn press(&mut self, x: i32) -> bool {
        let Some(id) = self.hit(x) else {
            return false;
        };
        self.close_card();
        self.set_press(Some(&id));
        self.pressed = Some(id);
        true
    }

    fn release(&mut self, x: i32) -> Option<Action> {
        let pressed = self.pressed.take()?;
        self.set_press(None);
        let over = self.hit(x);
        if over.as_ref() != Some(&pressed) {
            self.set_hover(over);
            return None;
        }
        let provider = self
            .slots
            .iter()
            .find(|slot| slot.view.id == pressed)
            .and_then(|slot| slot.view.provider);
        let mut cursor = POINT { x: 0, y: 0 };
        unsafe { GetCursorPos(&mut cursor) };
        Some(Action::Open {
            provider,
            anchor: (cursor.x, cursor.y),
        })
    }
}

/// Longest a first show waits for Explorer's buttons to be measured.
const OBSTACLE_WAIT: Duration = Duration::from_millis(1500);

/// Clearance kept from Explorer's buttons, in DIPs.
const OBSTACLE_GAP: f32 = 8.0;

/// Left edge of a start-aligned strip and the width available to the strip,
/// both in taskbar client pixels.
fn free_span(geometry: Geometry, obstacles: Option<Obstacles>) -> (i32, f32) {
    let scale = geometry.scale;
    let gap = (OBSTACLE_GAP * scale).round() as i32;
    let cluster = obstacles.and_then(|obstacles| obstacles.cluster);
    let to_client = |x: i32| x - geometry.origin;
    match geometry.alignment {
        TaskbarAlignment::Tray => {
            let right = geometry.anchor - (TRAY_MARGIN * scale).round() as i32;
            let left = cluster
                .map(|(_, right)| to_client(right) + gap)
                .filter(|left| *left < right)
                .unwrap_or(i32::MIN / 2);
            (0, (right - left).max(0) as f32)
        }
        TaskbarAlignment::Start => {
            let leading = obstacles
                .and_then(|obstacles| obstacles.widgets_right)
                .map(to_client)
                .unwrap_or(geometry.anchor)
                .max(geometry.anchor)
                + (START_MARGIN * scale).round() as i32;
            let right = cluster
                .map(|(left, _)| to_client(left) - gap)
                .unwrap_or(i32::MAX / 2);
            (leading, (right - leading).max(0) as f32)
        }
    }
}

/// Keeps widgets in order while they fit `available` DIPs; the rest
/// overflow, the way notification icons do.
fn fit(views: Vec<WidgetView>, widths: Vec<f32>, available: f32) -> (Vec<WidgetView>, Vec<f32>) {
    let mut used = 0.0;
    views
        .into_iter()
        .zip(widths)
        .enumerate()
        .take_while(|(index, (_, width))| {
            let next = used + width + if *index > 0 { WIDGET_GAP } else { 0.0 };
            let fits = next <= available + 0.5;
            if fits {
                used = next;
            }
            fits
        })
        .map(|(_, pair)| pair)
        .unzip()
}

/// Bounds of the monitor showing `window`.
fn monitor_bounds(window: HWND) -> RECT {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
    };
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        info.rcMonitor
    }
}

fn key(rect: RECT) -> (i32, i32, i32, i32) {
    (rect.left, rect.top, rect.right, rect.bottom)
}

fn current_appearance(colored_icons: bool) -> Appearance {
    Appearance {
        light: crate::taskbar::uses_light_theme(),
        colored_icons,
        accent: crate::theme::current_accent_rgb(),
    }
}

fn metrics_for(geometry: Geometry) -> Metrics {
    Metrics {
        scale: geometry.scale,
        height: geometry.height as f32 / geometry.scale,
    }
}

/// Finds the primary taskbar and where the strip belongs on it.
fn locate(alignment: TaskbarAlignment) -> Option<Geometry> {
    unsafe {
        let taskbar = FindWindowW(wide("Shell_TrayWnd").as_ptr(), null_mut());
        if taskbar.is_null() {
            return None;
        }
        let mut client = std::mem::zeroed::<RECT>();
        GetClientRect(taskbar, &mut client);
        let (width, height) = (client.right - client.left, client.bottom - client.top);
        // Windows 11 has no vertical taskbar; an older shell may.
        if width <= 0 || height <= 0 || width < height {
            return None;
        }
        let dpi = GetDpiForWindow(taskbar);
        let scale = if dpi == 0 { 1.0 } else { dpi as f32 / 96.0 };
        let anchor = match alignment {
            TaskbarAlignment::Tray => {
                let tray = FindWindowExW(
                    taskbar,
                    null_mut(),
                    wide("TrayNotifyWnd").as_ptr(),
                    null_mut(),
                );
                if tray.is_null() {
                    client.right - (200.0 * scale) as i32
                } else {
                    let mut rect = std::mem::zeroed::<RECT>();
                    GetWindowRect(tray, &mut rect);
                    let mut points = [
                        POINT {
                            x: rect.left,
                            y: rect.top,
                        },
                        POINT {
                            x: rect.right,
                            y: rect.bottom,
                        },
                    ];
                    MapWindowPoints(null_mut(), taskbar, points.as_mut_ptr(), 2);
                    points[0].x
                }
            }
            TaskbarAlignment::Start => 0,
        };
        let mut origin = POINT { x: 0, y: 0 };
        MapWindowPoints(taskbar, null_mut(), &mut origin, 1);
        Some(Geometry {
            taskbar,
            scale,
            height,
            anchor,
            alignment,
            origin: origin.x,
        })
    }
}
