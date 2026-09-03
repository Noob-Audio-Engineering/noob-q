//! Filter shapes and slopes: the second-order sections one EQ band is made
//! of, and how a band is designed from `(kind, freq, gain, q, slope)`.
//!
//! * Bell, notch, band-pass, all-pass and single shelves are *RBJ Audio EQ
//!   Cookbook* biquads ([`Coefs::rbj`]), the standard closed-form designs
//!   from the bilinear transform of analog prototypes.
//! * Steeper shelves are `order / 2` shelves in series, each with `gain / n`
//!   and the Butterworth section Q of the combined order, so the transition
//!   narrows the way a higher-order shelf's does. Cascading *identical*
//!   shelves does not: N shelves of `gain / N` at one Q reproduce a single
//!   shelf of the full gain, which is what the slope control used to do.
//! * A shelf's Q goes through [`shelf_q`] first. The cookbook's shelf form
//!   puts Q in the denominator of the slope term, so a raw band Q of 40
//!   walks the poles onto the unit circle; the knob is mapped onto a
//!   bounded, useful range instead, as the inventory's §5.7 describes.
//! * Tilt shelves are a low shelf at `−g` and a high shelf at `+g` on the
//!   same corner (`g = gain / 2`), so the two ends differ by `gain` dB and
//!   the corner sits at 0 dB. *Flat Tilt* uses one such pair at a very low
//!   Q so the response is nearly a straight line on a log-frequency axis.
//! * Cuts are Butterworth cascades: `order / 2` second-order sections with
//!   the classic pole Qs ([`butterworth_q`]) plus one first-order section
//!   for odd orders ([`Coefs::one_pole_lp`] / [`Coefs::one_pole_hp`]). The
//!   band's Q scales the Q of the most resonant section — `k = 1`, the
//!   first one [`butterworth_q`] returns — so it shapes the knee the way
//!   Pro-Q's resonant cuts do while the asymptotic slope stays the order's.
//!   "Brickwall" is a 32nd-order Butterworth (192 dB/oct).
//! * At the 6 dB/oct slope the Q control does nothing, on shelves and tilts
//!   as well as cuts, which is what the inventory's §5.3 and §5.7 specify.
//!
//! Coefficients are normalized (`a0 = 1`) and applied by [`Biquad`] in
//! transposed direct form II, two channels per section. The JavaScript twin
//! of this module, `crates/noob-vst-webgui-framework/web/components/eqcurve.js`, uses the same formulas so
//! the curve the page draws is the curve the audio gets; keep the two in
//! step when changing anything here.

use std::f32::consts::{PI, SQRT_2};

/// Largest number of second-order sections one band can need
/// (brickwall cut = order 32 = 16 sections; tilt shelf at 96 dB/oct = 2 × 8).
pub const MAX_STAGES: usize = 16;

/// The shape of a band. Discriminants match [`KIND_NAMES`] and the
/// `b<n>_shape` parameter (0 = Bell … 9 = All Pass).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Kind {
    /// Peaking EQ: `gain` at the centre, unity far away, Q sets the width.
    #[default]
    Bell,
    /// `gain` below the corner, unity above. Uses the slope control.
    LowShelf,
    /// High-pass Butterworth cascade of the slope's order; Q shapes the knee.
    LowCut,
    /// `gain` above the corner, unity below. Uses the slope control.
    HighShelf,
    /// Low-pass Butterworth cascade of the slope's order; Q shapes the knee.
    HighCut,
    /// Infinitely deep dip at the centre; Q sets the width. No gain.
    Notch,
    /// Only the region around the centre passes; Q sets the width. No gain.
    BandPass,
    /// Lows down and highs up (or the reverse) around the corner; `gain` is
    /// the difference between the two ends. Uses the slope control.
    TiltShelf,
    /// A tilt whose response is a straight line on a log-frequency axis.
    FlatTilt,
    /// Phase shift without a gain change (an alternative to polarity flip).
    AllPass,
}

/// UI labels, indexed like [`Kind`].
pub const KIND_NAMES: [&str; 10] = [
    "Bell",
    "Low Shelf",
    "Low Cut",
    "High Shelf",
    "High Cut",
    "Notch",
    "Band Pass",
    "Tilt Shelf",
    "Flat Tilt",
    "All Pass",
];

impl Kind {
    /// Inverse of `kind as usize`; out-of-range indices give `Bell`.
    pub fn from_index(i: usize) -> Kind {
        match i {
            1 => Kind::LowShelf,
            2 => Kind::LowCut,
            3 => Kind::HighShelf,
            4 => Kind::HighCut,
            5 => Kind::Notch,
            6 => Kind::BandPass,
            7 => Kind::TiltShelf,
            8 => Kind::FlatTilt,
            9 => Kind::AllPass,
            _ => Kind::Bell,
        }
    }
    /// The gain control applies (and so can dynamics).
    pub fn has_gain(self) -> bool {
        matches!(
            self,
            Kind::Bell | Kind::LowShelf | Kind::HighShelf | Kind::TiltShelf | Kind::FlatTilt
        )
    }
    /// Low cut or high cut (Butterworth cascades; the Q shapes the knee).
    pub fn is_cut(self) -> bool {
        matches!(self, Kind::LowCut | Kind::HighCut)
    }
    /// The slope control applies (cuts, shelves and the tilt shelf).
    pub fn uses_slope(self) -> bool {
        matches!(
            self,
            Kind::LowCut | Kind::HighCut | Kind::LowShelf | Kind::HighShelf | Kind::TiltShelf
        )
    }
}

/// Slope labels for the `b<n>_slope` parameter. Each maps to a filter order
/// in [`SLOPE_ORDERS`] (order = dB/oct ÷ 6; "Brickwall" is approximated by
/// a 32nd-order Butterworth, 192 dB/oct).
pub const SLOPE_NAMES: [&str; 10] = [
    "6 dB",
    "12 dB",
    "18 dB",
    "24 dB",
    "30 dB",
    "36 dB",
    "48 dB",
    "72 dB",
    "96 dB",
    "Brickwall",
];
/// Filter order per slope index. For shelves the order is halved and
/// clamped to 1..=8 cascaded shelves, so 6 dB and 12 dB both give one.
pub const SLOPE_ORDERS: [usize; 10] = [1, 2, 3, 4, 5, 6, 8, 12, 16, 32];

/// Normalized biquad coefficients (`a0 = 1`) of
/// `H(z) = (b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`.
/// A first-order section is stored with `b2 = a2 = 0`.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Coefs {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

/// The RBJ cookbook prototypes [`Coefs::rbj`] can design.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rbj {
    /// Peaking EQ with gain.
    Peak,
    /// Low shelf with gain.
    LowShelf,
    /// High shelf with gain.
    HighShelf,
    /// Second-order low-pass; the gain argument is ignored.
    LowPass,
    /// Second-order high-pass; the gain argument is ignored.
    HighPass,
    /// Notch; the gain argument is ignored.
    Notch,
    /// Constant-0-dB-peak band-pass; the gain argument is ignored.
    BandPass,
    /// Second-order all-pass; the gain argument is ignored.
    AllPass,
}

impl Coefs {
    /// A section that does nothing (`y = x`).
    pub const IDENTITY: Coefs = Coefs {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// RBJ *Audio EQ Cookbook* design, normalized by `a0`.
    ///
    /// With `ω0 = 2π · freq / sr`, `A = 10^(gain_dB / 40)` and
    /// `α = sin ω0 / (2 Q)`, each prototype's `b`/`a` are the cookbook's
    /// closed forms. `freq` is clamped to `1 Hz ..= 0.499 · sr` and `q` to at
    /// least `1e-3`, so any parameter value yields a stable section. The
    /// shelves use the cookbook's "shelf slope" form with `Q` as the slope
    /// parameter, which gives the familiar resonant bump above `Q ≈ 0.7`.
    pub fn rbj(kind: Rbj, freq: f32, gain_db: f32, q: f32, sr: f32) -> Coefs {
        let w0 = 2.0 * PI * freq.clamp(1.0, sr * 0.499) / sr;
        let (sn, cs) = w0.sin_cos();
        let a = 10f32.powf(gain_db / 40.0);
        let alpha = sn / (2.0 * q.max(1e-3));
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Rbj::LowShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * (a + 1.0 - (a - 1.0) * cs + sq),
                    2.0 * a * (a - 1.0 - (a + 1.0) * cs),
                    a * (a + 1.0 - (a - 1.0) * cs - sq),
                    a + 1.0 + (a - 1.0) * cs + sq,
                    -2.0 * (a - 1.0 + (a + 1.0) * cs),
                    a + 1.0 + (a - 1.0) * cs - sq,
                )
            }
            Rbj::HighShelf => {
                let sq = 2.0 * a.sqrt() * alpha;
                (
                    a * (a + 1.0 + (a - 1.0) * cs + sq),
                    -2.0 * a * (a - 1.0 + (a + 1.0) * cs),
                    a * (a + 1.0 + (a - 1.0) * cs - sq),
                    a + 1.0 - (a - 1.0) * cs + sq,
                    2.0 * (a - 1.0 - (a + 1.0) * cs),
                    a + 1.0 - (a - 1.0) * cs - sq,
                )
            }
            Rbj::LowPass => (
                (1.0 - cs) / 2.0,
                1.0 - cs,
                (1.0 - cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Rbj::HighPass => (
                (1.0 + cs) / 2.0,
                -(1.0 + cs),
                (1.0 + cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Rbj::Notch => (1.0, -2.0 * cs, 1.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            Rbj::BandPass => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            Rbj::AllPass => (
                1.0 - alpha,
                -2.0 * cs,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Rbj::Peak => (
                1.0 + alpha * a,
                -2.0 * cs,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cs,
                1.0 - alpha / a,
            ),
        };
        Coefs {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// First-order low-pass, 6 dB/oct: the bilinear transform of
    /// `1 / (1 + s)` with `k = tan(π · freq / sr)`, giving
    /// `b0 = b1 = k / (1 + k)`, `a1 = (k − 1) / (1 + k)`. −3 dB at `freq`.
    pub fn one_pole_lp(freq: f32, sr: f32) -> Coefs {
        let k = (PI * freq.clamp(1.0, sr * 0.499) / sr).tan();
        let n = 1.0 / (1.0 + k);
        Coefs {
            b0: k * n,
            b1: k * n,
            b2: 0.0,
            a1: (k - 1.0) * n,
            a2: 0.0,
        }
    }

    /// First-order high-pass, 6 dB/oct: the complement of
    /// [`one_pole_lp`](Self::one_pole_lp), `b0 = −b1 = 1 / (1 + k)`.
    pub fn one_pole_hp(freq: f32, sr: f32) -> Coefs {
        let k = (PI * freq.clamp(1.0, sr * 0.499) / sr).tan();
        let n = 1.0 / (1.0 + k);
        Coefs {
            b0: n,
            b1: -n,
            b2: 0.0,
            a1: (k - 1.0) * n,
            a2: 0.0,
        }
    }

    /// `20 · log10 |H(e^{jω})|` at `freq`, evaluated directly from the
    /// coefficients (`|b0 + b1 e^{−jω} + b2 e^{−2jω}| / |1 + a1 e^{−jω} +
    /// a2 e^{−2jω}|`). Used for the response curve, auto gain and the
    /// linear-phase target; cheap enough to call thousands of times per
    /// redesign.
    ///
    /// Accumulated in `f64`. Deep in a cut's stopband the numerator's three
    /// terms very nearly cancel, and in single precision what survives is
    /// mostly rounding error: below about 15 Hz a high-pass read up to
    /// 0.51 dB away from the true response of its own coefficients. That is
    /// a display fault, not an audio one — this runs on redesign, never per
    /// sample — but it made the drawn curve wrong, and nothing caught it
    /// because the browser's twin was doing the same arithmetic the same
    /// way until it got an independent implementation to disagree with.
    pub fn magnitude_db(&self, freq: f32, sr: f32) -> f32 {
        let w = 2.0 * std::f64::consts::PI * freq as f64 / sr as f64;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let (b0, b1, b2) = (self.b0 as f64, self.b1 as f64, self.b2 as f64);
        let (a1, a2) = (self.a1 as f64, self.a2 as f64);
        let nr = b0 + b1 * c1 + b2 * c2;
        let ni = b1 * s1 + b2 * s2;
        let dr = 1.0 + a1 * c1 + a2 * c2;
        let di = a1 * s1 + a2 * s2;
        (10.0 * ((nr * nr + ni * ni).max(1e-300) / (dr * dr + di * di).max(1e-300)).log10()) as f32
    }
}

/// A second-order section with state for two channels (left/right or
/// mid/side, whichever domain the band runs in). Transposed direct form II:
/// two state variables per channel, good numerical behaviour at low
/// frequencies, and coefficients can be swapped between samples without a
/// click as long as the change is small (the engine redesigns per block and
/// the dynamics smooth their gain).
#[derive(Clone, Copy, Default, Debug)]
pub struct Biquad {
    /// The coefficients; the engine writes them directly on redesign.
    pub c: Coefs,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Biquad {
    /// Filter one sample of channel `ch` (0 or 1).
    #[inline]
    pub fn process(&mut self, ch: usize, x: f32) -> f32 {
        let y = self.c.b0 * x + self.z1[ch];
        self.z1[ch] = self.c.b1 * x - self.c.a1 * y + self.z2[ch];
        self.z2[ch] = self.c.b2 * x - self.c.a2 * y;
        y
    }
    /// Clear the state of both channels (used when a section is added to a
    /// cascade or the sample rate changes, never on a plain coefficient
    /// update).
    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }
}

/// Q of the `k`-th (1-based) second-order section of a Butterworth filter of
/// the given order: `1 / (2 sin((2k − 1) π / 2N))`. The sections' pole pairs
/// sit evenly on the unit circle of the analog prototype, which is what
/// gives the maximally flat pass-band; cascading them in this order (least
/// resonant first) keeps intermediate signals small.
pub fn butterworth_q(order: usize, k: usize) -> f32 {
    1.0 / (2.0 * ((2 * k - 1) as f32 * PI / (2.0 * order as f32)).sin())
}

/// The Q of a Butterworth section, and of a shelf with no overshoot:
/// `1/√2`. Both the cut and the shelf paths treat the band's Q as a factor
/// on this, so a band Q of `NEUTRAL_Q` leaves the textbook design alone.
pub const NEUTRAL_Q: f32 = SQRT_2 / 2.0;

/// Top of the range the band's Q knob maps onto for a shelf. The real
/// ceiling on any one section is [`section_q_ceiling`]; this only sets how
/// far the knob travels before that bites.
const SHELF_Q_MAX: f32 = 8.0;

/// Largest Q one shelving section may be designed at, given its own gain.
///
/// The cookbook's shelf form is `α = sin ω₀ / 2Q` feeding a slope term
/// `2√A·α`. As `Q` grows `α → 0`, the denominator's `a₂/a₀ → 1` and the
/// poles reach the unit circle: at the band Q ceiling of 40 a single
/// +30 dB shelf designs a +62 dB peak with a pole radius of 0.996, and an
/// eight-section cascade of those reaches +213 dB and rings for a minute.
///
/// How much overshoot a given Q buys depends on how far the section's gain
/// is from unity, so a gentle section in a long cascade can safely be far
/// more resonant than a single hot shelf. Measured Q for 2 dB of overshoot,
/// against this curve:
///
/// | section gain | 1 | 3 | 6 | 12 | 30 dB |
/// |---|---|---|---|---|---|
/// | measured | 5.04 | 2.33 | 1.64 | 1.31 | 1.15 |
/// | this curve | 3.15 | 2.30 | 1.67 | 1.28 | 1.20 |
///
/// which holds the overshoot of any one section to about 2.3 dB across the
/// whole gain range — a resonant shelf rather than a detonating one. The
/// inventory's §5.7 describes the real product doing the same thing:
/// "shelf Q values are chosen internally for useful shelf shapes".
fn section_q_ceiling(gain_db: f32, sections: usize) -> f32 {
    let one = 1.20 + 2.6 * (-gain_db.abs().max(0.25) / 3.5).exp();
    // Every section of a cascade sits on the same corner, so their peaks
    // land on top of one another and the overshoots add. Share the budget
    // out, or twelve gentle sections at their individual ceiling stack into
    // a 30 dB spike on a band that asked for 1.5 dB of tilt.
    NEUTRAL_Q + (one - NEUTRAL_Q) / (sections as f32).sqrt()
}

/// Map the band's Q knob onto the shelf's Q factor.
///
/// Identity up to [`NEUTRAL_Q`], where the cookbook form is well behaved
/// and where every non-resonant shelf lives, then log-compressed onto
/// `(NEUTRAL_Q, SHELF_Q_MAX]` so the rest of the knob keeps working and
/// stays monotone. A per-section ceiling, derived from each section's own
/// gain, bounds what any section actually gets.
pub fn shelf_q(q: f32) -> f32 {
    let q = q.max(1e-3);
    if q <= NEUTRAL_Q {
        return q;
    }
    // The band parameter's own ceiling; anything above it saturates.
    let q_max = 40.0f32;
    let t = ((q / NEUTRAL_Q).ln() / (q_max / NEUTRAL_Q).ln()).clamp(0.0, 1.0);
    NEUTRAL_Q * (SHELF_Q_MAX / NEUTRAL_Q).powf(t)
}

/// The Q of each section of an `n`-section shelf cascade.
///
/// A shelf gets steeper the way any filter does, by staggering its poles:
/// section `k` takes the Butterworth Q of the combined order `2n`, so the
/// resonant sections sharpen the corner while the damped ones keep the
/// pass-band flat. `n` identical sections instead sum to a single shelf of
/// the full gain and no extra steepness, which is the bug this replaces.
///
/// The band's Q scales the whole cascade through [`shelf_q`], and each
/// section is then held to [`section_q_ceiling`] for its own gain, so the
/// knob keeps working at every slope without any section reaching the unit
/// circle. At the 6 dB/oct slope (`slope == 0`) the Q is locked, per §5.3.
///
/// `section_gain_db` is the gain of one section, not the band's.
fn shelf_section_qs(
    n: usize,
    q: f32,
    slope: usize,
    section_gain_db: f32,
    sections: usize,
) -> [f32; 8] {
    let mut qs = [NEUTRAL_Q; 8];
    let scale = if slope == 0 {
        1.0
    } else {
        shelf_q(q) / NEUTRAL_Q
    };
    let ceiling = section_q_ceiling(section_gain_db, sections);
    for (k, out) in qs.iter_mut().enumerate().take(n) {
        let base = if n == 1 {
            NEUTRAL_Q
        } else {
            butterworth_q(2 * n, k + 1)
        };
        *out = (base * scale).clamp(0.05, ceiling);
    }
    qs
}

/// Design one band. Fills `out` and returns how many sections were used
/// (at most [`MAX_STAGES`]); the caller applies `out[..n]` in series.
///
/// * `kind` — the shape; see [`Kind`] for what each one does with `gain_db`,
///   `q` and `slope`.
/// * `freq` — centre / corner in Hz.
/// * `gain_db` — the band gain (ignored by shapes without gain).
/// * `q` — the band's Q; for cuts it scales the most resonant section so the
///   knee goes from soft (`q < 0.707`) to resonant (`q > 0.707`).
/// * `slope` — index into [`SLOPE_ORDERS`]; ignored by bells, notches,
///   band-passes, all-passes and the flat tilt.
///
/// Pure function of its inputs; the engine caches the inputs and only calls
/// it when one of them changed.
pub fn design_band(
    kind: Kind,
    freq: f32,
    gain_db: f32,
    q: f32,
    slope: usize,
    sr: f32,
    out: &mut [Coefs; MAX_STAGES],
) -> usize {
    let order = SLOPE_ORDERS[slope.min(SLOPE_ORDERS.len() - 1)];
    match kind {
        Kind::Bell => {
            out[0] = Coefs::rbj(Rbj::Peak, freq, gain_db, q, sr);
            1
        }
        Kind::Notch => {
            out[0] = Coefs::rbj(Rbj::Notch, freq, 0.0, q, sr);
            1
        }
        Kind::BandPass => {
            out[0] = Coefs::rbj(Rbj::BandPass, freq, 0.0, q, sr);
            1
        }
        Kind::AllPass => {
            out[0] = Coefs::rbj(Rbj::AllPass, freq, 0.0, q, sr);
            1
        }
        Kind::LowShelf | Kind::HighShelf => {
            let n = (order / 2).clamp(1, 8);
            let rbj = if kind == Kind::LowShelf {
                Rbj::LowShelf
            } else {
                Rbj::HighShelf
            };
            let g = gain_db / n as f32;
            for (k, qk) in shelf_section_qs(n, q, slope, g, n)
                .into_iter()
                .enumerate()
                .take(n)
            {
                out[k] = Coefs::rbj(rbj, freq, g, qk, sr);
            }
            n
        }
        Kind::TiltShelf | Kind::FlatTilt => {
            let flat = kind == Kind::FlatTilt;
            let n = if flat { 1 } else { (order / 2).clamp(1, 8) };
            let g = gain_db / (2.0 * n as f32);
            let qs = if flat {
                [0.18; 8]
            } else {
                shelf_section_qs(n, q, slope, g, 2 * n)
            };
            for (i, qk) in qs.into_iter().enumerate().take(n) {
                out[2 * i] = Coefs::rbj(Rbj::LowShelf, freq, -g, qk, sr);
                out[2 * i + 1] = Coefs::rbj(Rbj::HighShelf, freq, g, qk, sr);
            }
            2 * n
        }
        Kind::LowCut | Kind::HighCut => {
            let n2 = order / 2;
            let odd = order % 2;
            let rbj = if kind == Kind::LowCut {
                Rbj::HighPass
            } else {
                Rbj::LowPass
            };
            for k in 1..=n2 {
                let mut qk = butterworth_q(order, k);
                if k == 1 && slope != 0 {
                    // The band's Q shapes the knee via the most resonant
                    // section, which is `k = 1`. Scaling the *least*
                    // resonant one instead leaves the peak height almost
                    // independent of the order, which is not the control
                    // the inventory describes. Locked at 6 dB/oct (§5.3).
                    qk = (qk * q / NEUTRAL_Q).clamp(0.05, 40.0);
                }
                out[k - 1] = Coefs::rbj(rbj, freq, 0.0, qk, sr);
            }
            if odd == 1 {
                out[n2] = if kind == Kind::LowCut {
                    Coefs::one_pole_hp(freq, sr)
                } else {
                    Coefs::one_pole_lp(freq, sr)
                };
            }
            n2 + odd
        }
    }
}

/// Magnitude of a designed band at `freq`, in dB: the sum of its sections'
/// [`Coefs::magnitude_db`] (magnitudes multiply, so dB add).
pub fn band_magnitude_db(stages: &[Coefs], freq: f32, sr: f32) -> f32 {
    stages.iter().map(|c| c.magnitude_db(freq, sr)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn design(kind: Kind, f: f32, g: f32, q: f32, slope: usize) -> Vec<Coefs> {
        let mut out = [Coefs::IDENTITY; MAX_STAGES];
        let n = design_band(kind, f, g, q, slope, 48000.0, &mut out);
        out[..n].to_vec()
    }

    #[test]
    fn every_slope_is_butterworth_at_the_corner() {
        for (i, order) in SLOPE_ORDERS.iter().enumerate() {
            let s = design(Kind::LowCut, 1000.0, 0.0, SQRT_2 / 2.0, i);
            assert_eq!(s.len(), order / 2 + order % 2);
            let corner = band_magnitude_db(&s, 1000.0, 48000.0);
            assert!((corner + 3.0).abs() < 0.15, "order {order}: {corner}");
            let per_oct =
                band_magnitude_db(&s, 100.0, 48000.0) - band_magnitude_db(&s, 50.0, 48000.0);
            // Each order contributes 6 dB/oct, plus about 0.021 from the
            // bilinear transform warping the poles. That excess is
            // proportional to the order — 0.17 dB at order 8, 0.66 at 32 —
            // so it is asserted per order rather than as a flat window,
            // which is both tighter at low orders and honest at high ones.
            // The old flat 0.6 dB window happened to fit only because the
            // magnitude was accumulated in single precision, where the
            // stopband's catastrophic cancellation pulled the figure back
            // toward the ideal by luck.
            let per_order = per_oct / *order as f32;
            assert!(
                (per_order - 6.0).abs() < 0.025,
                "order {order}: {per_oct} dB/oct is {per_order} per order"
            );
        }
    }

    #[test]
    fn one_pole_is_six_db_per_octave() {
        let c = Coefs::one_pole_lp(1000.0, 48000.0);
        // Well above the corner but away from Nyquist, where the bilinear
        // transform's zero steepens the digital response.
        let d = c.magnitude_db(4000.0, 48000.0) - c.magnitude_db(8000.0, 48000.0);
        assert!(d > 5.5 && d < 7.0, "{d}");
        assert!((c.magnitude_db(1000.0, 48000.0) + 3.0).abs() < 0.1);
        let h = Coefs::one_pole_hp(1000.0, 48000.0);
        let d = h.magnitude_db(250.0, 48000.0) - h.magnitude_db(125.0, 48000.0);
        assert!(d > 5.5 && d < 6.5, "{d}");
    }

    #[test]
    fn tilt_shelf_is_antisymmetric() {
        let s = design(Kind::TiltShelf, 1000.0, 6.0, 0.7, 1);
        let lo = band_magnitude_db(&s, 50.0, 48000.0);
        let hi = band_magnitude_db(&s, 15000.0, 48000.0);
        assert!((lo + 3.0).abs() < 0.3, "{lo}");
        assert!((hi - 3.0).abs() < 0.3, "{hi}");
        assert!(band_magnitude_db(&s, 1000.0, 48000.0).abs() < 0.3);
    }

    #[test]
    fn steep_shelf_reaches_full_gain() {
        let s = design(Kind::HighShelf, 2000.0, 9.0, 0.7, 8);
        assert_eq!(s.len(), 8);
        assert!((band_magnitude_db(&s, 15000.0, 48000.0) - 9.0).abs() < 0.3);
        assert!(band_magnitude_db(&s, 100.0, 48000.0).abs() < 0.3);
    }

    /// Steepest slope of a response over a log sweep, in dB/octave.
    fn max_slope(s: &[Coefs], f0: f32, f1: f32) -> f32 {
        let n = 4000;
        let step = (f1 / f0).powf(1.0 / n as f32);
        let mut worst = 0.0f32;
        let mut f = f0;
        for _ in 0..n {
            let d = (band_magnitude_db(s, f * step, 48000.0) - band_magnitude_db(s, f, 48000.0))
                / step.log2();
            if d.abs() > worst.abs() {
                worst = d;
            }
            f *= step;
        }
        worst.abs()
    }

    /// Largest magnitude anywhere in the audio band, dB.
    fn peak_db(s: &[Coefs]) -> f32 {
        let mut best = f32::NEG_INFINITY;
        let mut f = 5.0f32;
        while f < 23000.0 {
            best = best.max(band_magnitude_db(s, f, 48000.0));
            f *= 1.005;
        }
        best
    }

    /// The largest pole radius of a set of sections. Anything at or above 1
    /// is an unstable filter.
    fn max_pole_radius(s: &[Coefs]) -> f32 {
        s.iter().fold(0.0f32, |worst, c| {
            let disc = c.a1 * c.a1 - 4.0 * c.a2;
            let r = if disc >= 0.0 {
                let d = disc.sqrt();
                (((-c.a1 + d) / 2.0).abs()).max(((-c.a1 - d) / 2.0).abs())
            } else {
                c.a2.abs().sqrt()
            };
            worst.max(r)
        })
    }

    /// The slope control has to change the slope. The old cascade of
    /// identical shelves moved a 16:1 nominal range by 16 %, and the test
    /// that was here asserted only the section count and the asymptotes,
    /// all of which a single shelf satisfies.
    #[test]
    fn slope_steepens_a_shelf() {
        // A shelf can only be as steep as its gain allows: it has to travel
        // the whole gain within the transition, so a 12 dB shelf cannot
        // sustain 96 dB/oct. Measured against a generous gain.
        let gentle = max_slope(
            &design(Kind::HighShelf, 1000.0, 30.0, NEUTRAL_Q, 1),
            20.0,
            20000.0,
        );
        let steep = max_slope(
            &design(Kind::HighShelf, 1000.0, 30.0, NEUTRAL_Q, 8),
            20.0,
            20000.0,
        );
        assert!(
            steep > gentle * 2.0,
            "96 dB shelf {steep:.1} dB/oct is not twice the 12 dB one {gentle:.1}"
        );
        // Monotone in the slope index, allowing for the measurement grid.
        let mut prev = 0.0;
        for i in [1usize, 3, 6, 8] {
            let s = max_slope(
                &design(Kind::LowShelf, 1000.0, 30.0, NEUTRAL_Q, i),
                20.0,
                20000.0,
            );
            assert!(s > prev - 1.0, "slope {i} gave {s:.1} after {prev:.1}");
            prev = s;
        }
    }

    /// Every legal parameter combination has to stay stable and stay in
    /// proportion. Before the shelf Q was bounded, a Low Shelf at the Q
    /// ceiling designed a +213 dB peak with poles at 0.9984 and a full page
    /// of tilt shelves drove the engine's output non-finite.
    #[test]
    fn no_legal_shelf_setting_explodes() {
        for kind in [
            Kind::LowShelf,
            Kind::HighShelf,
            Kind::TiltShelf,
            Kind::FlatTilt,
        ] {
            for &g in &[-30.0f32, -12.0, -3.0, 3.0, 12.0, 30.0] {
                for &q in &[0.025f32, 0.1, NEUTRAL_Q, 1.0, 4.0, 40.0] {
                    for slope in 0..SLOPE_ORDERS.len() {
                        for &f in &[10.0f32, 100.0, 1000.0, 10000.0, 22000.0] {
                            let s = design(kind, f, g, q, slope);
                            let peak = peak_db(&s);
                            let radius = max_pole_radius(&s);
                            assert!(
                                peak.is_finite() && peak < g.abs() + 20.0,
                                "{kind:?} f{f} g{g} q{q} slope{slope}: peak {peak:.1} dB"
                            );
                            assert!(
                                radius < 0.99999,
                                "{kind:?} f{f} g{g} q{q} slope{slope}: pole radius {radius:.6}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// The Q control has to do something at every slope, which the first
    /// bounded design got wrong: a fixed ceiling clamped the whole cascade
    /// and left Q inert above two sections.
    #[test]
    fn shelf_q_bites_at_every_slope() {
        for slope in [1usize, 3, 6, 8] {
            let soft = design(Kind::HighShelf, 1000.0, 12.0, NEUTRAL_Q, slope);
            let sharp = design(Kind::HighShelf, 1000.0, 12.0, 40.0, slope);
            let d = max_slope(&sharp, 20.0, 20000.0) - max_slope(&soft, 20.0, 20000.0);
            assert!(
                d > 2.0,
                "slope {slope}: Q changed the knee by only {d:.1} dB/oct"
            );
        }
    }

    /// §5.3 and §5.7: the Q control is locked at the 6 dB/oct slope, on
    /// shelves and tilts as well as cuts.
    #[test]
    fn q_is_locked_at_six_db_per_octave() {
        for kind in [
            Kind::LowCut,
            Kind::LowShelf,
            Kind::HighShelf,
            Kind::TiltShelf,
        ] {
            let g = if kind.has_gain() { 12.0 } else { 0.0 };
            let lo = design(kind, 1000.0, g, 0.4, 0);
            let hi = design(kind, 1000.0, g, 4.0, 0);
            let mut worst = 0.0f32;
            let mut f = 20.0f32;
            while f < 20000.0 {
                worst = worst.max(
                    (band_magnitude_db(&lo, f, 48000.0) - band_magnitude_db(&hi, f, 48000.0)).abs(),
                );
                f *= 1.02;
            }
            assert!(
                worst < 0.01,
                "{kind:?}: Q moved the response by {worst:.2} dB"
            );
        }
    }

    /// The band's Q scales the *most* resonant section of a cut, which is
    /// `k = 1`, the first Q [`butterworth_q`] returns. Scaling the least
    /// resonant one instead — which is what the code did — left the
    /// resonant peak almost independent of the order (32.04, 32.04, 32.06,
    /// 32.12 dB across four slopes) and contradicted both the module docs
    /// and the README.
    #[test]
    fn cut_q_scales_the_most_resonant_section() {
        for slope in [1usize, 3, 6, 8] {
            let order = SLOPE_ORDERS[slope];
            let base = butterworth_q(order, 1);
            let sharp = design(Kind::LowCut, 1000.0, 0.0, 4.0, slope);
            let soft = design(Kind::LowCut, 1000.0, 0.0, 0.2, slope);
            // Recover each section's Q from its poles: for the RBJ
            // high-pass, `a2 = (1 - α) / (1 + α)` with `α = sin ω₀ / 2Q`.
            let q_of = |c: &Coefs| {
                let alpha = (1.0 - c.a2) / (1.0 + c.a2);
                let w0 = 2.0 * PI * 1000.0 / 48000.0;
                w0.sin() / (2.0 * alpha)
            };
            let (qs, qf) = (q_of(&sharp[0]), q_of(&soft[0]));
            assert!(
                qs > base * 1.5,
                "slope {slope}: a high band Q left section 1 at {qs:.2}, base {base:.2}"
            );
            assert!(
                qf < base * 0.7,
                "slope {slope}: a low band Q left section 1 at {qf:.2}, base {base:.2}"
            );
        }
        // And the composite peak now depends on the order rather than
        // sitting at 32 dB whatever the slope.
        let peaks: Vec<f32> = [1usize, 3, 6, 8]
            .iter()
            .map(|&i| peak_db(&design(Kind::LowCut, 1000.0, 0.0, 40.0, i)))
            .collect();
        let spread = peaks.iter().cloned().fold(f32::MIN, f32::max)
            - peaks.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            spread > 6.0,
            "resonant peak barely moved across orders: {peaks:?}"
        );
    }
}
