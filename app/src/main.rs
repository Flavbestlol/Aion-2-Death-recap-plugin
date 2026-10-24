//! Death Recap, the program: reads Flav-Toolkit's feed and shows the recap
//! at the top of the screen when you die or lose a duel. A skull icon by the
//! minimap opens the last recap any time (a click) and the settings (a
//! right-click). Flav-Toolkit's hub starts and stops it.
//!
//! ```text
//! death-recap        beside a running Flav-Toolkit (or `flav feed <capture.txt>`)
//! ```

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alt;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use death_recap::reader::{self, Screen};
use death_recap::settings::{History, Settings};
use death_recap::store::{Extra, Recap, Shared, lock};
use flav_sdk::Show;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

/// The overlay's width, in logical pixels.
const WIDTH: f64 = 900.0;
/// The minimap icon's size, in logical pixels.
const ICON: f64 = 40.0;
/// The settings panel's size, in logical pixels.
const PANEL: (f64, f64) = (330.0, 290.0);

/// The settings changed and are not on disk yet (the icon being dragged).
struct Dirty(AtomicBool);

fn main() {
    tauri::Builder::default()
        // One Death Recap at a time: starting it again opens the running one's
        // settings (the way back to them when the icon is turned off).
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = open_settings(app.clone());
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(Dirty(AtomicBool::new(false)))
        .invoke_handler(tauri::generate_handler![
            fit,
            hide_overlay,
            open_profile,
            last_recap,
            toggle_recap,
            open_settings,
            settings_get,
            settings_set,
            settings_close
        ])
        .setup(|app| {
            let dir = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("death-recap"));
            // Flav-Toolkit's own folder: read once, on the first start, for the
            // choices made when Death Recap was built into it.
            let core_dir = dir.parent().map(|p| p.join("com.flavbest.flav-toolkit"));
            let shared = Shared::new(dir, core_dir.as_deref());
            app.manage(shared.clone());

            // Neither window takes the focus from the game, even when clicked.
            WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("index.html".into()))
                .title("Death Recap")
                .inner_size(WIDTH, 60.0)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focusable(false)
                .focused(false)
                .visible(false)
                .build()?;
            let icon = WebviewWindowBuilder::new(app, "icon", WebviewUrl::App("icon.html".into()))
                .title("Death Recap · icon")
                .inner_size(ICON, ICON)
                .min_inner_size(ICON, ICON)
                .max_inner_size(ICON, ICON)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focusable(false)
                .focused(false)
                .visible(false)
                .build()?;
            place_icon(&icon, &shared);
            let handle = app.handle().clone();
            icon.on_window_event(move |event| {
                if let WindowEvent::Moved(at) = event {
                    let shared = handle.state::<Arc<Shared>>();
                    lock(&shared.settings).icon_at = Some((at.x, at.y));
                    handle.state::<Dirty>().0.store(true, Ordering::Relaxed);
                }
            });

            let handle = app.handle().clone();
            std::thread::Builder::new().name("recap-watch".into()).spawn(move || watch(handle))?;

            let screen: Arc<dyn Screen> = Arc::new(Windows(app.handle().clone()));
            std::thread::Builder::new().name("recap-reader".into()).spawn(move || {
                reader::run(shared, screen, &death_recap::config(), &|| false);
            })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Death Recap could not start");
}

/// The screen the reader shows things on: the overlay and the icon.
struct Windows(AppHandle);

impl Screen for Windows {
    fn recap(&self, recap: &Recap) {
        show_recap(&self.0, recap);
    }
    fn extra(&self, fight_id: i64, id: i32, extra: &Extra) {
        #[derive(Serialize, Clone)]
        struct Message<'a> {
            fight_id: i64,
            id: i32,
            extra: &'a Extra,
        }
        let _ = self.0.emit_to("overlay", "extra", Message { fight_id, id, extra });
    }
    fn revived(&self) {
        let _ = self.0.emit_to("overlay", "revived", ());
    }
    fn status(&self, text: &str) {
        // The hub writes what an add-on prints into its log.
        println!("{text}");
    }
    fn show(&self, what: Show) {
        // Tauri's windows are handled on its own thread.
        let app = self.0.clone();
        let _ = self.0.run_on_main_thread(move || match what {
            // The hub's Open: the last recap (left up if it already is).
            Show::Open => {
                let visible = app.get_webview_window("overlay").and_then(|w| w.is_visible().ok()).unwrap_or(false);
                if !visible {
                    toggle(&app);
                }
            }
            Show::Settings => {
                let _ = open_settings(app);
            }
        });
    }
}

/// Every 50 ms: Alt (while a recap is shown); every 250 ms: whether Alt can
/// be read, and whether the icon should be up (the game in front); every
/// second: settings to disk.
fn watch(app: AppHandle) {
    let (mut alt_was, mut blocked_was, mut icon_was) = (false, false, false);
    let mut tick: u64 = 0;
    loop {
        std::thread::sleep(Duration::from_millis(50));
        tick += 1;
        let overlay = app.get_webview_window("overlay");
        let visible = overlay.as_ref().and_then(|w| w.is_visible().ok()).unwrap_or(false);
        let held = visible && alt::held();
        if held != alt_was {
            alt_was = held;
            let _ = app.emit_to("overlay", "alt", held);
        }
        if tick.is_multiple_of(5) {
            let blocked = visible && alt::blocked();
            if blocked != blocked_was {
                blocked_was = blocked;
                let _ = app.emit_to("overlay", "alt-blocked", blocked);
            }
            let shared = app.state::<Arc<Shared>>();
            let want = lock(&shared.settings).icon && alt::game_in_front();
            if want != icon_was
                && let Some(icon) = app.get_webview_window("icon")
            {
                icon_was = want;
                let _ = if want { icon.show() } else { icon.hide() };
            }
        }
        if tick.is_multiple_of(20) && app.state::<Dirty>().0.swap(false, Ordering::Relaxed) {
            app.state::<Arc<Shared>>().save_settings();
        }
    }
}

/// The icon where the player left it, or left of the minimap (the game's
/// top right corner) the first time.
fn place_icon(icon: &WebviewWindow, shared: &Arc<Shared>) {
    let saved = lock(&shared.settings).icon_at;
    let on_screen = |x: i32, y: i32| {
        icon.available_monitors().unwrap_or_default().iter().any(|m| {
            let (p, s) = (m.position(), m.size());
            x >= p.x && y >= p.y && x < p.x + s.width as i32 - 8 && y < p.y + s.height as i32 - 8
        })
    };
    let at = match saved {
        Some((x, y)) if on_screen(x, y) => PhysicalPosition::new(x, y),
        _ => match icon.primary_monitor() {
            Ok(Some(m)) => {
                let scale = m.scale_factor();
                PhysicalPosition::new(
                    m.position().x + m.size().width as i32 - (300.0 * scale) as i32,
                    m.position().y + (14.0 * scale) as i32,
                )
            }
            _ => PhysicalPosition::new(0, 0),
        },
    };
    let _ = icon.set_position(at);
}

/// Show a recap at the top centre of the main screen.
fn show_recap(app: &AppHandle, recap: &Recap) {
    let Some(window) = app.get_webview_window("overlay") else { return };
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let scale = monitor.scale_factor();
        let x = monitor.position().x + ((monitor.size().width as f64 - WIDTH * scale) / 2.0) as i32;
        let _ = window.set_position(PhysicalPosition::new(x, monitor.position().y));
    }
    let _ = app.emit_to("overlay", "recap", recap);
    let _ = app.emit_to("icon", "has-recap", true);
    let _ = app.emit_to("icon", "overlay-open", true);
    let _ = window.show();
}

/// The last recap: shown when hidden, closed when shown.
fn toggle(app: &AppHandle) {
    let visible = app.get_webview_window("overlay").and_then(|w| w.is_visible().ok()).unwrap_or(false);
    if visible {
        let _ = app.emit_to("overlay", "close", ());
    } else if let Some(recap) = lock(&app.state::<Arc<Shared>>().last).clone() {
        show_recap(app, &recap);
    } else {
        let _ = app.emit_to("icon", "nothing", ());
    }
}

/// The minimap icon was clicked.
#[tauri::command]
fn toggle_recap(app: AppHandle) {
    toggle(&app);
    // If the click took the focus, the game gets it back.
    alt::focus_game();
}

/// The overlay's page asks for the height its content needs.
#[tauri::command]
fn fit(window: WebviewWindow, height: f64) {
    let _ = window.set_size(LogicalSize::new(WIDTH, height.clamp(40.0, 900.0)));
}

#[tauri::command]
fn hide_overlay(app: AppHandle, window: WebviewWindow) {
    let _ = window.hide();
    let _ = app.emit_to("icon", "overlay-open", false);
    // The death's note in the hub and on the dock comes down with it.
    app.state::<Arc<Shared>>().closed.store(true, Ordering::Relaxed);
}

/// Open an attacker's official character page (only pages the lookups
/// found; the page never names an address itself).
#[tauri::command]
fn open_profile(shared: tauri::State<'_, Arc<Shared>>, fight_id: i64, id: i32) {
    if let Some(url) = lock(&shared.pages).get(&(fight_id, id)).cloned() {
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    }
}

/// The last recap, for a page that (re)loads after it was sent.
#[tauri::command]
fn last_recap(shared: tauri::State<'_, Arc<Shared>>) -> Option<Recap> {
    lock(&shared.last).clone()
}

// ---- the settings panel (a right-click on the icon) ----

/// The panel, beside the icon. It takes the focus while open (it has
/// switches to click) and closes when it loses it.
#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    let window = match app.get_webview_window("settings") {
        Some(w) => w,
        None => WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Death Recap · settings")
            .inner_size(PANEL.0, PANEL.1)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible(false)
            .build()
            .map_err(|e| e.to_string())?,
    };
    if let Some(icon) = app.get_webview_window("icon")
        && let (Ok(at), Ok(scale)) = (icon.outer_position(), icon.scale_factor())
    {
        // Below the icon, kept on its screen.
        let (w, h) = ((PANEL.0 * scale) as i32, (PANEL.1 * scale) as i32);
        let mut x = at.x + (ICON * scale) as i32 - w;
        let mut y = at.y + (ICON * scale) as i32 + 4;
        if let Ok(Some(m)) = icon.current_monitor() {
            let (p, s) = (m.position(), m.size());
            x = x.clamp(p.x, p.x + s.width as i32 - w);
            if y + h > p.y + s.height as i32 {
                y = at.y - h - 4;
            }
        }
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[derive(Serialize)]
struct PanelState {
    settings: Settings,
    status: String,
    version: &'static str,
    fights: usize,
}

#[tauri::command]
fn settings_get(shared: tauri::State<'_, Arc<Shared>>) -> PanelState {
    PanelState {
        settings: lock(&shared.settings).clone(),
        status: lock(&shared.status).clone(),
        version: death_recap::VERSION,
        fights: shared.fights.load(Ordering::Relaxed),
    }
}

/// One change from the panel.
#[derive(Deserialize)]
#[serde(tag = "key", rename_all = "snake_case")]
enum Change {
    OnlineLookups { on: bool },
    EveryFight { on: bool },
    Icon { on: bool },
    History { value: History },
}

#[tauri::command]
fn settings_set(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>, change: Change) -> PanelState {
    {
        let mut s = lock(&shared.settings);
        match change {
            Change::OnlineLookups { on } => s.online_lookups = on,
            Change::EveryFight { on } => s.every_fight = on,
            Change::Icon { on } => s.icon = on,
            Change::History { value } => s.history = value,
        }
    }
    shared.save_settings();
    shared.save_history();
    if !lock(&shared.settings).icon
        && let Some(icon) = app.get_webview_window("icon")
    {
        let _ = icon.hide();
    }
    settings_get(shared)
}

#[tauri::command]
fn settings_close(window: WebviewWindow) {
    let _ = window.hide();
    alt::focus_game();
}
