//! sRGB <-> OKLCH conversion, pure and dependency-free.
//!
//! Why OKLCH and not HSL: in HSL, lightness is not perceptual. Shifting it by the same amount
//! makes a yellow look washed out and a blue look muddy, and a project accent is a three-stop
//! gradient built by shifting lightness (see `projects::derive_accent`), so that distortion is
//! exactly what would show on the orb.
//!
//! Why not the `palette` crate: this needs one conversion, and `ember-core` deliberately depends
//! on nothing but `serde`. The matrices below are Björn Ottosson's published OKLab transform and
//! they do not change; the round-trip tests pin them.

/// A colour in OKLCH: perceptual lightness, chroma, and hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    /// 0.0 (black) to 1.0 (white).
    pub l: f64,
    /// 0.0 (grey) upwards. What sRGB can actually show depends on hue and lightness.
    pub c: f64,
    /// Degrees, 0.0 to 360.0.
    pub h: f64,
}

/// Parses `#rrggbb` or `rrggbb`, case-insensitive. `None` for anything else, which is what makes
/// a bad value in a hand-edited config fall back to the indexed palette instead of painting the
/// orb black.
pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let t = s.trim().trim_start_matches('#');
    if t.len() != 6 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&t[i..i + 2], 16).ok();
    Some((byte(0)?, byte(2)?, byte(4)?))
}

/// `#rrggbb`, lowercase. The format the config and the CSS variables already use.
pub fn to_hex(rgb: (u8, u8, u8)) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

pub fn to_oklch(rgb: (u8, u8, u8)) -> Oklch {
    let r = srgb_to_linear(rgb.0 as f64 / 255.0);
    let g = srgb_to_linear(rgb.1 as f64 / 255.0);
    let b = srgb_to_linear(rgb.2 as f64 / 255.0);

    let l_ = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m_ = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s_ = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();

    let l = 0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_;
    let a = 1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_;
    let bb = 0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_;

    let c = (a * a + bb * bb).sqrt();
    // Hue of a grey is meaningless; report 0 instead of whatever atan2 makes of the noise, so a
    // derived ramp from a grey stays grey rather than drifting to a random hue.
    let h = if c < 1e-6 {
        0.0
    } else {
        bb.atan2(a).to_degrees().rem_euclid(360.0)
    };
    Oklch { l, c, h }
}

/// The raw inverse: may land outside sRGB, in which case the channels come back outside 0..1.
/// Callers that need a displayable colour go through [`to_srgb_in_gamut`].
fn to_linear_rgb(v: Oklch) -> (f64, f64, f64) {
    let (a, b) = (
        v.c * v.h.to_radians().cos(),
        v.c * v.h.to_radians().sin(),
    );
    let l_ = v.l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = v.l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = v.l - 0.0894841775 * a - 1.2914855480 * b;

    let l = l_ * l_ * l_;
    let m = m_ * m_ * m_;
    let s = s_ * s_ * s_;

    (
        4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s,
    )
}

fn fits_srgb(v: Oklch) -> bool {
    let (r, g, b) = to_linear_rgb(v);
    // Half a byte, measured in sRGB and not in linear light: what matters is whether the value
    // quantises to 0..255 without being clipped, and the same linear epsilon means very different
    // things at the dark end and the bright end of the curve.
    let eps = 0.5 / 255.0;
    let ok = |c: f64| {
        let s = linear_to_srgb(c);
        s >= -eps && s <= 1.0 + eps
    };
    ok(r) && ok(g) && ok(b)
}

/// Converts to sRGB bytes, pulling chroma down until the colour fits the gamut.
///
/// Clamping each channel on its own would be simpler and wrong: it shifts hue, and the whole point
/// of using OKLCH here is that the three stops of an accent keep the same hue. Reducing chroma
/// keeps hue and lightness and only gives up saturation, which is the one of the three nobody
/// notices at orb size.
pub fn to_srgb_in_gamut(v: Oklch) -> (u8, u8, u8) {
    // The colour already fits: return it untouched. Without this the search below could only ever
    // converge towards the requested chroma from underneath, never reach it, and colours that sit
    // on the gamut boundary lost visible saturation. Pure blue came back as #0030e6.
    if fits_srgb(v) {
        return quantise(to_linear_rgb(v));
    }
    let mut lo = 0.0;
    let mut hi = v.c;
    if !fits_srgb(Oklch { c: 0.0, ..v }) {
        // Lightness alone is already outside the gamut (below black or above white). Nothing to
        // negotiate; clamp it and take the grey.
        let l = v.l.clamp(0.0, 1.0);
        return quantise(to_linear_rgb(Oklch { l, c: 0.0, h: v.h }));
    }
    // 24 halvings take the chroma error below what a single byte can represent, so this cannot
    // loop and cannot leave a visible seam.
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        if fits_srgb(Oklch { c: mid, ..v }) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    quantise(to_linear_rgb(Oklch { c: lo, ..v }))
}

fn quantise((r, g, b): (f64, f64, f64)) -> (u8, u8, u8) {
    let to_byte = |c: f64| (linear_to_srgb(c).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_byte(r), to_byte(g), to_byte(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing_accepts_both_forms_and_refuses_junk() {
        assert_eq!(parse_hex("#b4512a"), Some((0xb4, 0x51, 0x2a)));
        assert_eq!(parse_hex("B4512A"), Some((0xb4, 0x51, 0x2a)));
        assert_eq!(parse_hex("  #b4512a  "), Some((0xb4, 0x51, 0x2a)));
        for bad in ["", "#", "red", "#12", "#1234567", "#gggggg", "#b4512"] {
            assert_eq!(parse_hex(bad), None, "should have refused {bad:?}");
        }
    }

    #[test]
    fn round_trip_survives_a_byte() {
        // Every colour that came from sRGB has to come back as itself: the whole ramp is built by
        // converting out and back, so an error here would show on every derived accent.
        for hex in [
            "#000000", "#ffffff", "#808080", "#ff0000", "#00ff00", "#0000ff", "#b4512a", "#fd8c3c",
            "#ffd9a8", "#14b8a6", "#8b5cf6", "#eab308", "#06b6d4", "#7c3f2d",
        ] {
            let rgb = parse_hex(hex).unwrap();
            let back = to_srgb_in_gamut(to_oklch(rgb));
            let close = |a: u8, b: u8| (a as i16 - b as i16).abs() <= 1;
            assert!(
                close(rgb.0, back.0) && close(rgb.1, back.1) && close(rgb.2, back.2),
                "{hex} came back as {}",
                to_hex(back)
            );
        }
    }

    #[test]
    fn black_and_white_stay_black_and_white() {
        let black = to_oklch((0, 0, 0));
        assert!(black.l.abs() < 1e-6);
        assert_eq!(to_srgb_in_gamut(black), (0, 0, 0));
        let white = to_oklch((255, 255, 255));
        assert!((white.l - 1.0).abs() < 1e-3, "white lightness was {}", white.l);
        assert_eq!(to_srgb_in_gamut(white), (255, 255, 255));
    }

    #[test]
    fn a_grey_reports_no_hue_instead_of_noise() {
        // atan2 on rounding noise would hand back an arbitrary hue, and a ramp derived from grey
        // would then drift into a colour nobody asked for.
        let grey = to_oklch((128, 128, 128));
        // Not exactly zero: the matrices are decimal approximations, so a true grey comes out with
        // chroma around 1e-8. Anything at that scale is invisible; what matters is that the hue is
        // reported as absent rather than as whatever atan2 makes of the noise.
        assert!(grey.c < 1e-6, "grey had chroma {}", grey.c);
        assert_eq!(grey.h, 0.0);
    }

    #[test]
    fn an_impossible_chroma_gives_up_saturation_and_keeps_the_hue() {
        // No sRGB colour is this saturated. Per-channel clamping would shift the hue; pulling
        // chroma down must not.
        let asked = Oklch { l: 0.55, c: 0.5, h: 250.0 };
        let got = to_srgb_in_gamut(asked);
        let back = to_oklch(got);
        assert!(back.c < asked.c, "chroma was not reduced: {}", back.c);
        let drift = (back.h - asked.h).abs().min(360.0 - (back.h - asked.h).abs());
        assert!(drift < 3.0, "hue drifted by {drift} degrees");
    }

    #[test]
    fn lightness_outside_the_gamut_clamps_to_a_grey_instead_of_panicking() {
        assert_eq!(to_srgb_in_gamut(Oklch { l: -0.5, c: 0.1, h: 30.0 }), (0, 0, 0));
        assert_eq!(
            to_srgb_in_gamut(Oklch { l: 1.8, c: 0.1, h: 30.0 }),
            (255, 255, 255)
        );
    }
}
