# Packet Lab and wire packets

This document describes every packet path in the app, the Packet Lab text
script, and how the Packet Lab is excluded from customer builds.

## Packet paths at a glance

| Direction | Producer | Timing | Payload (default `rs` driver) |
| --------- | -------- | ------ | ----------------------------- |
| TX | Master clock (`src-tauri/src/lib.rs`) | every 300 ms | `77 23` status read |
| TX | Send command | one shot | `71 …` write (command snapshot) |
| TX | Calibrate command | one shot | `71 …` write (calibration flag) |
| TX | Packet Lab | manual or scheduled | template-expanded bytes from the script |
| RX | Serial reader | after every read | `77 … 23` data frame, or `71 23` write ACK |
| RX | Packet Lab “Inject” | manual | synthetic chunk + telemetry |

The app is the master: the Rust-owned clock starts a new transaction every tick
by requesting status, and the MCU answers with the telemetry frame that feeds
the monitor cards. Writes only happen when the user clicks **Send command** or
**Calibrate**; the MCU replies with a two-byte acknowledge that is consumed
without producing telemetry.

## Master frames

### RS driver (default)

`protocol::encode_rs_status_read` produces the two-byte tick frame;
`protocol::encode_rs_slave` produces write frames for **Send command** and
**Calibrate**. The `calibrationRequest` field of a write distinguishes them.

```text
read:   77 23
write:  71 <amp u16><freq u16><biasPoint u16><ditherOn u16>
           <calibrationRequest u16><rst0 u16><rst1 u16><rst2 u16><txFooter>
ack:    71 23
data:   77 <biasVoltage u16><inputPower u16><outputPower u16><temperature u16>
           <calibrationDone u16><rsv0 u16><rsv1 u16><rsv2 u16><rsv3 u16> 23
```

Framing bytes (`rxSync` = `77`, `txHeader` = `71`, `rxFooter` = `23`,
`txFooter`), endianness, CRC and scaling are configurable under `[protocol]` in
`widgets.toml`. `protocol::decode_rs_master` consumes both response shapes and
publishes only data frames as telemetry.

### Legacy driver

With `driver = "legacy"`, frames use the selected wire format (editable builds
can choose **text** or **binary** in Settings; customer builds use a fixed
format). In **text (ASCII)** format:

```text
QUERY seq=41\n
STATE seq=42 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=0 gain=1.000 offset=0.000\n
CAL seq=43\n
```

`QUERY` is the periodic polling frame, `STATE` the on-demand command snapshot
and `CAL` the calibration command. Binary mode emits compact placeholder
tagged/packed bytes (see `src-tauri/src/protocol.rs`).

`seq` is a wrapping `u32` counter owned by the backend and used by legacy frames.
A scheduled Packet Lab payload is template-expanded, encoded and then written
independently of the periodic tick; the schedules do not merge or re-frame each
other.

## Packet Lab packet model

A Packet Lab packet has four parts:

| Field | Meaning |
| ----- | ------- |
| `label` | Human-readable name, optional. Defaults to `packet N`. |
| `encoding` | `text`, `hex` or `raw`. |
| `payload` | The bytes to write, in the chosen encoding. |
| order | Script line order; packets are sent round-robin in that order. |

Limits enforced by both the UI and the backend:

- A decoded packet must be non-empty.
- A decoded packet must be at most 8 KiB.
- The interval must be 10–60 000 ms.
- The optional limit is 0 (continuous) to 10 000 000 sends.

These limits apply to the payload after template expansion.

### Encodings

**`text`** (default) — UTF-8 text with escapes:

| Escape | Byte |
| ------ | ---- |
| `\n` | `0A` |
| `\r` | `0D` |
| `\t` | `09` |
| `\0` | `00` |
| `\\` | `5C` |
| `\xNN` | the byte `NN` |

Unknown escapes are preserved literally, so `\q` sends `\q`.

**`hex`** — hexadecimal bytes. Whitespace, commas, underscores, colons, dashes
and `0x` prefixes are ignored, so pasted dumps work without editing:

```text
01 02 A0 FF
0x01,0x02,0xA0,0xFF
01_02_A0_FF
```

**`raw`** — UTF-8 text sent without escape processing. Backslashes are
literal. Templates still expand, so double braces are needed for literal
braces. Useful for paths, Windows command lines and payloads that already
contain escape-like text.

## Text script format

The editor is a single text box. Each non-empty line is one packet, sent in
top-to-bottom order.

```text
# Comments start with # or //.
# Optional label before "=>"; encoding with [text], [hex] or [raw].

Ping => PING\n
TLM sample => TLM seq=1 theta0=12.3 temp=24.5 vbias=3.3 pin=1.2 pout=0.8\n
Cal => [hex] 01 02 A0 FF
Note => [raw] C:\logs\raw.txt
TLM seq=2 temp=25.0\n
```

Formal shape:

```text
line     := ws [ label ws "=>" ws ] body
body     := [ encoding ws ] payload
encoding := "[" ("text" | "hex" | "raw") "]" | ("text" | "hex" | "raw") ":"
```

Rules:

- A line with no `=>` is a text packet with an automatic label.
- `=>` is reserved as the label separator. The first `=>` on a line is the
  separator; to send an unlabelled payload that contains `=>`, give the line a
  label (`Note => A => B` sends `A => B`).
- A line starting with `#` or `//` after leading whitespace is a comment.
- Blank lines are ignored.
- Invalid lines are reported with their line number and are never sent.
- A running schedule keeps the packet definitions it started with, but
  re-expands every template at send time. Stop and start again to change the
  definitions.

The script is persisted in `localStorage` under
`bias-controller.packet-lab`. Older releases stored a JSON array of packets;
that array is migrated to script text on first load.

## Templates

Every payload is a small text template. Immediately before encoding, the
backend replaces `{...}` with live values from the current device state and the
newest telemetry sample. This applies to all three encodings. Expansion happens
on every send, including each frame of a schedule, so one packet can track
changing values without being rewritten.

```text
Live => STATE seq={seq} bias={biasEnabled} bp={biasPoint} temp={temp:.2|0.0}\n
Cal => [hex] A5 {seq:04}
```

Grammar:

```text
placeholder := "{" name [ ":" format ] [ "|" fallback ] "}"
```

| Form | Meaning |
| ---- | ------- |
| `{name}` | Value with the default formatting. |
| `{name:.2}` | Float with two decimals (0–12). |
| `{name:04}` | Integer zero-padded to four characters (0–32). |
| `{name\|fallback}` | Text used when a telemetry value has no sample yet. |
| `{{` and `}}` | Literal braces. |

Available names:

| Group | Names |
| ----- | ----- |
| Counters | `seq` (Packet Lab write counter), `sent` (sends in this schedule), `index` (zero-based packet position), `now` / `time` (epoch ms), `label` |
| State | `biasEnabled` / `bias`, `biasPoint` / `bp`, `ditherAmp` / `amp`, `ditherFreq` / `freq`, `mode`, `gain`, `offset` |
| Telemetry | `theta0` / `theta`, `temp` / `temperature`, `biasVoltage` / `vbias`, `inputPower` / `pin`, `outputPower` / `pout`, `biasCurrent` / `ibias` / `current`, `supplyVoltage` / `vsupply` / `supply`, `phaseError` / `phase` / `perr`, `lockStatus` / `lock` |

State and telemetry names can also be qualified as `{state.biasPoint}`,
`{telemetry.temp}` or `{tel.temp}`. Unqualified names resolve state first, then
telemetry.

Details:

- Booleans render as `1` or `0`, matching the wire protocol.
- A known telemetry field without a sample renders its fallback, or `-` when no
  fallback is given. The format is still validated so a typo is reported before
  the first sample arrives.
- An unknown name is an error. The packet is not sent and the schedule stops
  with the error.
- Text escapes are processed after expansion, so `{...}` output can include
  `\n` from the surrounding payload.
- A templated `hex` payload is validated after expansion. The editor previews
  it with the current sample and shows `—` while that sample cannot produce
  valid hex; the backend checks the live result again at send time.

## Packet structure analysis

Selecting a chunk in the inbound stream opens a read-only structure panel below
the hex dump. It describes the chunk as received, without re-framing or editing
it:

| Row | Meaning |
| --- | ------- |
| `kind` | `empty`, `text` (at least 85 % printable), `binary` (at most 30 %) or `mixed`. |
| `framing` | Trailing line ending: `LF`, `CRLF`, `CR` or `none`. |
| `printable` | Share of bytes that are printable ASCII or `\t` / `\r` / `\n`. |
| `lines` | Complete text lines, or 0 for binary chunks. |
| `command` | First whitespace-delimited token, typically `QUERY`, `STATE`, `CAL` or `TLM`. |
| `fields` | Every `key=value` token in the chunk, such as `temp=24.5`. |

The same panel is useful for checking a real MCU frame before writing a parser:
compare the observed framing, command tag and field names with the placeholder
protocol in this document. Injection sends a synthetic chunk through the same
path, so it can be analysed identically.

The composer also previews the live byte count for each packet. Templates are
expanded with the current device state and newest telemetry sample; a templated
`hex` payload shows `—` while the current sample cannot produce valid hex. The
backend still performs the authoritative decode and size check for every send.

## Backend commands and events

| Command | Purpose |
| ------- | ------- |
| `send_packet` | Write one packet immediately. |
| `start_packet_sender` | Start a round-robin schedule. |
| `stop_packet_sender` | Stop the schedule. |
| `get_packet_sender` | Read the current scheduler snapshot. |
| `inject_rx` | Feed bytes through the inbound path without hardware. |

| Event | Payload |
| ----- | ------- |
| `serial://rx` | One inbound chunk: timestamp, bytes, sequence, totals, synthetic flag. |
| `serial://packet` | One Packet Lab write: label, index, `{seq}` counter, byte count, totals, interval. |
| `serial://sender` | Scheduler snapshot after every start/stop/send. |
| `serial://status` | Link status, including TX/RX counters. |
| `telemetry://state` | Parsed `TLM` fields for the monitor cards. |

The reader thread emits `serial://rx` for every chunk and separately buffers
lines for telemetry. A `TLM` line is parsed when it ends in `\n`; partial lines
are buffered up to 8 KiB and then dropped.

## Customer builds

The Packet Lab is an engineering tool and is not part of a
configuration-locked customer build:

- the side-bar entry is hidden until build information confirms an editable
  build;
- the page is loaded lazily, so a customer build never renders it;
- the packets view falls back to the overview if it is reached indirectly;
- `send_packet`, `start_packet_sender`, `stop_packet_sender`,
  `get_packet_sender` and `inject_rx` return an error in a locked build, even
  if called directly.

The customer export is a copy of the same executable with the layout appended
in a trailer. The Packet Lab code therefore still exists in the binary but is
unreachable. If a build must physically omit the code, produce a separate
binary from a source tree with the Packet Lab removed rather than using the
export command.
