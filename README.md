# Bias Controller

Desktop master application for the MCU-04 bias stage. It owns the serial clock,
polls the MCU with a query frame every 300 ms, renders telemetry monitor cards
and exposes controls — both declared in a TOML file — and, in the editor build,
includes a packet lab for inspecting inbound bytes and scheduling raw outbound
packets. Command packets (state snapshots and calibration) are sent on demand
when the user clicks the corresponding action. The packet lab is not available
in a configuration-locked customer build.

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
scale = 1.0                 # first display multiplier (default 1.0)
multiply = 1.0              # second display multiplier (default 1.0)
offset = 0.0                # display offset (default 0.0)
offsetFirst = false         # add offset before scaling (default false)
description = "Board temperature"
range = { min = 0, max = 100 }      # bar / gauge scale
threshold = { low = 5, high = 75 }  # LOW / HIGH alarm; both keys optional
```

`scale`, `multiply` and `offset` apply a linear display transform:
`raw * scale * multiply + offset`, or `(raw + offset) * scale * multiply` when
`offsetFirst = true`. For example, `scale = 1000` shows volts as millivolts.
Both multipliers must be finite and non-zero; the offset must be finite. The
transform runs before `digits`, `range` and `threshold`, so set those in display
units. Wire decoding, packet templates and other widgets are unaffected.

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

Controls update the command snapshot that is sent when the user clicks **Send command**. Set `sendOnChange = true` on a control to send that snapshot automatically whenever its value changes.

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
sendOnChange = true         # send the snapshot on every change (default false)
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

`sendOnChange` (default `false`) transmits the full command snapshot as soon as
that control changes. Rapid slider and knob edits are coalesced into one frame,
and unchanged values send nothing. This uses the same path as **Send command**.

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
- the Packet Lab page and its packet commands are unavailable;
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
`phase`/`perr`, `lock`/`locked`. Unknown keys are ignored.

In the default `rs` binary mode, the last four frame fields are reserved slots.
`[protocol.telemetrySlots]` in `widgets.toml` decides which source reads each
one (`theta0 = "rsv0"`, `biasCurrent = "rsv1"`, `supplyVoltage = "rsv2"`,
`phaseError = "rsv3"` by default). Change these if the firmware uses another
layout — for example, θ₀ arriving in `rsv3`.

## Packet Lab

The **Packet Lab** is an editor-only page with two halves. It is omitted from
configuration-locked customer builds.

- **Inbound stream** — live chunks with timestamp, inter-packet delta, length and
  hex/ASCII preview. Filter by hex, ASCII, sequence or time, pause capture,
  follow the tail, and click a chunk for a full 16-byte hex dump plus a
  structure panel: text/binary balance, line framing, complete lines, command
  tag and `key=value` fields. Injected chunks are marked `SIM`.
- **Packet script** — one packet per line, sent in top-to-bottom order:

  ```text
  # Comments start with # or //.
  Ping => PING\n
  TLM sample => TLM seq=1 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\n
  Cal => [hex] 01 02 A0 FF
  Note => [raw] C:\logs\raw.txt
  Live => STATE seq={seq} bp={biasPoint} temp={temp:.2|0.0}\n
  ```

  A label is optional. `[text]` is the default; `[hex]` accepts spaces, commas
  and `0x` prefixes; `[raw]` sends UTF-8 without escape processing. Text
  supports `\n`, `\r`, `\t`, `\0`, `\\` and `\xNN`. Invalid lines are reported
  with their line number and are never sent. A configurable interval (default
  **300 ms**) writes one packet per tick. The countdown ring shows the next
  send, and the TX cadence strip visualises recent sends. Optional “stop after
  N” limits a burst.
- **Templates** — `{name}` placeholders are expanded at send time from live
  state and telemetry, including on every scheduled frame. Use `{temp:.2}` for
  a float, `{seq:04}` for a padded integer and `{temp|0.0}` for a fallback
  before the first sample. Write `{{` and `}}` for literal braces.
- **Inject** feeds a parsed packet through the inbound path. This drives the
  Packet Lab and monitor cards without hardware — useful for UI testing.

The schedule lives in the Rust backend, so the timing continues while you
navigate to another page. Connect a serial port before starting a schedule;
injection works without a link. A running schedule keeps its packet definitions
but re-expands templates on every send; stop and start again to change the
definitions. See [`docs/packets.md`](docs/packets.md) for the full format,
template variables and the backend packet structure.

## Wire protocol

The app is the **master** and starts every transaction; the MCU is the slave.
With the default `rs` driver the framing is:

```text
master -> slave  read:  77 23
master -> slave  write: 71 <8 x uint16> <txFooter>
slave -> master  ack:   71 23
slave -> master  data:  77 <9 x uint16> 23
```

The 300 ms tick sends only the two-byte status read (`77 23`); the MCU replies
with the data frame that feeds the monitor cards. Writes are sent on demand —
**Send command** for the command snapshot, **Calibrate** for the calibration
flag — using the same packet body with the `71` header, and the MCU answers with
the `71 23` acknowledge. ACKs are consumed without producing telemetry.

Framing bytes are configurable under `[protocol]` in `widgets.toml`
(`rxSync`, `txHeader`, `rxFooter`, `txFooter`) along with endianness, scaling and
the tick period; a customer build freezes those values. Scheduled Packet Lab
payloads are template-expanded and written independently of the master tick.
See `encode_rs_status_read`, `encode_rs_slave` and `decode_rs_master` in
[`src-tauri/src/protocol.rs`](src-tauri/src/protocol.rs).

The `legacy` driver keeps the older text (ASCII) master frames:

```text
QUERY seq=41\n
STATE seq=42 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=0 gain=1.000 offset=0.000\n
CAL seq=43\n
```

In editable (editor) builds, *Settings → Serial defaults → Wire format* switches
legacy query/command frames between **Text (ASCII)** and **Binary
(placeholder)**; customer builds use a fixed format.

## Layout

```
docs/
  widgets.md           widget layout reference and examples
  packets.md           packet structure, text script and customer-build rules
src/
  lib/device.ts        serial state + telemetry mirror
  lib/widgets.ts       TOML layout types, loading and control binding
  lib/packets.ts       RX buffer, packet script parser and scheduler state
  pages/main.vue       overview (monitor cards from TOML)
  pages/packets.vue    Packet Lab (editor builds only)
  pages/settings.vue   layout file management + link defaults
  components/          monitor variants, control widgets, bars, plot, logs
src-tauri/
  src/lib.rs           serial manager, reader, scheduler, commands
  src/template.rs      packet placeholder expansion
  src/config.rs        TOML schema + validation
  src/locking.rs       embedded config for customer builds
  src/protocol.rs      outbound encoding + telemetry parsing
  widgets.toml         bundled default layout
```
