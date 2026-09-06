//! Placement checks for the settings window, pure so they run in tests without a monitor.
//!
//! The window-state plugin restores whatever geometry was saved last time. That geometry can be
//! wrong today: a monitor was unplugged, the DPI changed, the laptop is docked somewhere else.
//! The settings window has no native title bar and no maximise button, so once its title strip
//! is off every screen there is nothing to grab and drag back. The shell asks these functions
//! before trusting the saved spot and re-centres when they say no.

use crate::overlay_geom::Rect;

/// The work area whose visible part of the top `strip_h` px of `win` is at least
/// `min_visible_w` wide and half the strip tall. Half, not one pixel: the plugin's own check
/// accepts any intersection, and one visible pixel is not a grab handle.
pub fn strip_area(win: Rect, work_areas: &[Rect], strip_h: i32, min_visible_w: i32) -> Option<Rect> {
    if win.w <= 0 || win.h <= 0 || strip_h <= 0 {
        return None;
    }
    let strip_h = strip_h.min(win.h);
    let need_h = (strip_h + 1) / 2;
    let need_w = min_visible_w.max(1);
    work_areas.iter().copied().find(|a| {
        let visible_w = (win.x + win.w).min(a.x + a.w) - win.x.max(a.x);
        let visible_h = (win.y + strip_h).min(a.y + a.h) - win.y.max(a.y);
        visible_w >= need_w && visible_h >= need_h
    })
}

/// True when the title strip can be grabbed on some monitor. See [`strip_area`].
pub fn title_strip_visible(win: Rect, work_areas: &[Rect], strip_h: i32, min_visible_w: i32) -> bool {
    strip_area(win, work_areas, strip_h, min_visible_w).is_some()
}

/// Size to apply when `win` is larger than the work area it sits on (a size saved on a bigger
/// or lower-DPI monitor), keeping `margin` px free on each side. `None` when it already fits.
pub fn shrink_to_fit(win: Rect, area: Rect, margin: i32) -> Option<(i32, i32)> {
    let max_w = (area.w - 2 * margin).max(1);
    let max_h = (area.h - 2 * margin).max(1);
    if win.w <= max_w && win.h <= max_h {
        return None;
    }
    Some((win.w.min(max_w), win.h.min(max_h)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STRIP: i32 = 36;
    const MIN_W: i32 = 160;
    const PRIMARY: Rect = Rect::new(0, 0, 2560, 1400);
    // Real two-monitor layout from the overlay tests: the secondary starts 87px lower, so the
    // band above it belongs to no screen.
    const SECONDARY: Rect = Rect::new(2560, 87, 1920, 1040);

    #[test]
    fn fully_inside_is_visible() {
        let win = Rect::new(400, 300, 1000, 640);
        assert!(title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
    }

    #[test]
    fn half_off_the_right_edge_is_still_grabbable() {
        let win = Rect::new(2060, 300, 1000, 640);
        assert!(title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
    }

    #[test]
    fn strip_above_the_top_edge_is_unreachable() {
        let win = Rect::new(400, -36, 1000, 640);
        assert!(!title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
    }

    #[test]
    fn strip_partly_above_the_top_edge_counts_when_half_shows() {
        let win = Rect::new(400, -10, 1000, 640);
        assert!(title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
        let win = Rect::new(400, -20, 1000, 640);
        assert!(!title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
    }

    #[test]
    fn one_pixel_inside_is_not_a_handle() {
        let win = Rect::new(2559, 300, 1000, 640);
        assert!(!title_strip_visible(win, &[PRIMARY], STRIP, MIN_W));
    }

    #[test]
    fn no_monitors_means_centre() {
        let win = Rect::new(400, 300, 1000, 640);
        assert!(!title_strip_visible(win, &[], STRIP, MIN_W));
    }

    #[test]
    fn second_monitor_is_found_and_returned() {
        let win = Rect::new(2800, 200, 1000, 640);
        assert_eq!(strip_area(win, &[PRIMARY, SECONDARY], STRIP, MIN_W), Some(SECONDARY));
        // The band above the secondary belongs to nobody: the strip is off-screen there.
        let win = Rect::new(2800, 40, 1000, 640);
        assert!(!title_strip_visible(win, &[PRIMARY, SECONDARY], STRIP, MIN_W));
    }

    #[test]
    fn degenerate_windows_are_never_visible() {
        assert!(!title_strip_visible(Rect::new(0, 0, 0, 640), &[PRIMARY], STRIP, MIN_W));
        assert!(!title_strip_visible(Rect::new(0, 0, 1000, 0), &[PRIMARY], STRIP, MIN_W));
        assert!(!title_strip_visible(Rect::new(0, 0, 1000, 640), &[PRIMARY], 0, MIN_W));
    }

    #[test]
    fn shrink_only_when_the_saved_size_does_not_fit() {
        let area = Rect::new(0, 0, 1920, 1040);
        assert_eq!(shrink_to_fit(Rect::new(0, 0, 1000, 640), area, 12), None);
        assert_eq!(shrink_to_fit(Rect::new(0, 0, 2400, 640), area, 12), Some((1896, 640)));
        assert_eq!(shrink_to_fit(Rect::new(0, 0, 2400, 1400), area, 12), Some((1896, 1016)));
    }
}
