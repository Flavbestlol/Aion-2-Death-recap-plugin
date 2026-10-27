//! Death Recap, the program: reads Flav-Toolkit's feed and shows the recap
//! at the top of the screen when you die or lose a duel. Flav-Toolkit's hub
//! starts and stops it; its dock and the hub's buttons open the last recap
//! and the settings (the feed's `show`).
//!
//! ```text
//! death-recap        beside a running Flav-Toolkit (or `flav feed <capture.txt>`)
//! ```

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alt;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use death_recap::reader::{self, Screen};
use death_recap::settings::Settings;
use death_recap::store::{Extra, Recap, Shared, lock};
use flav_sdk::Show;
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

/// The overlay's width, in logical pixels.
const WIDTH: f64 = 900.0;
/// The settings panel's size, in logical pixels.
const PANEL: (f64, f64) = (330.0, 290.0);

fn main() {
    tauri::Builder::default()
        // One Death Recap at a time: starting it again opens the running one's
        // settings.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = open_settings(app.clone());
        }))
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fit,
            hide_overlay,
            open_profile,
            last_recap,
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

            // The overlay never takes the focus from the game, even when clicked.
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

/// The screen the reader shows things on: the overlay.
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
        reader::say(&mut std::io::stdout(), text);
    }
    fn settings_changed(&self) {
        emit_settings(&self.0);
    }
    fn show(&self, what: Show) {
        // Tauri's windows are handled on its own thread.
        let app = self.0.clone();
        let _ = self.0.run_on_main_thread(move || match what {
            // The hub's Open.
            Show::Open => open_last(&app),
            // Death Recap's icon on the dock: open, or put away when shown.
            Show::Toggle => toggle(&app),
            Show::Settings => {
                let _ = open_settings(app);
            }
        });
    }
}

/// Every 50 ms: Alt (while a recap is shown); every 250 ms: whether Alt can
/// be read.
fn watch(app: AppHandle) {
    let (mut alt_was, mut blocked_was) = (false, false);
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
        }
    }
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
    let _ = window.show();
}

/// The last recap, at the top of the screen (left up if it already is).
fn open_last(app: &AppHandle) {
    let visible = app.get_webview_window("overlay").and_then(|w| w.is_visible().ok()).unwrap_or(false);
    if !visible && let Some(recap) = lock(&app.state::<Arc<Shared>>().last).clone() {
        show_recap(app, &recap);
    }
}

/// The dock's icon: the last recap when none is shown, or put the shown
/// one away, as its × does.
fn toggle(app: &AppHandle) {
    match app.get_webview_window("overlay") {
        Some(window) if window.is_visible().unwrap_or(false) => put_away(app, &window),
        _ => open_last(app),
    }
}

/// Hide the recap; the death's note in the hub and on the dock comes down
/// with it.
fn put_away(app: &AppHandle, window: &WebviewWindow) {
    let _ = window.hide();
    app.state::<Arc<Shared>>().closed.store(true, Ordering::Relaxed);
}

/// The overlay's page asks for the height its content needs.
#[tauri::command]
fn fit(window: WebviewWindow, height: f64) {
    let _ = window.set_size(LogicalSize::new(WIDTH, height.clamp(40.0, 900.0)));
}

#[tauri::command]
fn hide_overlay(app: AppHandle, window: WebviewWindow) {
    put_away(&app, &window);
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

// ---- the settings panel (the hub's Settings, or starting Death Recap again) ----

/// The panel, in the middle of the main screen. It takes the focus while
/// open (it has switches to click) and closes when it loses it.
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
    if let Ok(Some(m)) = window.primary_monitor() {
        let scale = m.scale_factor();
        let (w, h) = ((PANEL.0 * scale) as i32, (PANEL.1 * scale) as i32);
        let (p, s) = (m.position(), m.size());
        let _ =
            window.set_position(PhysicalPosition::new(p.x + (s.width as i32 - w) / 2, p.y + (s.height as i32 - h) / 2));
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

/// One change from the panel: a switch's `on`, or a choice's `value`.
#[derive(Deserialize)]
struct Change {
    key: String,
    on: Option<bool>,
    value: Option<String>,
}

/// The panel's change goes the hub's way (`Shared::change`): saved, and the
/// hub is told the new values by the reader.
#[tauri::command]
fn settings_set(app: AppHandle, shared: tauri::State<'_, Arc<Shared>>, change: Change) -> PanelState {
    let value = match (change.on, change.value) {
        (Some(on), _) => serde_json::Value::Bool(on),
        (None, Some(v)) => serde_json::Value::String(v),
        (None, None) => serde_json::Value::Null,
    };
    if shared.change(&change.key, &value) {
        emit_settings(&app);
    }
    settings_get(shared)
}

/// The settings changed: the overlay takes the look, an open panel shows
/// the new values.
fn emit_settings(app: &AppHandle) {
    let theme = lock(&app.state::<Arc<Shared>>().settings).theme;
    let _ = app.emit_to("overlay", "theme", theme);
    let _ = app.emit_to("settings", "settings-changed", ());
}

/// `give_back`: the player closed it (× or Esc), so the game gets its focus
/// back; closed because another window was clicked, that window keeps it.
#[tauri::command]
fn settings_close(window: WebviewWindow, give_back: bool) {
    let _ = window.hide();
    if give_back {
        alt::focus_game();
    }
}
