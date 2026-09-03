# Noob-Q vs. the Pro-Q 4 manual

> I wrote Noob-Q as a humorous, affectionate spoof of Pro-Q 4 to exercise
> noob-vst-webgui-framework. This document tracks which features of the manual it
> imitates and how; it is not a claim of parity with, or a replacement for,
> FabFilter's product.

The inventory in `PROQ4-FEATURES.md` lists every feature the manual describes.
This is what Noob-Q does with each. **Done** means implemented as described;
**approx.** means the behaviour is there but the algorithm is simpler than
FabFilter's; **no** means not implemented.

## Bands and shapes (§5)

| feature | status | notes |
|---|---|---|
| Up to 24 bands, numbered on creation, no renumbering on delete | done | band N is parameter group `bN_*` |
| Bell, Low/High Shelf, Low/High Cut, Notch, Band Pass, Tilt Shelf, Flat Tilt, All Pass | done | RBJ biquads; steep shelves are cascades; Flat Tilt is a very wide tilt shelf |
| Slopes 6–96 dB/oct + Brickwall | done (stepped) | fixed steps only, no fractional slopes; Brickwall ≈ 192 dB/oct Butterworth |
| Slope applies to cuts and to shelves / tilt | done | on shelves the sections are staggered to the Butterworth Qs of the combined order. A shelf can only be as steep as its gain allows, since it has to travel the whole gain within the transition: a 30 dB shelf reaches about 30 dB/oct at the 96 dB setting, a 12 dB one about 15 |
| Frequency 10 Hz – 30 kHz, gain ±30 dB, Q 0.025 – 40 (Q = 1 default) | done | |
| Gain-Q interaction (Bell) | approx. | `Q · (1 + |gain| / 30)`, global toggle |
| Q locked at 6 dB/oct | done | on shelves and tilts as well as cuts |
| Bypass, solo (hold), delete, previous / next band, split L+R / M+S | done | |
| Stereo placement Left / Right / Stereo / Mid / Side with the manual's colours | done | |
| Copy / paste bands | done, within one instance | not the browser clipboard and not across instances: the buffer is a value inside one page's JavaScript, and each instance runs its own web view. Copying the whole plug-in state does use the browser clipboard |
| Surround / Atmos speaker panel | no | stereo and mono only |

## Display and gestures (§3, §28)

| feature | status |
|---|---|
| Create by dragging the yellow curve, double-click / click empty space, curve previews on hover, shape by position | done |
| `Alt` creates a dynamic band | done |
| `Alt+Shift` creates a spectral band | no (creates a dynamic band) |
| Select, `Ctrl`+click multi-select, `Shift`+click range, `Shift`+drag rectangle, click background to deselect | done |
| Drag = frequency + gain (multi-band moves together), `Ctrl`+drag = Q, `Alt`+drag constrains an axis, `Shift` = fine | done |
| Wheel = Q (slope steps on cuts), `Ctrl`+wheel = gain, `Alt`+wheel = dynamic range, `Alt+Ctrl`+wheel linked | done |
| `Alt`+click bypass, `Ctrl+Alt`+click cycle shape, `Alt+Shift`+click cycle slope, double-click to type values, right-click menu | done |
| EQ parameter display (values draggable / typeable, `Tab` steps through them) | done |
| Display ranges ±3 / 6 / 12 / 30 dB, auto-adjust | done |
| Horizontal zoom / scroll on the frequency scale, double-click to reset | done |
| Piano display A0–C8, click to quantise, drag on notes, note names with cents, note-name text entry | done |
| Show frequency on hover | done |
| Full screen | done (browser full screen) |
| Resize menu Mini … Extra Large | done | sends a `resize` request to the host via the adapter |
| Scaling submenu | no | the web view follows OS DPI |

## EQ Sketch (§4)

Drawing left-to-right on an empty preset (or after arming the pencil button)
fits one band per excursion from 0 dB: cuts at the edges, shelves near the
edges, bells elsewhere with Q from the excursion width; moving back erases.
**approx.**

## Dynamic EQ (§6)

| feature | status | notes |
|---|---|---|
| Dynamic range ring (±30 dB), positive = expand, negative = compress | done | |
| Threshold: automatic or manual, live trigger level shown | done | auto = 3 dB above the region's running average |
| Attack / release | done (ms) | manual gives 0–100 % with 50 % = auto; here explicit milliseconds. The gain is recomputed once per block, so one block is the finest timing that can be resolved; the times themselves come from the per-sample envelope and no longer follow the host's buffer size |
| External side-chain trigger per band | done | plug-in exposes a stereo side-chain input |
| Band / Free trigger filter | no | trigger is always the band's own region |
| Audition trigger, dynamics bypass, clear dynamics | done | audition uses the band's solo path |
| Program-dependent knee | approx. | fixed 12 dB soft knee |
| Dynamic gain shown on the node (indicator bar + moving dot) | done | streamed from the DSP every block |
| Not available at Very High / Maximum linear-phase resolution (warning) | done | |

## Spectral dynamics (§7)

**no.** Per-bin spectral processing is not implemented; "Make Spectral" is
greyed out.

## Processing modes (§15)

| feature | status | notes |
|---|---|---|
| Zero Latency | done | minimum-phase IIR |
| Natural Phase | approx. | same IIR path (no analog phase matching) |
| Linear Phase with Low … Maximum resolutions and reported latency | done | FIR designed from the response, partitioned FFT convolution; single stage in L/R or M/S, two stages only when both L/R-specific and M/S-specific bands exist (doubled latency, as the manual says) |
| Zipper-free frequency changes in linear phase | approx. | FIR redesign at most every second block |
| Oversampling | no, but the saturator is band-limited | the manual describes no oversampling *control*, which says nothing about the internal rate, so the old reason here was a non-sequitur. The *Character* stage now uses antiderivative anti-aliasing rather than a plain `tanh` at the sample rate. Measured at 48 kHz on a full-scale sine, worst alias below the fundamental: Warm at 9 kHz 13.5 dB before, 19.5 dB after; Subtle at 9 kHz 17.6 before, 24.8 after. First-order anti-aliasing cannot do much better that close to Nyquist, and the rest would need a resampler and its latency |

## Character (§16)

Clean / Subtle / Warm: **approx.** (tanh saturation, Warm adds even harmonics).

## Spectrum analyzer (§17)

| feature | status |
|---|---|
| Pre / Post / SC (side-chain) spectra | done |
| Range 60 / 90 / 120 dB, resolution 1024–8192, speed, tilt (default 4.5 dB/oct) | done |
| Freeze (click) and temporary freeze (hold), peak hold while frozen | done |
| Spectrum Grab: hover to grab, drag a peak into a Bell; permanent grab (`G` key / click-hold) | done |
| External spectrum from other instances, Show Collisions | no, and no control is drawn | the disabled placeholder has been removed rather than left on the panel |

## EQ Match (§18)

Two-step panel: average the input and a reference (side-chain, or a saved
spectrum), difference curve, choose the number of bands, Finish adds them.
**approx.** (greedy Bell fitting; no audio-file loading).

## Output (§20)

| feature | status |
|---|---|
| Global bypass (keeps latency), phase invert, auto gain (estimated, static) | done |
| Output gain −60 … +36 dB, gain scale 0–200 %, drag the output button | done |
| Output pan with L/R or M/S mode | done |
| Output meter with clip indication, toggleable | done |

## Presets, history, help (§22, §24, §26)

| feature | status | notes |
|---|---|---|
| Undo / redo, A/B, copy A→B | done | `@noob-audio-engineering/noob-vst-webgui-framework`'s `History`, snapshot per gesture |
| Preset browser: folders, search, favourites, details, Save As, prev / next, keyboard navigation | done | user presets persist in the web view's storage, not in files |
| Copy / paste the whole plug-in state | done | |
| Help menu options (parameter display, auto range, frequency on hover) | done | |
| MIDI Learn | no, and no control is drawn | map controllers in your host; the disabled placeholder has been removed |
| Instance list | partly done | the bottom bar lists the other running instances with their port and process id and opens them, over the framework's `/instances` endpoint. What is missing is acting on another instance from here, not the communication itself |
| Collision detection | no | |
| iOS gestures | n/a | |

## Framework support these needed (`noob-vst-webgui-framework` / `@noob-audio-engineering/noob-vst-webgui-framework` / adapters)

* Batched multi-parameter edits in one frame (`client.setMany`) and whole-state snapshots.
* `History` (undo / redo / A/B) on top of snapshots.
* Editor resize requests from the page (`resize` message → host `request_resize`).
* Table tapers, so any nih-plug parameter range renders correctly.
* Per-client stream throttling / disabling, so hidden analyzers cost nothing.
* Embedded assets via `include_dir` (`Assets::Lookup`).
