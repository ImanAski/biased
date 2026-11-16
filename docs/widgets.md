# Widget layout guide

The overview page is driven by a single TOML file. **Monitors** are read-only
cards fed by inbound telemetry; **controls** write fields of the device state
that is mirrored to the MCU. The file is presentation and limits only — it
never changes the wire protocol.

- Bundled reference layout: [`src-tauri/widgets.toml`](../src-tauri/widgets.toml)
- Schema and validation: [`src-tauri/src/config.rs`](../src-tauri/src/config.rs)
- Wire format: [`src-tauri/src/protocol.rs`](../src-tauri/src/protocol.rs)

## Where the file lives

On first launch the app copies the bundled layout to its config directory:

| OS      | Path                                                              |
| ------- | ----------------------------------------------------------------- |
| Windows | `%APPDATA%\com.iman.tauri-biased\widgets.toml`                    |
| macOS   | `~/Library/Application Support/com.iman.tauri-biased/widgets.toml` |
| Linux   | `~/.config/com.iman.tauri-biased/widgets.toml`                    |

Upgrading the app does **not** overwrite an existing file, so keep your edits
in the config directory rather than in `src-tauri/widgets.toml`.

In **Settings → Layout**:

- **Reload** re-reads the active file.
- **Open file…** selects another `.toml` file and remembers it across restarts.
- **Use app default** returns to the bundled layout.
- **Save a copy…** writes the active layout somewhere else.

Parse and validation errors are listed in the UI; the last good layout stays
active until the file is fixed. A configuration-locked customer build always
uses its embedded layout and ignores any file on the machine (see the main
[README](../README.md#configuration-locked-customer-builds)).

## Minimal layout

```toml
title = "Bench setup"

[[monitors]]
id = "temp"
source = "temp"
label = "Temperature"
unit = "°C"
variant = "gauge"
range = { min = 0, max = 100 }
threshold = { low = 5, high = 75 }

[[controls]]
id = "dither_amp"
label = "Dither amplitude"
kind = "range"
target = "ditherAmp"
unit = "V"
range = { min = 0, max = 10, step = 0.01 }
presets = [0, 0.5, 1, 5]
```

Keys are **camelCase** (`biasVoltage`, `ditherAmp`, `inputPower`) and enum
values are **snake_case** (`sparkline`, `segmented`). Unknown keys are rejected,
so a typo fails the load instead of being silently ignored. Monitor and control
ids share one namespace. Empty `[[monitors]]` / `[[controls]]` sections are
allowed.

## Monitors

Monitors read the latest telemetry value. A monitor with no data shows `—` and
a `no data` badge. Monitors never write anything.

### Monitor fields

| Key           | Type    | Default  | Notes                                                        |
| ------------- | ------- | -------- | ------------------------------------------------------------ |
| `id`          | string  | required | Unique among monitors and controls; must not be empty.       |
| `source`      | string  | `id`     | Telemetry field, see the source table.                       |
| `label`       | string  | `id`     | Card title.                                                  |
| `variant`     | string  | `bar`    | `bar`, `gauge`, `sparkline`, `led`, `tile`, `meter`.          |
| `unit`        | string  | `""`     | Display suffix only.                                         |
| `digits`      | integer | `1`      | Decimal places, 0–8.                                         |
| `description` | string  | `""`     | Small caption under the value.                               |
| `history`     | integer | `60`     | Samples kept by `sparkline`, 8–600.                          |
| `segments`    | integer | `20`     | Cells drawn by `meter`, 5–40.                                |
| `range`       | table   | none     | `{ min = …, max = … }`, finite `min < max`. `step` is ignored. |
| `threshold`   | table   | none     | `{ low = …, high = … }`, both optional, finite, `low < high`. |

If `source` is omitted it defaults to the monitor `id`; this is handy when the
id is already a source name (`id = "temp"`).

### Telemetry sources

The MCU sends telemetry lines to the reader thread:

```text
TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8 ibias=0.4 vsupply=12.1 phase=1.5 lock=1\n
```

| `source`        | Telemetry keys       | Unit | Meaning                         |
| --------------- | -------------------- | ---- | ------------------------------- |
| `theta0`        | `theta0`, `theta`    | °    | Measured Θ₀ angle               |
| `temp`          | `temp`, `temperature`| °C   | Board temperature               |
| `biasVoltage`   | `vbias`, `bias`      | V    | Bias voltage                    |
| `inputPower`    | `pin`, `input`       | W    | Input power                     |
| `outputPower`   | `pout`, `output`     | W    | Output power                    |
| `biasCurrent`   | `ibias`, `current`   | A    | Current through the bias stage  |
| `supplyVoltage` | `vsupply`, `supply`  | V    | Supply rail                     |
| `phaseError`    | `phase`, `perr`      | °    | Residual phase error            |
| `lockStatus`    | `lock`, `locked`     | 0/1  | Loop lock flag                  |

Unknown telemetry keys are ignored. Use **Packet Lab → Inject** to feed a `TLM`
line without hardware while designing a layout.

### Monitor variants

| `variant`   | Rendering                                                              |
| ----------- | ---------------------------------------------------------------------- |
| `bar`       | Value, horizontal fill bar, value dot and threshold ticks.             |
| `gauge`     | 270° radial gauge with threshold ticks.                                |
| `sparkline` | Rolling trend chart that keeps the last `history` samples.             |
| `led`       | Large ON/OFF lamp; ON when the value is ≥ 0.5.                         |
| `tile`      | Compact numeric readout with a status badge and limit text.            |
| `meter`     | Segmented level meter with `segments` cells and threshold marks.       |

`range` provides the display scale for `bar`, `gauge` and `meter`; without it
those variants still show the numeric value but draw no fill. `sparkline` uses
`range` as a fixed Y axis and otherwise auto-scales to the samples it has.
`tile`, `led` and the numeric readouts do not need a range.

Thresholds are strict comparisons: a value **below** `low` is `LOW`, a value
**above** `high` is `HIGH`, and a value exactly on a limit is nominal. Alerts
turn the card border amber; they do not change the transmitted data.

The `unit`, `digits`, `label` and `description` fields affect display only.
Values outside `range` are clamped for the bar/arc/meter fill but the numeric
readout always shows the real value.

## Controls

Controls edit the in-memory device snapshot. The serial loop sends a complete
`STATE` frame every 300 ms, so a change takes effect on the next tick. Numeric
values are clamped to the control range and snapped to `step`; `mode` is also
rounded to an integer and clamped to 0–255.

### Control fields

| Key           | Type    | Default  | Notes                                                         |
| ------------- | ------- | -------- | ------------------------------------------------------------- |
| `id`          | string  | required | Unique among monitors and controls.                            |
| `label`       | string  | `id`     | Widget title.                                                  |
| `kind`        | string  | required | `toggle`, `segmented`, `select`, `range`, `stepper`, `knob`.   |
| `target`      | string  | required | Device state field, see the target table.                      |
| `unit`        | string  | `""`     | Display suffix for numeric controls.                           |
| `description` | string  | `""`     | Small caption under the title.                                 |
| `options`     | array   | `[]`     | `select` only; at least one option.                            |
| `range`       | table   | none     | Required for `range`, `stepper`, `knob`: `{ min, max, step }`. |
| `presets`     | array   | `[]`     | Optional quick-set buttons for `range`, `stepper`, `knob`.     |

A control target may appear **only once** in a layout, so two widgets cannot
fight over the same device field.

### Targets

| `target`      | Type                          | Suitable kinds                    |
| ------------- | ----------------------------- | --------------------------------- |
| `biasEnabled` | boolean                       | `toggle`, `segmented`             |
| `biasPoint`   | `QUAD+`, `QUAD-`, `MAX`, `NULL` | `select`                        |
| `ditherAmp`   | number                        | `range`, `stepper`, `knob`        |
| `ditherFreq`  | number (Hz)                   | `range`, `stepper`, `knob`        |
| `mode`        | integer 0–255                 | `select`, `range`, `stepper`, `knob` |
| `gain`        | number                        | `range`, `stepper`, `knob`        |
| `offset`      | number                        | `range`, `stepper`, `knob`        |

### Control kinds

| `kind`      | UI                                                              |
| ----------- | --------------------------------------------------------------- |
| `toggle`    | Header switch.                                                  |
| `segmented` | Two large OFF / ON buttons.                                     |
| `select`    | Grid of option buttons.                                         |
| `range`     | Slider plus numeric input.                                      |
| `stepper`   | − / + buttons plus numeric input.                               |
| `knob`      | Rotary knob: drag ↕, arrow keys, Home/End; plus presets.        |

**Select options.** `biasPoint` is restricted to `QUAD+`, `QUAD-`, `MAX` and
`NULL`. `mode` options must be strings that parse as integers from 0 to 255,
for example `["0", "1", "2", "3"]`. Other targets cannot use `select`.

**Ranges.** `min` and `max` must be finite with `min < max`; `step` must be
greater than 0 and defaults to `1`. A range is required for numeric kinds and
ignored by the other kinds.

**Presets.** Presets are shown under numeric controls, at most 8 per control.
Every preset must lie inside the control range. For a `mode` control presets
must be integers from 0 to 255. Presets are available on `range`, `stepper` and
`knob` only.

## Complete example

A valid layout that uses all seven targets and every monitor variant. Copy it
over the file in the config directory, then press **Reload**.

```toml
title = "MCU-04 · Bias stage"

# --- Monitors ---------------------------------------------------------------

[[monitors]]
id = "angle"
label = "Θ₀"
source = "theta0"
unit = "°"
digits = 1
variant = "bar"
description = "Measured angle"
range = { min = -180, max = 180 }
threshold = { low = -90, high = 90 }

[[monitors]]
id = "temp"
label = "Temperature"
source = "temp"
unit = "°C"
digits = 1
variant = "gauge"
description = "Board temperature"
range = { min = 0, max = 100 }
threshold = { low = 5, high = 75 }

[[monitors]]
id = "input_power"
label = "Input power"
source = "inputPower"
unit = "W"
digits = 2
variant = "meter"
description = "Measured at input"
segments = 24
range = { min = 0, max = 100 }
threshold = { high = 90 }

[[monitors]]
id = "output_power"
label = "Output power"
source = "outputPower"
unit = "W"
digits = 2
variant = "sparkline"
description = "Output trend"
history = 120
range = { min = 0, max = 100 }

[[monitors]]
id = "bias_current"
label = "Bias current"
source = "biasCurrent"
unit = "A"
digits = 3
variant = "tile"
description = "Current through the bias stage"
threshold = { high = 0.8 }

[[monitors]]
id = "supply_voltage"
label = "Supply voltage"
source = "supplyVoltage"
unit = "V"
digits = 2
variant = "tile"
description = "Rail feeding the output stage"
threshold = { low = 11, high = 13 }

[[monitors]]
id = "phase_error"
label = "Phase error"
source = "phaseError"
unit = "°"
digits = 2
variant = "gauge"
description = "Residual loop phase error"
range = { min = -10, max = 10 }
threshold = { low = -2, high = 2 }

[[monitors]]
id = "lock"
label = "Loop lock"
source = "lockStatus"
variant = "led"
description = "MCU reports a locked loop"

# --- Controls ---------------------------------------------------------------

[[controls]]
id = "bias_enabled"
label = "Bias output"
kind = "toggle"
target = "biasEnabled"
description = "Master enable for the bias stage"

[[controls]]
id = "bias_point"
label = "Bias point"
kind = "select"
target = "biasPoint"
options = ["QUAD+", "QUAD-", "MAX", "NULL"]
description = "Operating point of the bias loop"

[[controls]]
id = "dither_amp"
label = "Dither amplitude"
kind = "range"
target = "ditherAmp"
unit = "V"
range = { min = 0, max = 10, step = 0.01 }
presets = [0, 0.5, 1, 5]
description = "Peak-to-peak dither amplitude"

[[controls]]
id = "dither_freq"
label = "Dither frequency"
kind = "stepper"
target = "ditherFreq"
unit = "Hz"
range = { min = 1, max = 5000, step = 1 }
presets = [10, 100, 1000, 2500]
description = "Dither tone frequency"

[[controls]]
id = "mode"
label = "Operating mode"
kind = "select"
target = "mode"
options = ["0", "1", "2", "3"]
description = "Mode code passed through to the MCU"

[[controls]]
id = "gain"
label = "Loop gain"
kind = "knob"
target = "gain"
unit = "×"
range = { min = 0, max = 4, step = 0.05 }
presets = [0.5, 1, 2]
description = "Error amplifier gain"

[[controls]]
id = "offset"
label = "Loop offset"
kind = "stepper"
target = "offset"
unit = "V"
range = { min = -5, max = 5, step = 0.1 }
presets = [-1, 0, 1]
description = "Fine trim added to the bias setpoint"
```

## Component gallery

The examples below are alternatives, not additions. Replace the corresponding
widget in the complete layout instead of adding a second control for the same
target.

Two ways to switch `biasEnabled` — pick one:

```toml
[[controls]]
id = "bias_switch"
label = "Bias output"
kind = "toggle"
target = "biasEnabled"
```

```toml
[[controls]]
id = "bias_buttons"
label = "Bias output"
kind = "segmented"
target = "biasEnabled"
```

Two ways to drive `mode` — pick one:

```toml
[[controls]]
id = "mode"
label = "Operating mode"
kind = "select"
target = "mode"
options = ["0", "1", "2", "3"]
```

```toml
[[controls]]
id = "mode"
label = "Operating mode"
kind = "stepper"
target = "mode"
range = { min = 0, max = 3, step = 1 }
presets = [0, 1, 2]
```

A rotary frequency control:

```toml
[[controls]]
id = "dither_freq"
label = "Dither frequency"
kind = "knob"
target = "ditherFreq"
unit = "Hz"
range = { min = 1, max = 5000, step = 1 }
presets = [100, 1000, 2500]
```

A threshold alarm monitor:

```toml
[[monitors]]
id = "rail"
label = "Supply rail"
source = "supplyVoltage"
unit = "V"
digits = 2
variant = "bar"
range = { min = 10, max = 14 }
threshold = { low = 11, high = 13 }
```

## Validation rules

Every problem found is reported; the layout is only applied when the list is
empty. A TOML syntax error is reported on its own. The most common rules:

- `id` must be non-empty and unique across **both** monitors and controls.
- Monitor `source` must be one of the nine sources in the table above.
- Control `target` must be one of the seven targets and may be used only once.
- `toggle` and `segmented` only accept `biasEnabled`; `select` only accepts
  `biasPoint` or `mode`; numeric kinds only accept the five numeric targets.
- `select` needs at least one `option`. `biasPoint` options are restricted to
  `QUAD+`, `QUAD-`, `MAX`, `NULL`; `mode` options must be integer strings.
- Numeric kinds need a `range` with finite `min < max` and `step > 0`.
- At most 8 presets, all inside the range; `mode` presets must be integers
  from 0 to 255.
- Monitor `digits` 0–8, `history` 8–600, `segments` 5–40.
- Monitor ranges need finite `min < max`; thresholds need finite values and,
  when both are present, `low < high`.
- Unknown keys at any level are rejected.

## Troubleshooting

- **“Unknown field”** — check the spelling and case of a key. Keys are
  camelCase; `title` belongs at the top level, not inside a monitor.
- **“Target … is already controlled”** — remove the duplicate control. For
  example, a layout cannot have both a `toggle` and a `segmented` control for
  `biasEnabled`.
- **My edit does nothing** — the app uses the file in its config directory.
  Press **Reload** after editing, and check the Settings page shows the path
  you expect. Upgrades never replace an existing file.
- **Monitor shows `—`** — no telemetry has arrived for that source. Inject a
  `TLM` line in Packet Lab or connect the MCU.
- **Sparkline says “waiting for samples”** — samples arrive with telemetry;
  the chart fills as new values come in.
- **Bar, gauge or meter has no fill** — add a `range`; the numeric value still
  displays without one.
- **Control changes seem delayed** — by design: controls are mirrored in the
  next `STATE` frame, sent every 300 ms.
