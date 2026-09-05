//! Offline, bounded native geometry exercise. No config, vault, hooks or providers.
use std::sync::atomic::Ordering;
use tauri::{Emitter, Manager};
pub fn run() {
    let scene = std::env::args().nth(1).unwrap_or_else(|| "orb".into());
    assert!(matches!(
        scene.as_str(),
        "orb" | "project" | "preview" | "hint"
    ));
    tauri::Builder::default()
        .manage(crate::state::AppState::new())
        .invoke_handler(tauri::generate_handler![crate::floating::floating_position, crate::flow::overlay_snapshot])
        .setup(move |app| {
            let handle = app.handle().clone();
            let window = crate::get_or_create_window(&handle, "overlay").ok_or("Overlay creation failed")?;
            window.set_focusable(false)?;
            window.set_ignore_cursor_events(true)?;
            let state = handle.state::<crate::state::AppState>();
            let payload = serde_json::json!({ "runId": 1, "sequence": state.event_seq.fetch_add(1, Ordering::SeqCst) + 1,
                "phase": if scene == "preview" { "preview" } else if scene == "hint" { "hint" } else { "refining" },
                "project": if scene == "project" { Some("A project with a deliberately long name") } else { None },
                "message": if scene == "hint" { Some("Field unavailable. Select text in another editor.") } else { None },
                "confirmationScope": "selection"
            });
            *state.last_state.lock().unwrap() = Some(payload.clone());
            let monitors = crate::monitors_of(&window);
            println!("{}", serde_json::json!({ "scene": scene, "monitors": monitors.iter().map(|m| serde_json::json!({ "x":m.work.x,"y":m.work.y,"width":m.work.w,"height":m.work.h,"scale":m.scale })).collect::<Vec<_>>() }));
            window.show()?;
            std::thread::spawn(move || {
                let mut surface = crate::floating::Surface::new(handle.clone(), window, "ember://overlay-at");
                let start = std::time::Instant::now();
                while start.elapsed() < std::time::Duration::from_secs(25) {
                    surface.follow();
                    std::thread::sleep(std::time::Duration::from_millis(16));
                }
                let _ = handle.emit_to("overlay", "ember://state", serde_json::json!({ "runId": 1, "sequence":u64::MAX, "phase":"hidden" }));
                handle.exit(0);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Native qualification failed");
}
