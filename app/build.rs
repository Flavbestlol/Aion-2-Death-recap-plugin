//! The Tauri context. The class emblems the overlay shows
//! (`ui/img/class/<code>.png`) come from the A2 Tools DPS Meter's repository
//! and are not committed here: `python tools/class_icons.py` fetches them
//! before a build (CI does). A build without them still works; the overlay
//! then shows no emblem.

fn main() {
    println!("cargo:rerun-if-changed=ui/img/class");
    tauri_build::build();
}
