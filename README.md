# Bias Controller

Desktop master application for the MCU-04 bias stage. It owns the serial clock,
streams a state frame every 300 ms, renders telemetry monitor cards and exposes
controls — both declared in a TOML file — and includes a packet lab for
inspecting inbound bytes and scheduling raw outbound packets.

Built with Tauri 2, Vue 3, Vite, Tailwind CSS 4 and Rust.

## Run

```sh
bun install
bun run tauri dev
```

Production build:

```sh
bun run tauri build
```

## Widget layout (`widgets.toml`)

On first launch the app copies the bundled layout to its config directory:

| OS      | Path                                                              |
| ------- | ----------------------------------------------------------------- |
| Windows | `%APPDATA%\com.iman.tauri-biased\widgets.toml`                    |
| macOS   | `~/Library/Application Support/com.iman.tauri-biased/widgets.toml` |
| Linux   | `~/.config/com.iman.tauri-biased/widgets.toml`                    |

Edit that file and press **Reload** in *Settings*, or open any other `.toml`
file from the same page. Parse and validation errors are listed in the UI; the
last good layout stays active until the file is fixed. `Save a copy…` creates a
backup or a starting point.

The bundled reference file lives at
[`src-tauri/widgets.toml`](src-tauri/widgets.toml). The full reference — every
source, variant, target and control kind, validation rules, a complete starter
layout and copy-paste examples — is in [`docs/widgets.md`](docs/widgets.md).

### Monitors

Monitors are read-only cards fed by inbound telemetry.

```toml
[[monitors]]
id = "temp"                 # unique id
label = "Temperature"       # defaults to id
source = "temp"             # theta0 | temp | biasVoltage | inputPower | outputPower |
                            # biasCurrent | supplyVoltage | phaseError | lockStatus
variant = "gauge"           # bar | gauge | sparkline | led | tile | meter (default: bar)
history = 60                # samples kept by the sparkline variant (8..600)
segments = 20               # cells drawn by the meter variant (5..40)
unit = "°C"
digits = 1
description = "Board temperature"
range = { min = 0, max = 100 }      # bar / gauge scale
threshold = { low = 5, high = 75 }  # LOW / HIGH alarm; both keys optional
```

`range` controls the bar, gauge or meter; `threshold` controls the warning
state. Threshold markers are drawn on the bar and meter. Values outside a
threshold turn the card amber. `led` treats values ≥ 0.5 as ON, `sparkline`
keeps the last `history` samples, `tile` is a compact numeric readout, and
`bar` is the default card.

| `variant`   | Rendering                                                        |
| ----------- | ---------------------------------------------------------------- |
| `bar`       | value, horizontal bar with threshold ticks (default)             |
| `gauge`     | 270° radial gauge with threshold ticks                           |
| `sparkline` | rolling trend chart, sized by `history`                          |
| `led`       | large ON/OFF indicator for 0/1 signals                           |
| `tile`      | compact numeric tile for dense layouts                           |
| `meter`     | segmented level meter, sized by `segments`                       |

### Controls

Controls write to the device snapshot sent on every tick.

```toml
[[controls]]
id = "dither_amp"
label = "Dither amplitude"
kind = "range"              # toggle | segmented | select | range | stepper | knob
target = "ditherAmp"        # see the table below
unit = "V"
description = "Peak-to-peak dither amplitude"
range = { min = 0, max = 10, step = 0.01 }
presets = [0, 0.5, 1, 5]    # optional quick-set buttons
```

| `kind`      | Allowed `target`                   | Extra fields                        |
| ----------- | ---------------------------------- | ----------------------------------- |
| `toggle`    | `biasEnabled`                      | —                                   |
| `segmented` | `biasEnabled`                      | —                                   |
| `select`    | `biasPoint`, `mode`                | `options = ["QUAD+", "NULL", …]`    |
| `range`     | `ditherAmp`, `ditherFreq`, `mode`, `gain`, `offset` | `range`, optional `presets` |
| `stepper`   | `ditherAmp`, `ditherFreq`, `mode`, `gain`, `offset` | `range`, optional `presets` |
| `knob`      | `ditherAmp`, `ditherFreq`, `mode`, `gain`, `offset` | `range`, optional `presets` |

Widget ids must be unique and a target can only be controlled once, so the
config cannot create two sliders fighting over the same field. Presets must lie
inside the control range (at most 8 per control).

## Configuration-locked customer builds

Build a production app first (`bun run tauri build`) and launch that build. In
*Settings → Customer build*, press **Export customer build…** and choose where
to save it. The export is a standalone copy of the running executable with the
active `widgets.toml` embedded in it.

In a customer build:

- the embedded layout is the only configuration, and any `widgets.toml` on the
  customer's machine is ignored;
- Reload, Open file, Use app default and Save a copy are unavailable;
- the app refuses to load another layout even if the command is called
  directly;
- the status is shown as **configuration locked** in the side bar and Settings.

The layout is validated before export, so a broken file cannot be shipped. The
copy targets the current operating system, which means the production app must
be built on the platform you intend to distribute. Exporting is disabled in
debug builds because those depend on the Vite dev server. On Linux, both a
plain binary and an AppImage can be used as the source. On macOS the export is
the app's executable, so place it in a `.app` bundle and sign that bundle before
distribution. If releases are code-signed, re-sign the exported copy as well.

## Inbound telemetry

The reader thread mirrors every inbound chunk to the Packet Lab and buffers
lines for telemetry. The placeholder line format is:

```text
TLM seq=12 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8 ibias=0.4 vsupply=12.1 phase=1.5 lock=1\n
```

Accepted keys: `theta0`/`theta`, `temp`/`temperature`, `vbias`/`bias`,
`pin`/`input`, `pout`/`output`, `ibias`/`current`, `vsupply`/`supply`,
`phase`/`perr`, `lock`/`locked`. Unknown keys are ignored. Adjust
`src-tauri/src/protocol.rs` when the real MCU format is known.

## Packet Lab

The **Packet Lab** page has two halves:

- **Inbound stream** — live chunks with timestamp, inter-packet delta, length and
  hex/ASCII preview. Filter by hex, ASCII, sequence or time, pause capture,
  follow the tail, and click a chunk for a full 16-byte hex dump. Injected
  chunks are marked `SIM`.
- **Packet composer** — an ordered list of raw packets. Each packet is text
  (with `\n`, `\r`, `\t`, `\\`, `\0`, `\xNN` escapes) or hex (spaces, commas and
  `0x` prefixes are fine). A configurable interval (default **300 ms**) writes
  one packet per tick in list order. The countdown ring shows the next send,
  and the TX cadence strip visualises recent sends. Optional “stop after N”
  limits a burst.
- **Inject** feeds a composed packet through the inbound path. This drives the
  Packet Lab and monitor cards without hardware — useful for UI testing.

The schedule lives in the Rust backend, so the timing continues while you
navigate to another page. Connect a serial port before starting a schedule;
injection works without a link. Switching a packet between TXT and HEX rewrites
the payload so the transmitted bytes stay the same, and invalid payloads are
rejected before sending or injecting.

## Wire protocol (outbound)

The master writes one complete snapshot per tick (300 ms by default):

```text
STATE seq=42 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=0 gain=1.000 offset=0.000\n
CAL seq=43\n
```

All bytes leaving the port are produced by `encode_state` and
`encode_calibrate` in [`src-tauri/src/protocol.rs`](src-tauri/src/protocol.rs).
Scheduled Packet Lab payloads are written verbatim between regular state
frames.

## Layout

```
docs/
  widgets.md           widget layout reference and examples
src/
  lib/device.ts        serial state + telemetry mirror
  lib/widgets.ts       TOML layout types, loading and control binding
  lib/packets.ts       RX buffer, packet composer and scheduler state
  pages/main.vue       overview (monitor cards from TOML)
  pages/packets.vue    Packet Lab
  pages/settings.vue   layout file management + link defaults
  components/          monitor variants, control widgets, bars, plot, logs
src-tauri/
  src/lib.rs           serial manager, reader, scheduler, commands
  src/config.rs        TOML schema + validation
  src/locking.rs       embedded config for customer builds
  src/protocol.rs      outbound encoding + telemetry parsing
  widgets.toml         bundled default layout
```
