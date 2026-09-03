/**
 * The browser's model of the engine's static response.
 *
 * The drawn curve has to predict what the user hears, and until now it did
 * not: `EqCurve` was handed the raw per-band handles, so Gain Scale and Auto
 * Gain — both of which the engine applies — never reached it. With one
 * +9 dB bell at 200 % the audio was +18 dB and the page drew +9.
 *
 * Everything here mirrors `src/dsp/engine.rs`, function for function:
 *
 * | here | there |
 * |---|---|
 * | `scaledGainDb`  | `Band::static_db`, `gain_db * g.gain_scale` for gain-bearing shapes only |
 * | `staticSumDb`   | `Engine::static_sum_db`, enabled bands only, frequency clamped below Nyquist |
 * | `autoGainDb`    | `Engine::update_auto_gain`, minus the mean over 64 log-spaced points |
 * | `responseDb`    | `Engine::response_db`, the sum plus auto gain, zero when bypassed |
 *
 * The filter maths itself is the framework's `bandCoefs` / `bandDb`, which
 * is the same model `EqCurve` draws with, so this file only adds the global
 * stages around it. `web/test/curve.test.js` checks the whole thing against
 * the engine's own `curve` stream, which is what stops the two halves
 * drifting apart again.
 *
 * Gain Scale reaches the filter *design*, not just the output: the engine
 * scales the gain before choosing coefficients, and passes the scaled gain
 * to `effective_q`, so a scaled band is a different shape rather than the
 * same shape at a different level. Auto Gain is a flat offset on the
 * composite and changes no band.
 */
import { bandCoefs, bandDb } from '@noob-audio-engineering/noob-vst-webgui-framework/components';

/** Shapes whose gain control does anything, by `SHAPE_IDS` name. Mirrors `Kind::has_gain`. */
export const GAIN_SHAPE_IDS = new Set(['peak', 'lowshelf', 'highshelf', 'tiltshelf', 'flattilt']);

/** Points in the auto-gain mean, and its range. Mirrors `Engine::update_auto_gain`. */
export const AUTO_GAIN_POINTS = 64;
export const AUTO_GAIN_MIN_HZ = 20;
export const AUTO_GAIN_MAX_HZ = 20_000;

/**
 * A band's gain after Gain Scale, in dB. Shapes without a gain control read
 * zero, as they do in the engine.
 * @param {string} type A `SHAPE_IDS` / `FilterTypes` id.
 * @param {number} gainDb The band's own gain.
 * @param {number} gainScale The global scale as a factor (1 = 100 %).
 * @returns {number} dB
 */
export function scaledGainDb(type, gainDb, gainScale = 1) {
  return GAIN_SHAPE_IDS.has(type) ? gainDb * gainScale : 0;
}

/**
 * Sum of every enabled band's static magnitude at `freq`, dB. Dynamics,
 * placement and output gain are all excluded, exactly as in the engine.
 * @param {number} freq Hz
 * @param {{type:string,freq:number,gain:number,q:number,slope:number,enabled:boolean}[]} bands
 * @param {number} sampleRate
 * @param {{gainQ?:boolean, gainScale?:number}} [opts]
 * @returns {number} dB
 */
export function staticSumDb(freq, bands, sampleRate, opts = {}) {
  const { gainQ = false, gainScale = 1 } = opts;
  const f = Math.min(freq, sampleRate * 0.499);
  let db = 0;
  for (const b of bands) {
    if (b.enabled === false) continue;
    const gain = scaledGainDb(b.type, b.gain, gainScale);
    db += bandDb(bandCoefs(b.type, b.freq, gain, b.q, b.slope, sampleRate, { gainQ }), f, sampleRate);
  }
  return db;
}

/**
 * The Auto Gain make-up in dB: minus the mean static response over 64
 * log-spaced points from 20 Hz to 20 kHz, or zero when it is off.
 * @param {object[]} bands See `staticSumDb`.
 * @param {number} sampleRate
 * @param {{gainQ?:boolean, gainScale?:number, autoGain?:boolean}} [opts]
 * @returns {number} dB
 */
export function autoGainDb(bands, sampleRate, opts = {}) {
  if (!opts.autoGain) return 0;
  const n = AUTO_GAIN_POINTS;
  const ratio = AUTO_GAIN_MAX_HZ / AUTO_GAIN_MIN_HZ;
  let sum = 0;
  for (let i = 0; i < n; i++) {
    sum += staticSumDb(AUTO_GAIN_MIN_HZ * Math.pow(ratio, i / (n - 1)), bands, sampleRate, opts);
  }
  return -(sum / n);
}

/**
 * The engine's static response at `freq`: every enabled band, scaled, plus
 * the auto-gain make-up. Zero everywhere when bypassed.
 * @param {number} freq Hz
 * @param {object[]} bands See `staticSumDb`.
 * @param {number} sampleRate
 * @param {{gainQ?:boolean, gainScale?:number, autoGain?:boolean, bypass?:boolean}} [opts]
 * @returns {number} dB
 */
export function responseDb(freq, bands, sampleRate, opts = {}) {
  if (opts.bypass) return 0;
  return staticSumDb(freq, bands, sampleRate, opts) + autoGainDb(bands, sampleRate, opts);
}
