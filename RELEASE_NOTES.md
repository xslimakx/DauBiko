## DauBiko 1.0.6

- Захист від NaN / inf від хоста: один «битий» семпл більше не ламає плагін назавжди (раніше фільтри залишались у NaN і звук пропадав до перезапуску).
- Оверсемплінг сатурації (Drive): новий поліфазний half-band фільтр, рівний до 20 кГц, аліасинг > 100 дБ нижче.

## DauBiko 1.0.5

- **True-peak safe limiter:** the limiter watched only the samples, so on hot, bright material the waveform between them could still go over the ceiling (+0.1 dBTP with the ceiling at -0.3). It now also checks the points between the samples (4x); measured true peak stays within 0.05 dB of the ceiling. Latency grows by 4 samples (reported to the host).
- New tests: true peak under the ceiling, the width control keeps the mono sum exactly.

## DauBiko 1.0.4

- **Blank GUI after reopening fixed:** after closing the plugin window and opening it again, the photo, knobs, VU meters, switches and lamps came up empty (only text and lines were drawn). The pictures belonged to the previous window; they are now loaded again for every new window.

## DauBiko 1.0.3

- **LED ring:** the dots around DYNAMICS are now perfectly round and identical (they were drawn as tiny polygons and came out slightly oval or crooked).
- **Alt-tab fix (Linux):** after alt-tab, minimising or covering the window, parts of the picture (backgrounds, knobs, images) could stay blank until something on them moved. The window is now drawn again in full whenever the system throws its contents away, and pictures are no longer lost while the window is hidden or a frame is skipped.
- Smoother images in the CPU renderer (used when Vulkan is not available): pictures drawn smaller than their real size, such as knobs and photos, are now scaled with mipmaps instead of picking single pixels, so they no longer look grainy or jagged.

# DauBiko v1.0.2

- **Display fix:** the built-in CPU renderer (used when Vulkan is not available) drew pixels on the shared edges of triangles twice, which showed as brighter seams and specks in see-through fills (spectra, curves, bars). It now uses an exact fill rule like a GPU: every pixel is drawn once, without holes.
- Clicking a value box and pressing Enter no longer changes a value whose unit matters (for example "1.03 s" was read back as 1.03 ms).
- New automatic test: every parameter's text is typed back over its whole range and must give the same value.

# DauBiko v1.0.1

- Engraved copper name plate (no longer touches the corner screw)
- Knob shadows are no longer cut off (DYNAMICS and the small knobs)
- Knob labels sit clear of the centre marks; MONO / TUBE switches line up with the knob labels
- OUTPUT: LIMIT button, limiter GR meter and peak read-out in one tidy row
- Calmer panel edges

# DauBiko v1.0.0

First release of DauBiko, an analog-style mastering processor.

- Tone: low cut, LOW, BODY, PRESENCE, AIR and TILT with a live curve on the amber screen
- DYNAMICS: one knob from GLUE (bus compression with auto makeup) to PUNCH (transient lift, tail control), 1 ms lookahead
- COLOR: tape or tube saturation, 2x oversampled
- STEREO: width 0 to 200 % and bass mono below 120 Hz
- OUTPUT: gain and a 1.5 ms lookahead limiter with ceiling
- Worn analog console look: brushed, scratched and finger-polished metal, machined aluminium knobs, chrome toggles, glass lamps, aged backlit VU meters
- Live output spectrum behind the tone curve on the amber screen
- Gain change and limiter LEDs, resizable window
- Passes the Steinberg VST3 validator (537 / 537 tests)
