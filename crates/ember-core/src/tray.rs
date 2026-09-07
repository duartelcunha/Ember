//! The tray menu popup, decided without I/O: where it goes, and whether a click reopens it.
//!
//! The popup is a webview window of ours, not the Win32 context menu. The shell measures the
//! icon and the monitor; everything that can be wrong about the placement is in here, with tests.

use crate::overlay_geom::Rect;

/// Space between the icon and the popup, in physical px.
pub const GAP: i32 = 8;

/// Clicking the icon while the popup is open blurs the popup FIRST (which hides it) and only
/// then delivers the click, which would open it again on the same gesture. A click this soon
/// after a blur-hide is that gesture: it closed, it does not reopen.
pub const REOPEN_GUARD_MS: u64 = 250;

/// Where the popup goes, and which of its edges faces the icon. The frontend opens the surface
/// out of that edge, so the shape is born where the icon is; a popup forced below a top taskbar
/// that still unfolded upwards would grow AWAY from the thing it came out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    /// The popup sits below the icon (taskbar at the top), so the icon-facing edge is the top.
    pub below: bool,
}

/// Placement of the popup, in the same physical coordinates as `icon` and `work`.
///
/// Above the icon and centred on it, which is where a taskbar at the bottom (the Windows
/// default) wants it. Below it when there is no room above (taskbar at the top). Clamped to the
/// work area both ways, so an icon at the far right of the screen gets a popup that ends at the
/// edge instead of one that hangs off it. The work area excludes the taskbar the icon sits on,
/// which is why "above" is measured from the icon and not from the work area's bottom.
pub fn popup_origin(icon: Rect, popup: (i32, i32), work: Rect) -> Placement {
    let (pw, ph) = popup;
    let x = icon.x + icon.w / 2 - pw / 2;
    let above = icon.y - GAP - ph;
    let below = above < work.y;
    let y = if below { icon.y + icon.h + GAP } else { above };
    let max_x = work.x + (work.w - pw).max(0);
    let max_y = work.y + (work.h - ph).max(0);
    Placement {
        x: x.clamp(work.x, max_x),
        y: y.clamp(work.y, max_y),
        below,
    }
}

/// May a click on the icon open the popup, given how long ago it was last hidden by a blur?
/// `None` means it was never hidden that way (or the last hide was a real close), so yes.
pub fn reopens_after(since_hide_ms: Option<u64>) -> bool {
    since_hide_ms.is_none_or(|ms| ms >= REOPEN_GUARD_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect::new(0, 0, 1920, 1032);
    const POPUP: (i32, i32) = (208, 128);

    #[test]
    fn a_bottom_taskbar_puts_the_popup_above_the_icon_centred_on_it() {
        // Icon in the taskbar strip below the work area.
        let icon = Rect::new(1700, 1040, 24, 24);
        let p = popup_origin(icon, POPUP, WORK);
        assert_eq!(p.x, 1700 + 12 - 104);
        assert_eq!(p.y, 1040 - GAP - 128);
        assert!(!p.below);
    }

    #[test]
    fn a_top_taskbar_puts_the_popup_below_the_icon() {
        let work = Rect::new(0, 48, 1920, 1032);
        let icon = Rect::new(1700, 12, 24, 24);
        let p = popup_origin(icon, POPUP, work);
        // Below the icon would be 44; the work area starts at 48 and wins, so the popup sits
        // flush against the taskbar instead of overlapping it by four pixels.
        assert_eq!(p.y, work.y);
        // And it says so, because the surface has to open from its TOP edge in this layout.
        assert!(p.below);
        assert_eq!(popup_origin(Rect::new(1700, 0, 24, 24), POPUP, work).y, work.y);
    }

    #[test]
    fn an_icon_at_the_edge_gets_a_popup_that_ends_at_the_edge() {
        let icon = Rect::new(1900, 1040, 24, 24);
        assert_eq!(popup_origin(icon, POPUP, WORK).x, 1920 - 208);
        // And a second monitor with a negative origin clamps against ITS edge, not against zero.
        let work = Rect::new(-1920, 0, 1920, 1032);
        let icon = Rect::new(-1910, 1040, 24, 24);
        assert_eq!(popup_origin(icon, POPUP, work).x, -1920);
    }

    #[test]
    fn a_click_right_after_a_blur_hide_is_the_same_gesture() {
        assert!(!reopens_after(Some(0)));
        assert!(!reopens_after(Some(REOPEN_GUARD_MS - 1)));
        assert!(reopens_after(Some(REOPEN_GUARD_MS)));
        assert!(reopens_after(None));
    }
}
