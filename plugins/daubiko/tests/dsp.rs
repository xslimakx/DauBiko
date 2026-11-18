//! DauBiko DSP tests: transparency, limiter ceiling, EQ, width, DYNAMICS, drive, latency, stability.
use daubiko::dsp::*;
use std::f32::consts::TAU;

const FS: f32 = 48000.0;

fn run(m: &mut Master, p: &Params, input: &[[f32; 2]]) -> Vec<[f32; 2]> {
    input
        .iter()
        .map(|x| {
            let mut y = *x;
            m.process(p, &mut y, 2);
            y
        })
        .collect()
}

fn sine(f: f32, amp: f32, len: usize, fs: f32) -> Vec<[f32; 2]> {
    (0..len).map(|n| {
        let v = amp * (TAU * f * n as f32 / fs).sin();
        [v, v]
    })
    .collect()
}

fn db(x: f32) -> f32 {
    20.0 * (x.abs() + 1e-12).log10()
}

fn peak(x: &[[f32; 2]]) -> f32 {
    x.iter().fold(0.0f32, |a, s| a.max(s[0].abs()).max(s[1].abs()))
}

fn rms(x: &[[f32; 2]], ch: usize) -> f32 {
    (x.iter().map(|s| (s[ch] as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt() as f32
}

/// Amplitude of frequency `f` (Goertzel) in channel `ch`.
fn tone(x: &[[f32; 2]], ch: usize, f: f32, fs: f32) -> f32 {
    let w = TAU as f64 * f as f64 / fs as f64;
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (n, s) in x.iter().enumerate() {
        re += s[ch] as f64 * (w * n as f64).cos();
        im -= s[ch] as f64 * (w * n as f64).sin();
    }
    (2.0 * (re * re + im * im).sqrt() / x.len() as f64) as f32
}

struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    fn bi(&mut self) -> f32 {
        self.f() * 2.0 - 1.0
    }
}

/// Drum-like test signal: decaying noise / tone hits on a pad, 4 hits per second.
fn drums(len: usize, fs: f32) -> Vec<[f32; 2]> {
    let mut r = Rng(7);
    (0..len)
        .map(|n| {
            let t = n as f32 / fs;
            let ph = t * 4.0 % 1.0;
            let env = (-ph * 18.0).exp();
            let hit = 0.6 * env * ((TAU * 70.0 * t).sin() + 0.5 * r.bi());
            let pad = 0.08 * (TAU * 220.0 * t).sin();
            let v = hit + pad;
            [v, v * 0.97]
        })
        .collect()
}

fn crest_db(x: &[[f32; 2]]) -> f32 {
    db(peak(x)) - db(rms(x, 0))
}

#[test]
fn neutral_is_transparent_apart_from_latency() {
    let mut m = Master::new(FS);
    let p = Params::default();
    let lat = m.latency();
    let x = sine(1000.0, 0.5, 48000, FS);
    let y = run(&mut m, &p, &x);
    let mut err = 0.0f32;
    for n in 4800..48000 {
        err = err.max((y[n][0] - x[n - lat][0]).abs()).max((y[n][1] - x[n - lat][1]).abs());
    }
    assert!(err < 2e-3, "neutral error {err}");
}

#[test]
fn power_off_is_bit_exact_delayed_dry() {
    let mut m = Master::new(FS);
    let p = Params { power: false, low_db: 8.0, drive: 1.0, dynamics: 1.0, gain_db: 12.0, width: 2.0, ..Params::default() };
    let lat = m.latency();
    let mut r = Rng(3);
    let x: Vec<[f32; 2]> = (0..20000).map(|_| [r.bi() * 0.8, r.bi() * 0.8]).collect();
    let y = run(&mut m, &p, &x);
    for n in 10000..20000 {
        assert_eq!(y[n], x[n - lat], "sample {n}");
    }
}

#[test]
fn latency_matches_impulse_position() {
    for fs in [44100.0f32, 48000.0, 96000.0, 192000.0] {
        let mut m = Master::new(fs);
        let p = Params { limiter: false, ..Params::default() };
        let mut x = vec![[0.0f32; 2]; 4000];
        x[100] = [0.5, 0.5];
        let y = run(&mut m, &p, &x);
        let (imax, _) = y.iter().enumerate().fold((0, 0.0f32), |a, (i, s)| if s[0].abs() > a.1 { (i, s[0].abs()) } else { a });
        assert_eq!(imax, 100 + m.latency(), "fs {fs}");
        assert!(m.latency() as f32 / fs < 0.003);
    }
}

#[test]
fn limiter_never_exceeds_ceiling() {
    for fs in [22050.0f32, 44100.0, 48000.0, 96000.0, 192000.0] {
        for ceiling in [0.0f32, -0.3, -1.0, -6.0] {
            let mut m = Master::new(fs);
            let p = Params { gain_db: 12.0, input_db: 12.0, ceiling_db: ceiling, drive: 0.3, dynamics: 1.0, ..Params::default() };
            let mut r = Rng(11);
            let len = fs as usize * 2;
            let x: Vec<[f32; 2]> = (0..len)
                .map(|n| match (n / 3000) % 5 {
                    0 => [r.bi(), r.bi()],
                    1 => [if (n / 7) % 2 == 0 { 1.0 } else { -1.0 }; 2],
                    2 => [if n % 997 == 0 { 1.0 } else { 0.0 }, 0.0],
                    3 => [(TAU * 60.0 * n as f32 / fs).sin(), -(TAU * 8000.0 * n as f32 / fs).sin()],
                    _ => [0.0, 0.0],
                })
                .collect();
            let y = run(&mut m, &p, &x);
            let pk = peak(&y);
            assert!(pk <= 10f32.powf(ceiling / 20.0) + 1e-6, "fs {fs} ceiling {ceiling}: peak {} dB", db(pk));
        }
    }
}

#[test]
fn limiter_is_clean_below_ceiling_and_reduces_above() {
    let mut m = Master::new(FS);
    let p = Params { gain_db: 6.0, ..Params::default() };
    let y = run(&mut m, &p, &sine(200.0, 0.5, 48000, FS));
    // 0.5 * 2 = 1.0 > ceiling: limited to ceiling, low distortion (3rd harmonic small)
    let t = &y[24000..];
    let f1 = tone(t, 0, 200.0, FS);
    let h3 = tone(t, 0, 600.0, FS);
    assert!(db(f1) < -0.25 && db(f1) > -3.0, "fundamental {} dB", db(f1));
    assert!(db(h3) - db(f1) < -30.0, "limiter THD too high: {} dB", db(h3) - db(f1));
}

#[test]
fn eq_bands_match_their_curves() {
    let cases: [(fn(&mut Params, f32), f32); 5] = [
        (|p, v| p.low_db = v, 40.0),
        (|p, v| p.body_db = v, BODY_HZ),
        (|p, v| p.presence_db = v, PRES_HZ),
        (|p, v| p.air_db = v, 16000.0),
        (|p, v| p.tilt_db = v, 8000.0),
    ];
    for (set, f) in cases {
        for gain in [-6.0f32, 4.0] {
            let mut p = Params { limiter: false, ..Params::default() };
            set(&mut p, gain);
            let want = eq_curve_db(FS, &p, f);
            let mut m = Master::new(FS);
            let y = run(&mut m, &p, &sine(f, 0.1, 48000, FS));
            let got = db(tone(&y[24000..], 0, f, FS)) - db(0.1);
            assert!((got - want).abs() < 0.3, "f {f} gain {gain}: got {got} want {want}");
            assert!(want.abs() > 1.0, "band at {f} hardly moves: {want}");
        }
    }
}

#[test]
fn low_cut_removes_rumble() {
    let p = Params { lowcut_hz: 80.0, limiter: false, ..Params::default() };
    let mut m = Master::new(FS);
    let y = run(&mut m, &p, &sine(20.0, 0.5, 96000, FS));
    assert!(db(tone(&y[48000..], 0, 20.0, FS)) - db(0.5) < -20.0);
    let mut m = Master::new(FS);
    let y = run(&mut m, &p, &sine(1000.0, 0.5, 48000, FS));
    assert!((db(tone(&y[24000..], 0, 1000.0, FS)) - db(0.5)).abs() < 0.1);
}

#[test]
fn width_and_bass_mono() {
    // pure side signal
    let side = |f: f32| -> Vec<[f32; 2]> { (0..48000).map(|n| { let v = 0.3 * (TAU * f * n as f32 / FS).sin(); [v, -v] }).collect() };
    let mut m = Master::new(FS);
    let y = run(&mut m, &Params { width: 0.0, limiter: false, ..Params::default() }, &side(1000.0));
    assert!(peak(&y[2000..]) < 1e-4, "width 0 must be mono");
    let mut m = Master::new(FS);
    let y = run(&mut m, &Params { width: 2.0, limiter: false, ..Params::default() }, &side(1000.0));
    assert!((db(tone(&y[24000..], 0, 1000.0, FS)) - db(0.6)).abs() < 0.2);
    // bass mono: 40 Hz side removed, 2 kHz side kept
    let bm = Params { bass_mono: true, limiter: false, ..Params::default() };
    let mut m = Master::new(FS);
    let y = run(&mut m, &bm, &side(40.0));
    assert!(db(tone(&y[24000..], 0, 40.0, FS)) - db(0.3) < -9.0);
    let mut m = Master::new(FS);
    let y = run(&mut m, &bm, &side(2000.0));
    assert!((db(tone(&y[24000..], 0, 2000.0, FS)) - db(0.3)).abs() < 0.3);
    // mid content is never touched by bass mono
    let mut m = Master::new(FS);
    let y = run(&mut m, &bm, &sine(40.0, 0.3, 48000, FS));
    assert!((db(tone(&y[24000..], 0, 40.0, FS)) - db(0.3)).abs() < 0.2);
}

/// Hit-to-floor contrast: average peak of the hits vs RMS of the quiet part between them.
fn contrast_db(x: &[[f32; 2]], fs: f32) -> f32 {
    let beat = (fs / 4.0) as usize;
    let (mut pk, mut fl, mut n) = (0.0f64, 0.0f64, 0);
    for b in x.chunks(beat).filter(|c| c.len() == beat) {
        let hit = peak(&b[..beat / 8]);
        let tail = rms(&b[beat * 6 / 10..beat * 95 / 100], 0);
        pk += db(hit) as f64;
        fl += db(tail) as f64;
        n += 1;
    }
    ((pk - fl) / n as f64) as f32
}

#[test]
fn dynamics_punch_adds_and_glue_reduces_contrast() {
    let x = drums(48000 * 4, FS);
    let measure = |d: f32| {
        let mut m = Master::new(FS);
        let y = run(&mut m, &Params { dynamics: d, limiter: false, ..Params::default() }, &x);
        let lat = m.latency();
        (contrast_db(&y[48000 + lat..], FS), crest_db(&y[48000..]))
    };
    let r: Vec<(f32, f32, f32)> = [-1.0f32, -0.5, 0.0, 0.5, 1.0].iter().map(|&d| { let (c, k) = measure(d); (d, c, k) }).collect();
    for (d, c, k) in &r {
        println!("dynamics {:+.1}: hit/floor contrast {c:.2} dB, crest {k:.2} dB", d);
    }
    let c: Vec<f32> = r.iter().map(|v| v.1).collect();
    assert!(c[4] > c[2] + 4.0, "punch should add >4 dB contrast: {} -> {}", c[2], c[4]);
    assert!(c[0] < c[2] - 3.0, "glue should remove >3 dB contrast: {} -> {}", c[2], c[0]);
    assert!(c.windows(2).all(|w| w[0] < w[1]), "DYNAMICS must be monotonic: {c:?}");
    assert!(r[0].2 < r[2].2, "glue must lower the crest factor");
}

#[test]
fn glue_keeps_loudness_roughly_with_auto_makeup() {
    let x = drums(48000 * 6, FS);
    let mut m = Master::new(FS);
    let y0 = run(&mut m, &Params { limiter: false, ..Params::default() }, &x);
    let mut m = Master::new(FS);
    let y1 = run(&mut m, &Params { dynamics: -1.0, limiter: false, ..Params::default() }, &x);
    let d = db(rms(&y1[96000..], 0)) - db(rms(&y0[96000..], 0));
    assert!(d.abs() < 4.0, "glue loudness change {d} dB");
}

#[test]
fn dynamics_does_not_pump_up_silence_or_noise_floor() {
    let mut r = Rng(5);
    let x: Vec<[f32; 2]> = (0..96000).map(|_| { let v = r.bi() * 1e-4; [v, v] }).collect(); // -80 dBFS noise
    for d in [-1.0f32, 1.0] {
        let mut m = Master::new(FS);
        let y = run(&mut m, &Params { dynamics: d, ..Params::default() }, &x);
        let g = db(rms(&y[48000..], 0)) - db(rms(&x[48000..], 0));
        assert!(g.abs() < 1.0, "d {d}: noise floor changed by {g} dB");
    }
    let mut m = Master::new(FS);
    let y = run(&mut m, &Params { dynamics: 1.0, drive: 1.0, gain_db: 12.0, ..Params::default() }, &vec![[0.0; 2]; 48000]);
    assert!(peak(&y) == 0.0, "silence in, silence out");
}

#[test]
fn no_clicks_when_parameters_jump() {
    // a low sine: any discontinuity shows up as a big sample-to-sample step
    let x = sine(80.0, 0.5, 48000 * 2, FS);
    let mut m = Master::new(FS);
    let mut y = Vec::with_capacity(x.len());
    // like nih-plug's 40 ms linear smoother on the DYNAMICS parameter
    let (mut dynv, step) = (0.0f32, 2.0 / (0.040 * FS));
    for (n, s) in x.iter().enumerate() {
        let k = (n / 9000) % 4;
        let target = [-1.0f32, 1.0, 0.0, 1.0][k];
        dynv += (target - dynv).clamp(-step, step);
        let p = Params {
            dynamics: dynv,
            bass_mono: k % 2 == 1,
            power: k != 2,
            limiter: false,
            ..Params::default()
        };
        let mut v = *s;
        m.process(&p, &mut v, 2);
        y.push(v);
    }
    let (imax, max_step) = y.windows(2).map(|w| (w[1][0] - w[0][0]).abs()).enumerate().fold((0, 0.0f32), |a, (i, v)| if v > a.1 { (i, v) } else { a });
    println!("max step {max_step} at {imax} (segment {})", imax / 9000);
    let sine_step = TAU * 80.0 / FS * 0.5 * 2.5; // generous: gain moves up to ~+8 dB
    assert!(max_step < sine_step * 1.6, "click: step {max_step} vs {sine_step}");
}

#[test]
fn drive_adds_harmonics_tape_odd_tube_even() {
    let x = sine(1000.0, 0.5, 48000, FS);
    let measure = |drive: f32, ch: Character| {
        let mut m = Master::new(FS);
        let y = run(&mut m, &Params { drive, character: ch, limiter: false, ..Params::default() }, &x);
        let t = &y[24000..];
        let f1 = tone(t, 0, 1000.0, FS);
        (db(tone(t, 0, 2000.0, FS)) - db(f1), db(tone(t, 0, 3000.0, FS)) - db(f1))
    };
    let (h2_0, h3_0) = measure(0.0, Character::Tape);
    let (h2_t, h3_t) = measure(1.0, Character::Tape);
    let (h2_u, _h3_u) = measure(1.0, Character::Tube);
    println!("clean h2 {h2_0:.1} h3 {h3_0:.1} | tape h2 {h2_t:.1} h3 {h3_t:.1} | tube h2 {h2_u:.1}");
    assert!(h3_0 < -90.0 && h2_0 < -90.0);
    assert!(h3_t > -40.0, "tape must add 3rd harmonic");
    assert!(h2_t < -60.0, "tape is symmetric");
    assert!(h2_u > -40.0, "tube must add 2nd harmonic");
}

#[test]
fn drive_is_aliasing_controlled() {
    // 9 kHz at full drive: the 3rd harmonic (27 kHz) folds to 21 kHz at 48k. Oversampling keeps it low.
    let mut m = Master::new(FS);
    let y = run(&mut m, &Params { drive: 1.0, limiter: false, ..Params::default() }, &sine(9000.0, 0.5, 48000, FS));
    let t = &y[24000..];
    let alias = db(tone(t, 0, 21000.0, FS)) - db(tone(t, 0, 9000.0, FS));
    assert!(alias < -30.0, "alias {alias} dB");
}

#[test]
fn gain_and_input_are_exact() {
    let mut m = Master::new(FS);
    let y = run(&mut m, &Params { gain_db: 6.0, input_db: -3.0, limiter: false, ..Params::default() }, &sine(1000.0, 0.1, 48000, FS));
    let g = db(tone(&y[24000..], 0, 1000.0, FS)) - db(0.1);
    assert!((g - 3.0).abs() < 0.05, "{g}");
}

#[test]
fn every_sample_rate_and_random_setting_stays_finite() {
    let mut r = Rng(99);
    for fs in [8000.0f32, 22050.0, 44100.0, 48000.0, 88200.0, 96000.0, 192000.0, 384000.0] {
        for _ in 0..12 {
            let p = Params {
                power: r.f() > 0.1,
                input_db: -12.0 + 24.0 * r.f(),
                lowcut_hz: 10.0 + 70.0 * r.f(),
                low_db: -8.0 + 16.0 * r.f(),
                body_db: -6.0 + 12.0 * r.f(),
                presence_db: -6.0 + 12.0 * r.f(),
                air_db: -8.0 + 16.0 * r.f(),
                tilt_db: -4.0 + 8.0 * r.f(),
                width: 2.0 * r.f(),
                bass_mono: r.f() > 0.5,
                dynamics: r.bi(),
                drive: r.f(),
                character: if r.f() > 0.5 { Character::Tape } else { Character::Tube },
                gain_db: 12.0 * r.f(),
                ceiling_db: -6.0 * r.f(),
                limiter: r.f() > 0.3,
            };
            let mut m = Master::new(fs);
            for n in 0..(fs as usize / 2) {
                let s = match (n / 1500) % 6 {
                    0 => r.bi(),
                    1 => 0.0,
                    2 => if (n / 20) % 2 == 0 { 1.0 } else { -1.0 },
                    3 => 1.0, // DC
                    4 => 1e-30, // denormal territory
                    _ => r.bi() * 8.0, // way over full scale
                };
                let mut x = [s, -s * 0.5];
                m.process(&p, &mut x, 2);
                assert!(x[0].is_finite() && x[1].is_finite(), "fs {fs} n {n} {p:?}");
                assert!(x[0].abs() < 40.0 && x[1].abs() < 40.0, "fs {fs} n {n}: {:?} {p:?}", x);
                if p.limiter && p.power {
                    let c = 10f32.powf(p.ceiling_db / 20.0) + 1e-6;
                    assert!(x[0].abs() <= c && x[1].abs() <= c, "ceiling broken fs {fs}");
                }
            }
        }
    }
}

#[test]
fn mono_channel_works() {
    let mut m = Master::new(FS);
    let p = Params { low_db: 3.0, dynamics: 0.6, drive: 0.4, gain_db: 6.0, ..Params::default() };
    let mut pk = 0.0f32;
    for n in 0..48000 {
        let mut x = [0.7 * (TAU * 100.0 * n as f32 / FS).sin(), 123.0];
        m.process(&p, &mut x, 1);
        assert!(x[0].is_finite());
        pk = pk.max(x[0].abs());
    }
    assert!(pk > 0.5 && pk <= 10f32.powf(-0.3 / 20.0) + 1e-6);
}

#[test]
fn reset_clears_all_state() {
    let mut m = Master::new(FS);
    let p = Params { dynamics: 1.0, drive: 1.0, gain_db: 12.0, ..Params::default() };
    run(&mut m, &p, &drums(48000, FS));
    m.reset();
    let y = run(&mut m, &p, &vec![[0.0; 2]; 4800]);
    assert_eq!(peak(&y), 0.0);
}

#[test]
fn realtime_performance() {
    let x = drums(48000 * 10, FS);
    let p = Params { dynamics: 0.5, drive: 0.5, low_db: 2.0, air_db: 2.0, bass_mono: true, ..Params::default() };
    let mut m = Master::new(FS);
    let t = std::time::Instant::now();
    let y = run(&mut m, &p, &x);
    let secs = t.elapsed().as_secs_f32();
    println!("10 s of stereo audio processed in {:.3} s ({:.1}x realtime)", secs, 10.0 / secs);
    assert!(y.len() == x.len());
    assert!(secs < 2.0, "too slow: {secs}s for 10s");
}

#[test]
fn recovers_after_nan_or_inf_from_the_host() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut a = Master::new(48000.0);
        let p = Params::default();
        for n in 0..96000 {
            let v = if (100..110).contains(&n) { bad } else { 0.3 * (n as f32 * 0.05).sin() };
            let mut x = [v, v];
            a.process(&p, &mut x, 2);
            if n > 48000 {
                assert!(x[0].is_finite() && x[1].is_finite(), "daubiko {bad}: still broken at n {n}: {x:?}");
            }
        }
    }
}
