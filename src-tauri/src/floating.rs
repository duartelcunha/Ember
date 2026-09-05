//! One monitor transition at a time. Cursor motion moves DOM content, not a native window.
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub sequence: u64,
    pub generation: u64,
    pub ready: bool,
    pub scale: f64,
    pub width: i32,
    pub height: i32,
    pub x: f64,
    pub y: f64,
    pub origin_x: i32,
    pub origin_y: i32,
}

pub struct Surface {
    app: AppHandle,
    window: WebviewWindow,
    event: &'static str,
    monitors: Vec<crate::MonitorInfo>,
    refreshed: Instant,
    placed: Option<(crate::geom::Rect, f64)>,
    last_cursor: Option<(i32, i32)>,
    generation: u64,
    pending: Option<((crate::geom::Rect, f64), Instant)>,
}

impl Surface {
    pub fn new(app: AppHandle, window: WebviewWindow, event: &'static str) -> Self {
        let monitors = crate::monitors_of(&window);
        Self {
            app,
            window,
            event,
            monitors,
            refreshed: Instant::now(),
            placed: None,
            last_cursor: None,
            generation: 0,
            pending: None,
        }
    }

    pub fn follow(&mut self) {
        // A bounded reconciliation also covers display removal and work-area changes which
        // don't produce a Tauri scale event. Never enumerate displays at cursor frame rate.
        if self.refreshed.elapsed() >= Duration::from_millis(500) {
            self.monitors = crate::monitors_of(&self.window);
            self.refreshed = Instant::now();
        }
        let Ok(cursor) = self.app.cursor_position() else {
            return;
        };
        let point = (cursor.x as i32, cursor.y as i32);
        let rectangles: Vec<_> = self.monitors.iter().map(|m| m.full).collect();
        let Some((full, _)) = crate::geom::monitor_for_point(point.0, point.1, &rectangles) else {
            return;
        };
        let Some(monitor) = self.monitors.iter().find(|m| m.full == full) else {
            return;
        };
        let placement = (monitor.work, monitor.scale);
        let transitioned = self.placed != Some(placement);
        if transitioned {
            let area = monitor.work;
            if self.pending.is_none_or(|(target, started)| {
                target != placement || started.elapsed() >= Duration::from_millis(500)
            }) {
                self.pending = Some((placement, Instant::now()));
                self.generation = self
                    .app
                    .state::<crate::state::AppState>()
                    .event_seq
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1;
                // Invalidate the previous DOM geometry before changing the native surface.
                let invalid = Position {
                    sequence: self
                        .app
                        .state::<crate::state::AppState>()
                        .event_seq
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                        + 1,
                    generation: self.generation,
                    ready: false,
                    scale: monitor.scale,
                    width: area.w,
                    height: area.h,
                    x: cursor.x,
                    y: cursor.y,
                    origin_x: area.x,
                    origin_y: area.y,
                };
                let state = self.app.state::<crate::state::AppState>();
                if let Ok(mut positions) = state.floating_positions.lock() {
                    positions.insert(self.window.label().to_owned(), invalid.clone());
                }
                let _ = self.app.emit_to(self.window.label(), self.event, invalid);
                if self
                    .window
                    .set_size(PhysicalSize::new(
                        area.w.max(1) as u32,
                        area.h.max(1) as u32,
                    ))
                    .is_err()
                    || self
                        .window
                        .set_position(PhysicalPosition::new(area.x, area.y))
                        .is_err()
                {
                    log::warn!("floating surface: monitor placement failed");
                    return;
                }
            }
            // A successful setter can precede the OS resize/DPI notification.
            let coherent = self.window.inner_size().is_ok_and(|size| {
                size.width == area.w.max(1) as u32 && size.height == area.h.max(1) as u32
            }) && self
                .window
                .inner_position()
                .is_ok_and(|pos| pos.x == area.x && pos.y == area.y)
                && self
                    .window
                    .scale_factor()
                    .is_ok_and(|scale| (scale - monitor.scale).abs() < 0.01);
            if !coherent {
                return;
            }
            self.placed = Some(placement);
            self.pending = None;
        }
        if transitioned || self.last_cursor != Some(point) {
            let payload = Position {
                generation: self.generation,
                ready: true,
                scale: monitor.scale,
                width: monitor.work.w,
                height: monitor.work.h,
                sequence: self
                    .app
                    .state::<crate::state::AppState>()
                    .event_seq
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                    + 1,
                x: cursor.x,
                y: cursor.y,
                origin_x: monitor.work.x,
                origin_y: monitor.work.y,
            };
            let state = self.app.state::<crate::state::AppState>();
            if let Ok(mut positions) = state.floating_positions.lock() {
                positions.insert(self.window.label().to_owned(), payload.clone());
            }
            let _ = self.app.emit_to(self.window.label(), self.event, payload);
            self.last_cursor = Some(point);
        }
    }
}

#[tauri::command]
pub fn floating_position(
    window: WebviewWindow,
    state: tauri::State<'_, crate::state::AppState>,
) -> Result<Option<Position>, String> {
    if !matches!(window.label(), "overlay" | "picker") {
        return Err("Unsupported floating window".into());
    }
    Ok(state
        .floating_positions
        .lock()
        .map_err(|_| "Position unavailable")?
        .get(window.label())
        .cloned())
}
