/**
 * The browser's response model, checked against the engine's own.
 *
 * `src/curveModel.js` and `src/dsp/engine.rs` implement the same maths in
 * two languages, and `src/dsp/filters.rs` says they must be kept "in step".
 * Nothing was keeping them there: this plug-in had no JavaScript tests at
 * all, which is how the drawn curve came to ignore Gain Scale and Auto Gain
 * and disagree with the audio by up to 9 dB while looking perfectly
 * plausible on screen.
 *
 * So this compares against the engine's published `curve` stream rather
 * than against expectations written here. An assertion about what the
 * browser thinks proves nothing; only the two halves agreeing does. If the
 * Rust changes shape, these fail, which is the point.
 *
 * Needs a running standalone:
 *
 *   cargo run --release --bin noob-q-standalone
 *   npm test                       # or NOOB_Q_PORT=4242 npm test
 *
 * With no standalone the tests skip rather than fail, so a checkout without
 * a build is not a red suite; CI starts one first.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { NoobVstWebguiFrameworkClient } from '@noob-audio-engineering/noob-vst-webgui-framework';
import { responseDb } from '../src/curveModel.js';

const PORT = process.env.NOOB_Q_PORT || '4242';
const URL = `ws://127.0.0.1:${PORT}/ws`;
/** The curve is a coarse log grid and the two filter cascades are floating point in different languages. */
const TOL_DB = 0.05;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Connect and wait for the manifest, or resolve null when nothing is listening. */
async function connect() {
  let client;
  try {
    client = new NoobVstWebguiFrameworkClient(URL, { autoReconnect: false });
  } catch {
    return null;
  }
  const ready = new Promise((resolve) => {
    const off = client.on('manifest', () => {
      off();
      resolve(true);
    });
    client.on('error', () => resolve(false));
    client.on('close', () => resolve(false));
  });
  const ok = await Promise.race([ready, sleep(4000).then(() => false)]);
  if (!ok || !client.manifest) {
    try {
      client.close();
    } catch {}
    return null;
  }
  return client;
}

const client = await connect();
const skip = client ? false : `no standalone on ${URL}; start one with: cargo run --release --bin noob-q-standalone`;

/** Set a parameter by plain value, as a complete gesture. */
function setPlain(id, plain) {
  const p = client.param(id);
  p.beginEdit();
  p.setPlain(plain);
  p.endEdit();
}

/** Set a labelled parameter by label index. */
function setIndex(id, index) {
  const p = client.param(id);
  const steps = p.spec.steps || (p.spec.labels ? p.spec.labels.length : 0);
  p.beginEdit();
  p.set(steps > 1 ? index / (steps - 1) : 0);
  p.endEdit();
}

/**
 * Wait for a `curve` frame newer than the one in hand, so the comparison
 * uses the engine's answer to the edits just made rather than the previous
 * one. The engine republishes only when its static response changed, so a
 * timeout here is itself a finding (it means an edit that moved the audio
 * did not move the curve).
 */
async function nextCurve(afterSeq) {
  const s = client.stream('curve');
  const t0 = Date.now();
  while (Date.now() - t0 < 4000) {
    if (s.seq > afterSeq && s.data.length) return { data: Float32Array.from(s.data), meta: s.meta, seq: s.seq };
    await sleep(20);
  }
  return null;
}

/** Silence every band, then apply `bands` from band 1 upward. */
function configure(bands, globals = {}) {
  for (let n = 1; n <= 24; n++) setPlain(`b${n}_on`, 0);
  setPlain('gain_scale', globals.gainScale ?? 100);
  setPlain('auto_gain', globals.autoGain ? 1 : 0);
  setPlain('gain_q', globals.gainQ ? 1 : 0);
  setPlain('bypass', 0);
  bands.forEach((b, i) => {
    const n = i + 1;
    setIndex(`b${n}_shape`, b.shapeIndex);
    setPlain(`b${n}_freq`, b.freq);
    setPlain(`b${n}_gain`, b.gain ?? 0);
    setPlain(`b${n}_q`, b.q ?? 1);
    setIndex(`b${n}_slope`, b.slope ?? 1);
    setPlain(`b${n}_dyn_on`, 0);
    setPlain(`b${n}_solo`, 0);
    setPlain(`b${n}_on`, 1);
  });
}

/** The band shapes as `curveModel` wants them. */
const SHAPE_IDS = ['peak', 'lowshelf', 'highpass', 'highshelf', 'lowpass', 'notch', 'bandpass', 'tiltshelf', 'flattilt', 'allpass'];


/** Compare the JS model with an engine curve frame; returns the worst error in dB. */
function worstError(frame, bands, sampleRate, globals) {
  const { data, meta } = frame;
  const n = data.length;
  const model = bands.map((b) => ({
    type: SHAPE_IDS[b.shapeIndex],
    freq: b.freq,
    gain: b.gain ?? 0,
    q: b.q ?? 1,
    slope: b.slope ?? 1,
    enabled: true,
  }));
  const opts = {
    gainQ: !!globals.gainQ,
    gainScale: (globals.gainScale ?? 100) / 100,
    autoGain: !!globals.autoGain,
  };
  let worst = 0;
  let at = 0;
  for (let i = 0; i < n; i++) {
    const f = meta.min_hz * Math.pow(meta.max_hz / meta.min_hz, i / (n - 1));
    const err = Math.abs(responseDb(f, model, sampleRate, opts) - data[i]);
    if (err > worst) {
      worst = err;
      at = f;
    }
  }
  return { worst, at };
}

/** One case: configure the engine, take its curve, compare. */
async function check(name, bands, globals = {}) {
  const sr = client.manifest.meta.sample_rate || 48000;
  const before = client.stream('curve').seq;
  configure(bands, globals);
  const frame = await nextCurve(before);
  assert.ok(frame, `${name}: the engine published no new curve after the edits`);
  const { worst, at } = worstError(frame, bands, sr, globals);
  assert.ok(
    worst <= TOL_DB,
    `${name}: the drawn curve and the engine disagree by ${worst.toFixed(3)} dB at ${at.toFixed(0)} Hz (tolerance ${TOL_DB} dB)`,
  );
}

test('the drawn curve matches the engine', { skip }, async (t) => {
  client.stream('curve').subscribe({ enabled: true });
  await sleep(200);

  const bell = [{ shapeIndex: 0, freq: 1000, gain: 9, q: 1 }];

  // The four cases from the audit: the two globals the page used to ignore.
  await t.test('one bell, nothing global', () => check('plain bell', bell));
  await t.test('gain scale 200 %', () => check('gain scale 200', bell, { gainScale: 200 }));
  await t.test('gain scale 50 %', () => check('gain scale 50', bell, { gainScale: 50 }));
  await t.test('auto gain on', () => check('auto gain', bell, { autoGain: true }));
  await t.test('auto gain and gain scale together', () =>
    check('auto gain + scale', bell, { autoGain: true, gainScale: 200 }));
  await t.test('gain scale 0 %', () => check('gain scale 0', bell, { gainScale: 0 }));

  // Gain-Q interaction changes the shape, and is scaled gain in the engine.
  await t.test('gain-Q interaction', () => check('gain q', [{ shapeIndex: 0, freq: 1000, gain: 18, q: 2 }], { gainQ: true }));
  await t.test('gain-Q with gain scale', () =>
    check('gain q + scale', [{ shapeIndex: 0, freq: 1000, gain: 18, q: 2 }], { gainQ: true, gainScale: 150 }));

  // Every shape.
  for (let s = 0; s < SHAPE_IDS.length; s++) {
    await t.test(`shape ${SHAPE_IDS[s]}`, () =>
      check(SHAPE_IDS[s], [{ shapeIndex: s, freq: 800, gain: 6, q: 1.2, slope: 3 }], { gainScale: 150 }));
  }

  // The extremes of Q and slope, where the cascade maths is most fragile.
  await t.test('lowest and highest Q on a bell', async () => {
    await check('Q min', [{ shapeIndex: 0, freq: 2000, gain: -12, q: 0.1 }]);
    await check('Q max', [{ shapeIndex: 0, freq: 2000, gain: -12, q: 40 }]);
  });
  await t.test('every slope on a low cut', async () => {
    for (let sl = 0; sl < 10; sl++) await check(`low cut slope ${sl}`, [{ shapeIndex: 2, freq: 200, q: 0.7, slope: sl }]);
  });
  await t.test('every slope on a low shelf', async () => {
    for (let sl = 0; sl < 10; sl++) await check(`low shelf slope ${sl}`, [{ shapeIndex: 1, freq: 200, gain: 9, q: 0.7, slope: sl }]);
  });

  // Several bands at once, with both globals engaged. Contains shelves and
  await t.test('a full curve', () =>
    check(
      'many bands',
      [
        { shapeIndex: 2, freq: 60, q: 0.7, slope: 3 },
        { shapeIndex: 1, freq: 180, gain: 4, q: 0.8, slope: 1 },
        { shapeIndex: 0, freq: 900, gain: -7, q: 3 },
        { shapeIndex: 0, freq: 3200, gain: 5, q: 1.4 },
        { shapeIndex: 3, freq: 9000, gain: -3, q: 0.7, slope: 1 },
        { shapeIndex: 4, freq: 16000, q: 0.7, slope: 5 },
      ],
      { autoGain: true, gainScale: 125, gainQ: true },
    ));
});

test.after(() => {
  try {
    client?.close();
  } catch {}
});
