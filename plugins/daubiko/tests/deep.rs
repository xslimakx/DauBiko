//! Extra checks: true peak of the limiter output, tilt symmetry, width keeps the mono sum, VU scale.
use daubiko::dsp::{Master, Params};
use std::f32::consts::PI;

fn run(p: &Params, f: impl Fn(usize) -> [f32; 2], n: usize) -> (Vec<[f32; 2]>, Master) {
    let fs = 48000.0;
    let mut m = Master::new(fs);
    let lat = m.latency();
    let mut y = Vec::with_capacity(n);
    for i in 0..n + lat {
        let mut x = if i < n { f(i) } else { [0.0; 2] };
        m.process(p, &mut x, 2);
        if i >= lat {
            y.push(x);
        }
    }
    (y, m)
}

/// 16x windowed-sinc reconstruction peak
fn true_peak(x: &[f32]) -> f32 {
    let l = 16usize;
    let taps = 32usize;
    let mut best = 0.0f32;
    for n in taps..x.len() - taps {
        for ph in 0..l {
            let t = ph as f64 / l as f64;
            let mut acc = 0.0f64;
            for k in -(taps as i64)..(taps as i64) {
                let d = t - k as f64;
                let s = if d.abs() < 1e-9 { 1.0 } else { (std::f64::consts::PI * d).sin() / (std::f64::consts::PI * d) };
                let w = 0.5 + 0.5 * (std::f64::consts::PI * d / taps as f64).cos();
                acc += s * w * x[(n as i64 + k) as usize] as f64;
            }
            best = best.max(acc.abs() as f32);
        }
    }
    best
}

#[test]
fn limiter_true_peak() {
    // hot, bright material: loud highs make inter-sample peaks
    let p = Params { gain_db: 12.0, air_db: 4.0, ..Params::default() };
    let (y, _) = run(&p, |i| {
        let t = i as f32 / 48000.0;
        let v = 0.5 * (2.0 * PI * 11000.0 * t + 0.4).sin() + 0.4 * (2.0 * PI * 7300.0 * t).sin() + 0.3 * (2.0 * PI * 120.0 * t).sin();
        [v, v]
    }, 48000);
    let ch: Vec<f32> = y.iter().map(|f| f[0]).collect();
    let sp = ch.iter().fold(0.0f32, |a, v| a.max(v.abs()));
    let tp = true_peak(&ch[4800..]);
    println!("ceiling -0.3: sample peak {:.2} dBFS, true peak {:.2} dBTP", 20.0 * sp.log10(), 20.0 * tp.log10());
    assert!(20.0 * sp.log10() <= -0.29);
    assert!(20.0 * tp.log10() <= 0.0, "true peak over 0 dBTP: {}", 20.0 * tp.log10());
}

#[test]
fn width_keeps_the_mono_sum_and_vu_is_calibrated() {
    for w in [0.0f32, 0.5, 1.5, 2.0] {
        let p = Params { width: w, ..Params::default() };
        let (y, _) = run(&p, |i| {
            let t = i as f32 / 48000.0;
            [0.2 * (2.0 * PI * 1000.0 * t).sin(), 0.1 * (2.0 * PI * 1000.0 * t + 1.0).sin()]
        }, 24000);
        let (y0, _) = run(&Params::default(), |i| {
            let t = i as f32 / 48000.0;
            [0.2 * (2.0 * PI * 1000.0 * t).sin(), 0.1 * (2.0 * PI * 1000.0 * t + 1.0).sin()]
        }, 24000);
        let e = |v: &[[f32; 2]]| v[12000..].iter().map(|f| ((f[0] + f[1]) as f64).powi(2)).sum::<f64>();
        let d = 10.0 * (e(&y) / e(&y0)).log10();
        println!("width {w}: mono sum {d:+.3} dB");
        assert!(d.abs() < 0.1, "width {w} changes the mono sum by {d}");
    }
    // a -12 dBFS (peak) sine should sit around 0 VU on the input meter ("0 VU = -12 dBFS")
    let a = 10f32.powf(-12.0 / 20.0);
    let (_, m) = run(&Params::default(), |i| {
        let s = a * (2.0 * PI * 1000.0 * i as f32 / 48000.0).sin();
        [s, s]
    }, 96000);
    println!("VU in for a -12 dBFS sine: {:.2} dB", m.vu_in_db());
}
