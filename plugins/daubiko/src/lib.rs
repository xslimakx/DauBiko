//! DauBiko: analog-style mastering processor (VST3).

use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub mod dsp;
mod editor;
mod pics;

/// Values the audio thread publishes for the GUI.
pub struct Meters {
    pub in_db: AtomicU32,
    pub out_db: AtomicU32,
    pub vu_in: AtomicU32,
    pub vu_out: AtomicU32,
    pub dyn_db: AtomicU32,
    pub lim_db: AtomicU32,
    pub fs: AtomicU32,
    /// live output (mono) for the spectrum on the tone screen
    pub tap: nyashk_common::tap::Tap,
}
impl Meters {
    fn new() -> Self {
        let m = (-90f32).to_bits();
        Self {
            in_db: AtomicU32::new(m),
            out_db: AtomicU32::new(m),
            vu_in: AtomicU32::new(m),
            vu_out: AtomicU32::new(m),
            dyn_db: AtomicU32::new(0f32.to_bits()),
            lim_db: AtomicU32::new(0f32.to_bits()),
            fs: AtomicU32::new(48000f32.to_bits()),
            tap: nyashk_common::tap::Tap::new(8192),
        }
    }
    pub fn get(a: &AtomicU32) -> f32 {
        f32::from_bits(a.load(Ordering::Relaxed))
    }
    fn set(a: &AtomicU32, v: f32) {
        a.store(v.to_bits(), Ordering::Relaxed)
    }
}

#[derive(Enum, PartialEq, Clone, Copy, Debug)]
pub enum CharacterParam {
    #[name = "TAPE"]
    Tape,
    #[name = "TUBE"]
    Tube,
}

#[derive(Params)]
pub struct DauBikoParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    #[id = "power"]
    pub power: BoolParam,
    #[id = "input"]
    pub input: FloatParam,
    #[id = "lowcut"]
    pub lowcut: FloatParam,
    #[id = "low"]
    pub low: FloatParam,
    #[id = "body"]
    pub body: FloatParam,
    #[id = "presence"]
    pub presence: FloatParam,
    #[id = "air"]
    pub air: FloatParam,
    #[id = "tilt"]
    pub tilt: FloatParam,
    #[id = "width"]
    pub width: FloatParam,
    #[id = "bassmono"]
    pub bass_mono: BoolParam,
    #[id = "dynamics"]
    pub dynamics: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,
    #[id = "character"]
    pub character: EnumParam<CharacterParam>,
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "ceiling"]
    pub ceiling: FloatParam,
    #[id = "limiter"]
    pub limiter: BoolParam,
}

fn db_param(name: &str, def: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(name, def, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_step_size(0.1)
        .with_smoother(SmoothingStyle::Linear(30.0))
        .with_value_to_string(Arc::new(|v| format!("{:+.1}", if v.abs() < 0.05 { 0.0 } else { v })))
}

impl Default for DauBikoParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(editor::BASE_W as u32, editor::BASE_H as u32),
            power: BoolParam::new("Power", true),
            input: db_param("Input", 0.0, -12.0, 12.0),
            lowcut: FloatParam::new("Low Cut", 10.0, FloatRange::Skewed { min: 10.0, max: 80.0, factor: FloatRange::skew_factor(-1.0) })
                .with_step_size(1.0)
                .with_smoother(SmoothingStyle::Logarithmic(50.0))
                .with_value_to_string(Arc::new(|v| if v <= 10.5 { "OFF".into() } else { format!("{:.0} Hz", v) }))
                .with_string_to_value(Arc::new(|s| if s.trim().eq_ignore_ascii_case("off") { Some(10.0) } else { s.trim().trim_end_matches("Hz").trim().parse().ok() })),
            low: db_param("Low", 0.0, -8.0, 8.0),
            body: db_param("Body", 0.0, -6.0, 6.0),
            presence: db_param("Presence", 0.0, -6.0, 6.0),
            air: db_param("Air", 0.0, -8.0, 8.0),
            tilt: db_param("Tilt", 0.0, -4.0, 4.0),
            width: FloatParam::new("Width", 100.0, FloatRange::Linear { min: 0.0, max: 200.0 })
                .with_unit(" %")
                .with_step_size(1.0)
                .with_smoother(SmoothingStyle::Linear(30.0)),
            bass_mono: BoolParam::new("Bass Mono", false),
            dynamics: FloatParam::new("Dynamics", 0.0, FloatRange::Linear { min: -100.0, max: 100.0 })
                .with_step_size(1.0)
                .with_smoother(SmoothingStyle::Linear(40.0))
                .with_value_to_string(Arc::new(|v| {
                    if v.abs() < 0.5 {
                        "0".into()
                    } else if v > 0.0 {
                        format!("PUNCH {:.0}", v)
                    } else {
                        format!("GLUE {:.0}", -v)
                    }
                }))
                .with_string_to_value(Arc::new(|s| {
                    let t = s.trim().to_ascii_uppercase();
                    if let Some(r) = t.strip_prefix("PUNCH") {
                        r.trim().parse::<f32>().ok()
                    } else if let Some(r) = t.strip_prefix("GLUE") {
                        r.trim().parse::<f32>().ok().map(|v| -v)
                    } else {
                        t.parse().ok()
                    }
                })),
            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 100.0 })
                .with_unit(" %")
                .with_step_size(0.5)
                .with_smoother(SmoothingStyle::Linear(30.0)),
            character: EnumParam::new("Character", CharacterParam::Tape),
            gain: db_param("Gain", 0.0, 0.0, 12.0),
            ceiling: db_param("Ceiling", -0.3, -6.0, 0.0),
            limiter: BoolParam::new("Limiter", true),
        }
    }
}

pub struct DauBiko {
    params: Arc<DauBikoParams>,
    meters: Arc<Meters>,
    dsp: Option<dsp::Master>,
}

impl Default for DauBiko {
    fn default() -> Self {
        Self { params: Arc::new(DauBikoParams::default()), meters: Arc::new(Meters::new()), dsp: None }
    }
}

impl DauBiko {
    fn read_params(&self) -> dsp::Params {
        let p = &self.params;
        dsp::Params {
            power: p.power.value(),
            input_db: p.input.smoothed.next(),
            lowcut_hz: p.lowcut.smoothed.next(),
            low_db: p.low.smoothed.next(),
            body_db: p.body.smoothed.next(),
            presence_db: p.presence.smoothed.next(),
            air_db: p.air.smoothed.next(),
            tilt_db: p.tilt.smoothed.next(),
            width: p.width.smoothed.next() / 100.0,
            bass_mono: p.bass_mono.value(),
            dynamics: p.dynamics.smoothed.next() / 100.0,
            drive: p.drive.smoothed.next() / 100.0,
            character: match p.character.value() {
                CharacterParam::Tape => dsp::Character::Tape,
                CharacterParam::Tube => dsp::Character::Tube,
            },
            gain_db: p.gain.smoothed.next(),
            ceiling_db: p.ceiling.smoothed.next(),
            limiter: p.limiter.value(),
        }
    }
}

impl Plugin for DauBiko {
    const NAME: &'static str = "DauBiko";
    const VENDOR: &'static str = "DauBiko";
    const URL: &'static str = "https://github.com/xslimakx/DauBiko";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout { main_input_channels: NonZeroU32::new(2), main_output_channels: NonZeroU32::new(2), ..AudioIOLayout::const_default() },
        AudioIOLayout { main_input_channels: NonZeroU32::new(1), main_output_channels: NonZeroU32::new(1), ..AudioIOLayout::const_default() },
    ];
    const SAMPLE_ACCURATE_AUTOMATION: bool = false;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _e: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone(), self.meters.clone())
    }

    fn initialize(&mut self, _l: &AudioIOLayout, config: &BufferConfig, ctx: &mut impl InitContext<Self>) -> bool {
        let m = dsp::Master::new(config.sample_rate);
        ctx.set_latency_samples(m.latency() as u32);
        Meters::set(&self.meters.fs, config.sample_rate);
        self.dsp = Some(m);
        true
    }

    fn reset(&mut self) {
        if let Some(d) = self.dsp.as_mut() {
            d.reset();
        }
    }

    fn process(&mut self, buffer: &mut Buffer, _aux: &mut AuxiliaryBuffers, _c: &mut impl ProcessContext<Self>) -> ProcessStatus {
        nyashk_common::demo::fill(buffer, Meters::get(&self.meters.fs));
        nyashk_common::demo::sanitize(buffer);
        if self.dsp.is_none() {
            return ProcessStatus::Normal;
        }
        let (mut ipk, mut opk, mut dyn_db, mut lim) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for mut frame in buffer.iter_samples() {
            let dp = self.read_params();
            let d = self.dsp.as_mut().unwrap();
            let n = frame.len().min(2);
            let mut x = [0.0f32; 2];
            for (i, s) in frame.iter_mut().enumerate().take(n) {
                x[i] = *s;
            }
            d.process(&dp, &mut x, n);
            self.meters.tap.push(if n == 2 { 0.5 * (x[0] + x[1]) } else { x[0] });
            ipk = ipk.max(d.in_peak);
            opk = opk.max(d.out_peak);
            if d.dyn_db.abs() > dyn_db.abs() {
                dyn_db = d.dyn_db;
            }
            lim = lim.min(d.lim_db);
            for (i, s) in frame.iter_mut().enumerate().take(n) {
                *s = x[i];
            }
        }
        let d = self.dsp.as_ref().unwrap();
        let db = |v: f32| 20.0 * (v + 1e-9).log10();
        Meters::set(&self.meters.in_db, db(ipk));
        Meters::set(&self.meters.out_db, db(opk));
        Meters::set(&self.meters.vu_in, d.vu_in_db());
        Meters::set(&self.meters.vu_out, d.vu_out_db());
        Meters::set(&self.meters.dyn_db, dyn_db);
        Meters::set(&self.meters.lim_db, lim);
        ProcessStatus::Normal
    }
}

impl Vst3Plugin for DauBiko {
    const VST3_CLASS_ID: [u8; 16] = *b"DauBikoMasterV3!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Mastering, Vst3SubCategory::Dynamics];
}

nih_export_vst3!(DauBiko);
