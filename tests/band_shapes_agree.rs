//! Noob-Q's own filter maths, against an independent implementation of the
//! same shapes.
//!
//! `src/dsp/filters.rs` says why this is worth having, in its own words: a
//! high pass once read 0.51 dB away from the true response of its own
//! coefficients, and "nothing caught it because the browser's twin was doing
//! the same arithmetic the same way until it got an independent
//! implementation to disagree with".
//!
//! `noob-band-shapes` is that independent implementation. It is a
//! **dev-dependency only**, and deliberately: it carries the *analogue*
//! prototypes with no sample rate, while Noob-Q draws the response of the
//! digital cascade it actually runs --- warping, Butterworth slopes and the
//! gain-Q rule included. That is the more accurate answer for an equaliser,
//! so the crate is a yardstick here rather than a replacement.
//!
//! Which means the agreement can only be checked where the two are supposed
//! to agree: **well below Nyquist**, where the bilinear transform's warping is
//! small. Up near it they must differ, and by a known amount --- so this also
//! records where the digital filter starts to leave its own prototype, which
//! is a real property of the design and not a fault in either.

use noob_band_shapes::{Kind, magnitude_db};
use noob_q::dsp::filters::{Coefs, Kind as QKind, NEUTRAL_Q, band_magnitude_db, design_band};

const SR: f32 = 48_000.0;
const MAX_STAGES: usize = 16;

/// Noob-Q's response for one band, at one frequency.
fn q_db(kind: QKind, f0: f32, gain: f32, q: f32, at: f32) -> f32 {
    let mut coefs = [Coefs::IDENTITY; MAX_STAGES];
    let n = design_band(kind, f0, gain, q, 0, SR, &mut coefs);
    band_magnitude_db(&coefs[..n], at, SR)
}

/// The five shapes both implementations have.
fn pairs() -> Vec<(QKind, Kind, &'static str)> {
    vec![
        (QKind::Bell, Kind::Bell, "bell"),
        (QKind::LowShelf, Kind::LowShelf, "low shelf"),
        (QKind::HighShelf, Kind::HighShelf, "high shelf"),
        (QKind::TiltShelf, Kind::Tilt, "tilt"),
    ]
}

#[test]
fn the_digital_filters_match_their_analogue_prototypes_below_nyquist() {
    // A tenth of Nyquist and below: warping there is under a per cent in
    // frequency, so any real disagreement is arithmetic rather than the
    // transform.
    let top = SR / 20.0;
    let mut worst = 0.0f32;
    let mut worst_at = ("", 0.0f32, 0.0f32);

    for (qk, bk, name) in pairs() {
        // A bell takes the band's Q straight through. A shelf and a tilt do
        // not: at slope 0 with one section, `shelf_section_qs` returns
        // `NEUTRAL_Q` and **ignores the band's Q entirely**. Sweeping Q for
        // those would be comparing the crate at a Q that Noob-Q never used,
        // which is a test failing for the wrong reason --- it read 3.9 dB
        // apart before this line existed.
        let qs: &[f32] = if qk == QKind::Bell {
            &[0.5, 0.7, 1.4, 4.0]
        } else {
            &[NEUTRAL_Q]
        };
        for &f0 in &[80.0f32, 400.0, 1_000.0] {
            for &gain in &[-9.0f32, -3.0, 3.0, 9.0] {
                for &q in qs {
                    let mut f = 20.0f32;
                    while f < top {
                        let mine = q_db(qk, f0, gain, q, f);
                        let theirs = magnitude_db(bk, f, f0, q, gain);
                        let e = (mine - theirs).abs();
                        if e > worst {
                            worst = e;
                            worst_at = (name, f0, f);
                        }
                        f *= 1.1;
                    }
                }
            }
        }
    }

    println!(
        "worst disagreement {worst:.4} dB, on a {} at {:.0} Hz, measured at {:.0} Hz",
        worst_at.0, worst_at.1, worst_at.2
    );
    // Six hundredths of a decibel, just above the 0.039 this measures.
    // Below a tenth of Nyquist the bilinear transform's error is small but
    // not zero and grows with the corner frequency, so this is not zero and
    // should not be asserted to be --- but it is far tighter than a bound
    // chosen for comfort would have been.
    assert!(
        worst < 0.06,
        "{} at {:.0} Hz differs from its prototype by {worst:.3} dB at {:.0} Hz",
        worst_at.0,
        worst_at.1,
        worst_at.2
    );
}

/// Where the digital filter leaves its prototype, recorded rather than
/// asserted away.
///
/// This is a property of the bilinear transform and not a fault: a digital
/// shelf at a high corner is a different curve from the analogue one it was
/// designed from. Knowing by how much is what stops somebody "fixing" it
/// later.
#[test]
fn where_the_warping_starts_to_show() {
    for (qk, bk, name) in pairs() {
        let f0 = 1_000.0f32;
        let mut printed = false;
        let mut f = 1_000.0f32;
        while f < SR * 0.48 {
            let q = if qk == QKind::Bell { 0.7 } else { NEUTRAL_Q };
            let d = (q_db(qk, f0, 9.0, q, f) - magnitude_db(bk, f, f0, q, 9.0)).abs();
            if d > 0.5 && !printed {
                println!("{name}: half a decibel apart by {f:.0} Hz");
                printed = true;
            }
            f *= 1.15;
        }
        if !printed {
            println!("{name}: within half a decibel all the way to Nyquist");
        }
    }
}
