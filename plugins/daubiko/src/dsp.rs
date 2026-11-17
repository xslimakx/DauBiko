//! DauBiko DSP: input, tone EQ, stereo width, DYNAMICS, analog drive, gain and lookahead limiter.
//!
//! Signal flow (per sample, stereo-linked):
//! input -> low cut -> LOW / BODY / PRESENCE / AIR / TILT -> width (+ bass mono)
//!       -> DYNAMICS (punch / glue) -> drive (2x oversampled) -> gain -> limiter -> out
use nyashk_common::dsp::*;
use std::f32::consts::PI;
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Character {
    Tape,
    Tube,
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub power: bool,
    pub input_db: f32,
    pub lowcut_hz: f32,
    pub low_db: f32,
    pub body_db: f32,
    pub presence_db: f32,
    pub air_db: f32,
    pub tilt_db: f32,
    /// 0..2 (1 = unchanged)
    pub width: f32,
    pub bass_mono: bool,
    /// -1 (glue / compress) .. 0 (off) .. +1 (punch / expand)
    pub dynamics: f32,
    /// 0..1
    pub drive: f32,
    pub character: Character,
    pub gain_db: f32,
    pub ceiling_db: f32,
    pub limiter: bool,
}

impl Default for Params {
    /// Neutral settings: the processor is transparent apart from the 10 Hz low cut and the limiter safety.
    fn default() -> Self {
        Self {
            power: true,
            input_db: 0.0,
            lowcut_hz: 10.0,
            low_db: 0.0,
            body_db: 0.0,
            presence_db: 0.0,
            air_db: 0.0,
            tilt_db: 0.0,
            width: 1.0,
            bass_mono: false,
            dynamics: 0.0,
            drive: 0.0,
            character: Character::Tape,
            gain_db: 0.0,
            ceiling_db: -0.3,
            limiter: true,
        }
    }
}

pub const LOW_HZ: f32 = 80.0;
pub const BODY_HZ: f32 = 250.0;
pub const PRES_HZ: f32 = 3200.0;
pub const AIR_HZ: f32 = 12000.0;
pub const TILT_HZ: f32 = 700.0;
pub const MONO_HZ: f32 = 120.0;
pub const EQ_N: usize = 7;
/// extra delay of the true-peak detector (half its interpolation filter)
pub const TP_D: usize = 4;
/// LOW CUT at or below this is OFF (no filter at all).
pub const LOWCUT_OFF_HZ: f32 = 10.5;

/// Tone EQ coefficients: low cut, LOW shelf, BODY bell, PRESENCE bell, AIR shelf, tilt (2 shelves).
pub fn eq_coefs(fs: f32, p: &Params) -> [Coef; EQ_N] {
    [
        if p.lowcut_hz <= LOWCUT_OFF_HZ { Coef::THRU } else { Coef::highpass(fs, p.lowcut_hz, std::f32::consts::FRAC_1_SQRT_2) },
        Coef::low_shelf(fs, LOW_HZ, p.low_db, 0.6),
        Coef::peaking(fs, BODY_HZ, p.body_db, 0.8),
        Coef::peaking(fs, PRES_HZ, p.presence_db, 0.7),
        Coef::high_shelf(fs, AIR_HZ, p.air_db, 0.6),
        Coef::low_shelf(fs, TILT_HZ, -0.5 * p.tilt_db, 0.5),
        Coef::high_shelf(fs, TILT_HZ, 0.5 * p.tilt_db, 0.5),
    ]
}

/// Magnitude of the tone EQ at `f` (for the display).
pub fn eq_curve_db(fs: f32, p: &Params, f: f32) -> f32 {
    eq_coefs(fs, p).iter().map(|c| c.response_db(fs, f)).sum()
}

/// DYNAMICS gain in dB.
/// * `fast_db` / `slow_db`: fast (0.3 ms / 40 ms) and slow (30 ms / 300 ms) peak envelopes.
/// * `glue_db`: medium (5 ms / 150 ms) envelope for the bus compressor.
/// d > 0 (PUNCH): transients are lifted (fast above slow) and decaying tails are lowered.
/// d < 0 (GLUE): soft-knee bus compression up to 4:1 plus softened transients.
pub fn dyn_gain_db(d: f32, fast_db: f32, slow_db: f32, glue_db: f32) -> f32 {
    let diff = (fast_db - slow_db).clamp(0.0, 24.0);
    let tail = (slow_db - fast_db).clamp(0.0, 12.0);
    // do not pump up noise / reverb tails of very quiet material
    let gate = ((slow_db + 70.0) / 20.0).clamp(0.0, 1.0);
    if d >= 0.0 {
        gate * d * ((diff * 0.7).min(9.0) - (tail * 0.6).min(6.0))
    } else {
        let a = -d;
        let thr = -6.0 - 18.0 * a;
        let ratio = 1.0 + 3.0 * a;
        let knee = 8.0;
        let over = glue_db - thr;
        let slope = 1.0 / ratio - 1.0;
        let comp = if over <= -knee * 0.5 {
            0.0
        } else if over < knee * 0.5 {
            slope * (over + knee * 0.5).powi(2) / (2.0 * knee)
        } else {
            slope * over
        };
        comp - gate * a.powf(1.5) * (diff * 0.7).min(8.0)
    }
}

/// O(1) amortised sliding-window minimum.
struct MinWin {
    q: VecDeque<(u64, f32)>,
    len: u64,
    n: u64,
}
impl MinWin {
    fn new(len: usize) -> Self {
        Self { q: VecDeque::with_capacity(len + 2), len: len as u64, n: 0 }
    }
    #[inline]
    fn push(&mut self, v: f32) -> f32 {
        while let Some(&(_, b)) = self.q.back() {
            if b >= v {
                self.q.pop_back();
            } else {
                break;
            }
        }
        self.q.push_back((self.n, v));
        while let Some(&(i, _)) = self.q.front() {
            if i + self.len <= self.n {
                self.q.pop_front();
            } else {
                break;
            }
        }
        self.n += 1;
        self.q.front().map(|x| x.1).unwrap_or(v)
    }
    fn reset(&mut self) {
        self.q.clear();
        self.n = 0;
    }
}

/// Fixed delay line.
struct Delay {
    buf: Vec<f32>,
    pos: usize,
}
impl Delay {
    fn new(len: usize) -> Self {
        Self { buf: vec![0.0; len.max(1)], pos: 0 }
    }
    /// Push `x`, return the sample from `len` pushes ago (len 0 -> pass-through handled by caller).
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }
}

pub struct Master {
    fs: f32,
    // EQ
    coefs: [Coef; EQ_N],
    eq: [Chain<EQ_N>; 2],
    eq_count: usize,
    // width
    mono_c: Coef,
    side_hp: Bq,
    mono_mix: Smooth,
    // dynamics
    fast: Env,
    slow: Env,
    glue: Env,
    c_glue: (f32, f32),
    gr_avg: f32,
    dyn_s: Smooth,
    c_dyn_down: f32,
    c_dyn_up: f32,
    c_fast: (f32, f32),
    c_slow: (f32, f32),
    c_avg: f32,
    // drive
    os: [Os2; 2],
    dc_c: Coef,
    dc: [Bq; 2],
    // limiter
    look: usize,
    minw: MinWin,
    boxbuf: Vec<f32>,
    boxpos: usize,
    boxsum: f64,
    lim_g: f32,
    c_rel: f32,
    /// true-peak detection: the last 8 samples (with gain) per channel and the 4x interpolation
    tp_hist: [[f32; 8]; 2],
    tp_h: [[f32; 8]; 3],
    delay: [Delay; 2],
    dyn_la: usize,
    dyn_delay: [Delay; 2],
    dry_delay: [Delay; 2],
    bypass: Smooth,
    // meters (read after each sample)
    pub in_peak: f32,
    pub out_peak: f32,
    pub dyn_db: f32,
    pub lim_db: f32,
    vu_in: f32,
    vu_out: f32,
    c_vu: f32,
}

impl Master {
    pub fn new(fs: f32) -> Self {
        let look = ((fs * 0.0015).round() as usize).max(1);
        let dyn_la = (fs * 0.001).round() as usize;
        let p = Params::default();
        Self {
            fs,
            coefs: eq_coefs(fs, &p),
            eq: [Chain::new(), Chain::new()],
            eq_count: 0,
            mono_c: Coef::highpass(fs, MONO_HZ, std::f32::consts::FRAC_1_SQRT_2),
            side_hp: Bq::default(),
            mono_mix: Smooth::new(0.0, fs, 20.0),
            fast: Env::default(),
            slow: Env::default(),
            glue: Env::default(),
            c_glue: (time_coef(1.0, fs), time_coef(150.0, fs)),
            gr_avg: 0.0,
            dyn_s: Smooth::new(0.0, fs, 2.0),
            c_dyn_down: time_coef(0.4, fs),
            c_dyn_up: time_coef(3.0, fs),
            c_fast: (time_coef(0.3, fs), time_coef(40.0, fs)),
            c_slow: (time_coef(30.0, fs), time_coef(300.0, fs)),
            c_avg: time_coef(2000.0, fs),
            os: [Os2::new(fs), Os2::new(fs)],
            dc_c: Coef::highpass1(fs, 5.0),
            dc: [Bq::default(); 2],
            look,
            minw: MinWin::new(look),
            boxbuf: vec![1.0; look],
            boxpos: 0,
            boxsum: look as f64,
            lim_g: 1.0,
            c_rel: time_coef(80.0, fs),
            tp_hist: [[0.0; 8]; 2],
            tp_h: {
                // windowed-sinc taps for the points 1/4, 2/4, 3/4 between samples 3 and 4
                let mut h = [[0.0f32; 8]; 3];
                for (q, row) in h.iter_mut().enumerate() {
                    let t = 3.0 + (q as f32 + 1.0) / 4.0;
                    for (i, c) in row.iter_mut().enumerate() {
                        let d = t - i as f32;
                        let sinc = if d.abs() < 1e-6 { 1.0 } else { (PI * d).sin() / (PI * d) };
                        *c = sinc * (0.5 + 0.5 * (PI * d / 4.5).cos());
                    }
                }
                h
            },
            delay: [Delay::new(look - 1 + TP_D), Delay::new(look - 1 + TP_D)],
            dyn_la,
            dyn_delay: [Delay::new(dyn_la), Delay::new(dyn_la)],
            dry_delay: [Delay::new(look - 1 + dyn_la + TP_D), Delay::new(look - 1 + dyn_la + TP_D)],
            bypass: Smooth::new(1.0, fs, 10.0),
            in_peak: 0.0,
            out_peak: 0.0,
            dyn_db: 0.0,
            lim_db: 0.0,
            vu_in: 0.0,
            vu_out: 0.0,
            c_vu: time_coef(300.0, fs),
        }
    }

    /// Latency in samples (constant: DYNAMICS lookahead 1 ms + limiter lookahead 1.5 ms, also when they are off).
    pub fn latency(&self) -> usize {
        self.dyn_la + self.look - 1 + TP_D
    }

    pub fn vu_in_db(&self) -> f32 {
        10.0 * (self.vu_in + 1e-12).log10()
    }
    pub fn vu_out_db(&self) -> f32 {
        10.0 * (self.vu_out + 1e-12).log10()
    }

    pub fn reset(&mut self) {
        for c in self.eq.iter_mut() {
            c.reset();
        }
        self.side_hp.reset();
        self.fast = Env::default();
        self.slow = Env::default();
        self.glue = Env::default();
        self.gr_avg = 0.0;
        self.dyn_s.v = 0.0;
        for o in self.os.iter_mut() {
            o.reset();
        }
        for d in self.dc.iter_mut() {
            d.reset();
        }
        self.minw.reset();
        self.boxbuf.iter_mut().for_each(|v| *v = 1.0);
        self.boxpos = 0;
        self.boxsum = self.look as f64;
        self.lim_g = 1.0;
        self.tp_hist = [[0.0; 8]; 2];
        for d in self.delay.iter_mut().chain(self.dry_delay.iter_mut()).chain(self.dyn_delay.iter_mut()) {
            d.reset();
        }
        self.vu_in = 0.0;
        self.vu_out = 0.0;
    }

    /// Process one frame in place. `n` = number of channels (1 or 2).
    pub fn process(&mut self, p: &Params, x: &mut [f32; 2], n: usize) {
        let n = n.clamp(1, 2);
        let dry = *x;
        // ---- input
        let gin = db_to_lin(p.input_db);
        let mut s = [x[0] * gin, if n > 1 { x[1] * gin } else { 0.0 }];
        let ip = s[0].abs().max(s[1].abs());
        self.in_peak = ip;
        let ms_in = if n > 1 { 0.5 * (s[0] * s[0] + s[1] * s[1]) } else { s[0] * s[0] };
        self.vu_in = flush(ms_in + (self.vu_in - ms_in) * self.c_vu);

        // ---- tone EQ (coefficients refreshed every 32 samples)
        if self.eq_count == 0 {
            self.coefs = eq_coefs(self.fs, p);
        }
        self.eq_count = (self.eq_count + 1) % 32;
        for c in 0..n {
            s[c] = self.eq[c].tick(&self.coefs, s[c]);
        }

        // ---- width / bass mono
        if n > 1 {
            let m = 0.5 * (s[0] + s[1]);
            let mut sd = 0.5 * (s[0] - s[1]);
            let bm = self.mono_mix.tick(if p.bass_mono { 1.0 } else { 0.0 });
            let hp = self.side_hp.tick(&self.mono_c, sd);
            sd += bm * (hp - sd);
            sd *= p.width.clamp(0.0, 2.0);
            s = [m + sd, m - sd];
        }

        // ---- DYNAMICS
        let det = s[0].abs().max(s[1].abs());
        let f = self.fast.tick(det, self.c_fast.0, self.c_fast.1);
        let sl = self.slow.tick(det, self.c_slow.0, self.c_slow.1);
        let gl = self.glue.tick(det, self.c_glue.0, self.c_glue.1);
        let (fdb, sdb, gldb) = (lin_to_db(f).max(-120.0), lin_to_db(sl).max(-120.0), lin_to_db(gl).max(-120.0));
        let d = p.dynamics.clamp(-1.0, 1.0);
        let mut gdb = dyn_gain_db(d, fdb, sdb, gldb);
        if d < 0.0 {
            // automatic makeup: give back part of the average gain reduction
            self.gr_avg = gdb.min(0.0) + (self.gr_avg - gdb.min(0.0)) * self.c_avg;
            gdb -= (1.4 * self.gr_avg).max(-12.0);
        } else {
            self.gr_avg *= self.c_avg;
        }
        // gain moves down fast (catch transients) and up smoothly (no ripple)
        let gdb = gdb.clamp(-18.0, 10.0);
        let c = if gdb < self.dyn_s.v { self.c_dyn_down } else { self.c_dyn_up };
        self.dyn_s.v = gdb + (self.dyn_s.v - gdb) * c;
        let gdb = self.dyn_s.v;
        self.dyn_db = gdb;
        let g = db_to_lin(gdb);
        // the gain is applied to the signal 1 ms later, so it is already down when a transient arrives
        for c in 0..n {
            let dl = if self.dyn_la > 0 { self.dyn_delay[c].tick(s[c]) } else { s[c] };
            s[c] = dl * g;
        }

        // ---- analog drive (2x oversampled)
        let drv = p.drive.clamp(0.0, 1.0);
        if drv > 1e-4 {
            let k = db_to_lin(drv * 15.0);
            let comp = db_to_lin(drv * 6.0);
            for c in 0..n {
                let ch = p.character;
                let y = self.os[c].process(s[c], |v| {
                    let w = match ch {
                        Character::Tape => sat_tanh(k * v),
                        Character::Tube => sat_asym(k * v, 0.25),
                    };
                    w / k
                });
                let y = if p.character == Character::Tube { self.dc[c].tick(&self.dc_c, y) } else { y };
                // blend in from 0 so the knob starts clean
                let mix = (drv * 4.0).min(1.0);
                s[c] = (s[c] + mix * (y - s[c])) * comp;
            }
        }

        // ---- gain into the limiter
        let gg = db_to_lin(p.gain_db);
        for v in s.iter_mut().take(n) {
            *v *= gg;
        }

        // ---- lookahead limiter (min-hold + box average, delay look-1, stereo linked)
        let ceil = db_to_lin(p.ceiling_db.min(0.0));
        // true peak: the samples and the points between them (4x), so the output also stays
        // under the ceiling after D/A conversion or encoding (it overshot by up to ~0.4 dB)
        let mut pk = 0.0f32;
        for c in 0..n {
            let h = &mut self.tp_hist[c];
            h.copy_within(1..8, 0);
            h[7] = s[c];
            pk = pk.max(h[3].abs()).max(h[4].abs());
            for row in &self.tp_h {
                let mut v = 0.0;
                for i in 0..8 {
                    v += row[i] * h[i];
                }
                pk = pk.max(v.abs());
            }
        }
        let req = if p.limiter && pk > ceil { ceil / pk } else { 1.0 };
        let held = self.minw.push(req);
        self.boxsum += held as f64 - self.boxbuf[self.boxpos] as f64;
        self.boxbuf[self.boxpos] = held;
        self.boxpos = (self.boxpos + 1) % self.look;
        let avg = (self.boxsum / self.look as f64) as f32;
        self.lim_g = if avg < self.lim_g { avg } else { avg + (self.lim_g - avg) * self.c_rel };
        self.lim_g = self.lim_g.clamp(0.0, 1.0);
        self.lim_db = lin_to_db(self.lim_g).max(-60.0);
        let lat = self.look - 1;
        for c in 0..n {
            let dl = if lat > 0 { self.delay[c].tick(s[c]) } else { s[c] };
            let mut y = dl * self.lim_g;
            if p.limiter {
                y = y.clamp(-ceil, ceil); // safety, should not engage
            }
            s[c] = y;
        }

        // ---- bypass crossfade against the latency-compensated dry signal
        let mut w = self.bypass.tick(if p.power { 1.0 } else { 0.0 });
        if w < 1e-5 {
            w = 0.0;
            self.bypass.v = 0.0;
        } else if w > 1.0 - 1e-5 {
            w = 1.0;
            self.bypass.v = 1.0;
        }
        let mut op = 0.0f32;
        for c in 0..n {
            let dd = if self.latency() > 0 { self.dry_delay[c].tick(dry[c]) } else { dry[c] };
            x[c] = if w == 0.0 { dd } else if w == 1.0 { s[c] } else { dd + w * (s[c] - dd) };
            op = op.max(x[c].abs());
        }
        self.out_peak = op;
        let ms_out = if n > 1 { 0.5 * (x[0] * x[0] + x[1] * x[1]) } else { x[0] * x[0] };
        self.vu_out = flush(ms_out + (self.vu_out - ms_out) * self.c_vu);
    }
}
