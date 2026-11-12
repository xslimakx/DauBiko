//! Parameter text conversion and defaults.
use daubiko::DauBikoParams;
use nih_plug::prelude::*;

#[test]
fn dynamics_text_round_trip() {
    let p = DauBikoParams::default();
    let d = &p.dynamics;
    for (txt, want) in [("PUNCH 40", 40.0f32), ("glue 25", -25.0), ("0", 0.0), ("-60", -60.0), ("100", 100.0)] {
        let n = d.string_to_normalized_value(txt).unwrap_or_else(|| panic!("parse {txt}"));
        assert!((d.preview_plain(n) - want).abs() < 0.01, "{txt}");
    }
    assert_eq!(d.normalized_value_to_string(d.preview_normalized(40.0), false), "PUNCH 40");
    assert_eq!(d.normalized_value_to_string(d.preview_normalized(-25.0), false), "GLUE 25");
    assert_eq!(d.normalized_value_to_string(0.5, false), "0");
}

#[test]
fn low_cut_text() {
    let p = DauBikoParams::default();
    let l = &p.lowcut;
    assert_eq!(l.normalized_value_to_string(0.0, true), "OFF");
    assert!((l.preview_plain(l.string_to_normalized_value("off").unwrap()) - 10.0).abs() < 0.01);
    assert!((l.preview_plain(l.string_to_normalized_value("35 Hz").unwrap()) - 35.0).abs() < 0.01);
    assert!((l.preview_plain(l.string_to_normalized_value("35").unwrap()) - 35.0).abs() < 0.01);
}

#[test]
fn defaults_are_neutral() {
    let p = DauBikoParams::default();
    assert!(p.power.value());
    for f in [&p.input, &p.low, &p.body, &p.presence, &p.air, &p.tilt, &p.dynamics, &p.drive, &p.gain] {
        assert_eq!(f.value(), 0.0, "{}", f.name());
    }
    assert_eq!(p.width.value(), 100.0);
    assert_eq!(p.ceiling.value(), -0.3);
    assert!(p.limiter.value());
    assert!(!p.bass_mono.value());
}

#[test]
fn db_values_typed_with_or_without_unit() {
    let p = DauBikoParams::default();
    for txt in ["3", "+3", "3 dB", "3.0"] {
        let n = p.low.string_to_normalized_value(txt).unwrap_or_else(|| panic!("{txt}"));
        assert!((p.low.preview_plain(n) - 3.0).abs() < 0.01, "{txt}");
    }
}
