//! Text packet templates.
//!
//! Packet Lab payloads are plain text with `{placeholder}` substitutions. The
//! same syntax is used for one-shot writes and for the scheduled sender, so a
//! scheduled packet can track the live device state and the newest telemetry
//! sample instead of being frozen when the schedule starts.
//!
//! Grammar (deliberately small):
//!
//! ```text
//! {name}                 value with the default formatting
//! {name:.2}              float with two decimals
//! {name:04}              integer zero-padded to four characters
//! {name|fallback}        fallback text when the value is not available
//! {name:.2|0.00}         both
//! {{ and }}              literal braces
//! ```
//!
//! Unknown names are an error so a typo is reported before anything is sent.
//! A known name without a value (telemetry before the first sample) renders the
//! fallback, or `-` when no fallback was given.

use crate::protocol::{DeviceState, TelemetryPatch};

/// Values available to a packet template.
pub struct TemplateVars<'a> {
    /// Packet counter, bumped for every raw packet written.
    pub seq: u64,
    /// Packets sent by the current schedule (1 for a one-shot write).
    pub sent: u64,
    /// Zero-based position of the packet in the script.
    pub index: usize,
    /// Packet label, available as `{label}`.
    pub label: &'a str,
    /// Epoch milliseconds, available as `{now}` and `{time}`.
    pub now_ms: u64,
    pub state: &'a DeviceState,
    pub telemetry: &'a TelemetryPatch,
}

enum Value {
    Int(i64),
    Float(f32),
    Bool(bool),
    Text(String),
    /// A known telemetry field that has not produced a sample yet.
    Missing,
}

/// Expand every placeholder in `template`.
pub fn expand(template: &str, vars: &TemplateVars<'_>) -> Result<String, String> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        match ch {
            '{' => {
                if matches!(chars.peek(), Some((_, '{'))) {
                    chars.next();
                    out.push('{');
                    continue;
                }

                let mut body = String::new();
                let mut closed = false;
                for (_, next) in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    body.push(next);
                }
                if !closed {
                    return Err(format!("unterminated template placeholder `{{{body}`"));
                }
                out.push_str(&render_placeholder(&body, vars)?);
            }
            '}' => {
                if matches!(chars.peek(), Some((_, '}'))) {
                    chars.next();
                }
                out.push('}');
            }
            other => out.push(other),
        }
    }

    Ok(out)
}

fn render_placeholder(body: &str, vars: &TemplateVars<'_>) -> Result<String, String> {
    let body = body.trim();
    if body.is_empty() {
        return Err("empty template placeholder `{}`".into());
    }

    let (expression, fallback) = match body.split_once('|') {
        Some((expression, fallback)) => (expression.trim(), Some(fallback)),
        None => (body, None),
    };

    let (name, format) = match expression.split_once(':') {
        Some((name, format)) => (name.trim(), Some(format.trim())),
        None => (expression, None),
    };

    let Some(value) = lookup(name, vars) else {
        return Err(format!("unknown template variable `{name}`"));
    };

    if value.is_missing() {
        // Validate the format even when there is no sample yet; otherwise a
        // typo would only surface after the first telemetry line arrives.
        if let Some(spec) = format {
            float_precision(spec)?;
        }
        return Ok(fallback.unwrap_or("-").to_string());
    }

    format_value(value, format)
}

fn lookup(name: &str, vars: &TemplateVars<'_>) -> Option<Value> {
    let (prefix, key) = match name.split_once('.') {
        Some((prefix, key)) => (Some(prefix), key),
        None => (None, name),
    };

    if prefix == Some("telemetry") || prefix == Some("tel") {
        return telemetry_value(key, vars.telemetry);
    }
    if prefix == Some("state") {
        return state_value(key, vars.state);
    }

    // Built-ins and state first, telemetry second. This keeps `biasEnabled`
    // and `biasPoint` unambiguous while still allowing `{temp}`.
    match name {
        "seq" => return Some(Value::Int(vars.seq as i64)),
        "sent" => return Some(Value::Int(vars.sent as i64)),
        "index" => return Some(Value::Int(vars.index as i64)),
        "now" | "time" => return Some(Value::Int(vars.now_ms as i64)),
        "label" => return Some(Value::Text(vars.label.to_string())),
        _ => {}
    }

    state_value(name, vars.state).or_else(|| telemetry_value(name, vars.telemetry))
}

fn state_value(name: &str, state: &DeviceState) -> Option<Value> {
    let value = match name {
        "bias" | "biasEnabled" => Value::Bool(state.bias_enabled),
        "bp" | "biasPoint" => Value::Text(state.bias_point.to_string()),
        "amp" | "ditherAmp" => Value::Float(state.dither_amp),
        "freq" | "ditherFreq" => Value::Float(state.dither_freq),
        "mode" => Value::Int(i64::from(state.mode)),
        "gain" => Value::Float(state.gain),
        "offset" => Value::Float(state.offset),
        _ => return None,
    };
    Some(value)
}

fn telemetry_value(name: &str, telemetry: &TelemetryPatch) -> Option<Value> {
    let value = match name {
        "theta0" | "theta" => telemetry.theta0,
        "temp" | "temperature" => telemetry.temp,
        "biasVoltage" | "vbias" => telemetry.bias_voltage,
        "inputPower" | "pin" => telemetry.input_power,
        "outputPower" | "pout" => telemetry.output_power,
        "biasCurrent" | "ibias" | "current" => telemetry.bias_current,
        "supplyVoltage" | "vsupply" | "supply" => telemetry.supply_voltage,
        "phaseError" | "phase" | "perr" => telemetry.phase_error,
        "lockStatus" | "lock" => telemetry.lock_status,
        _ => return None,
    };
    Some(match value {
        Some(value) => Value::Float(value),
        None => Value::Missing,
    })
}

impl Value {
    fn is_missing(&self) -> bool {
        matches!(self, Value::Missing)
    }
}

fn format_value(value: Value, format: Option<&str>) -> Result<String, String> {
    let spec = format.unwrap_or("");

    match value {
        // The wire protocol uses 1/0 for booleans.
        Value::Bool(value) => Ok(if value { "1" } else { "0" }.into()),
        Value::Int(value) => {
            if spec.is_empty() {
                Ok(value.to_string())
            } else {
                let width = parse_width(spec)?;
                Ok(format!("{value:0width$}"))
            }
        }
        Value::Float(value) => {
            if spec.is_empty() {
                Ok(format!("{value}"))
            } else {
                let precision = float_precision(spec)?;
                Ok(format!("{value:.precision$}"))
            }
        }
        Value::Text(value) => Ok(value),
        Value::Missing => Ok("-".into()),
    }
}

fn float_precision(spec: &str) -> Result<usize, String> {
    let digits = spec
        .strip_prefix('.')
        .ok_or_else(|| format!("unsupported float format `{spec}` (use `.N`)"))?;
    let precision = digits
        .parse::<usize>()
        .map_err(|_| format!("unsupported float precision `{spec}`"))?;
    if precision > 12 {
        return Err("float precision must be 0…12".into());
    }
    Ok(precision)
}

fn parse_width(spec: &str) -> Result<usize, String> {
    let digits = spec.strip_prefix('0').unwrap_or(spec);
    let width = digits
        .parse::<usize>()
        .map_err(|_| format!("unsupported integer format `{spec}` (use `0N`)"))?;
    if width > 32 {
        return Err("integer width must be 0…32".into());
    }
    Ok(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::BiasPoint;

    fn vars<'a>(state: &'a DeviceState, telemetry: &'a TelemetryPatch) -> TemplateVars<'a> {
        TemplateVars {
            seq: 7,
            sent: 3,
            index: 1,
            label: "test",
            now_ms: 1_700_000_000_000,
            state,
            telemetry,
        }
    }

    #[test]
    fn expands_state_and_telemetry() {
        let state = DeviceState {
            bias_enabled: true,
            bias_point: BiasPoint::QuadPlus,
            dither_amp: 0.5,
            dither_freq: 1000.0,
            mode: 2,
            gain: 1.5,
            offset: -0.25,
        };
        let telemetry = TelemetryPatch {
            temp: Some(24.5),
            phase_error: Some(1.25),
            ..TelemetryPatch::default()
        };

        let text = expand(
            "STATE seq={seq} bias={biasEnabled} bp={biasPoint} amp={ditherAmp:.3} \
             freq={ditherFreq:.1} mode={mode:02} gain={gain:.3} offset={offset:.2} \
             temp={temp:.2} phase={phaseError:.1} label={label} index={index}",
            &vars(&state, &telemetry),
        )
        .unwrap();

        assert_eq!(
            text,
            "STATE seq=7 bias=1 bp=QUAD+ amp=0.500 freq=1000.0 mode=02 gain=1.500 \
             offset=-0.25 temp=24.50 phase=1.2 label=test index=1"
        );
    }

    #[test]
    fn missing_values_use_the_fallback() {
        let state = DeviceState::default();
        let telemetry = TelemetryPatch::default();
        let text = expand(
            "{temp|0.0} {lockStatus:.0|0} {temp}",
            &vars(&state, &telemetry),
        )
        .unwrap();
        assert_eq!(text, "0.0 0 -");
    }

    #[test]
    fn doubles_are_literal_braces() {
        let state = DeviceState::default();
        let telemetry = TelemetryPatch::default();
        assert_eq!(
            expand("{{{seq}}}", &vars(&state, &telemetry)).unwrap(),
            "{7}"
        );
    }

    #[test]
    fn rejects_unknown_names_and_bad_formats() {
        let state = DeviceState::default();
        let telemetry = TelemetryPatch::default();
        let context = vars(&state, &telemetry);

        assert!(
            expand("{nope}", &context)
                .unwrap_err()
                .contains("unknown template variable")
        );
        assert!(
            expand("{seq:.2}", &context)
                .unwrap_err()
                .contains("unsupported integer format")
        );
        assert!(
            expand("{temp:.99|0}", &context)
                .unwrap_err()
                .contains("float precision")
        );
        assert!(
            expand("{seq", &context)
                .unwrap_err()
                .contains("unterminated")
        );
    }

    #[test]
    fn qualified_names_are_supported() {
        let state = DeviceState::default();
        let telemetry = TelemetryPatch {
            supply_voltage: Some(12.1),
            ..TelemetryPatch::default()
        };
        assert_eq!(
            expand(
                "{state.biasPoint} {telemetry.supplyVoltage:.1} {tel.temp|0}",
                &vars(&state, &telemetry)
            )
            .unwrap(),
            "NULL 12.1 0"
        );
    }
}
