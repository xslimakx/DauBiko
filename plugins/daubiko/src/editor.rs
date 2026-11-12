//! DauBiko GUI: a worn analog console. Photographic textures (brushed and scratched faceplate,
//! walnut cheeks, machined aluminium knobs, chrome toggles, glass lamps, aged VU paper) are
//! pre-rendered by tools/render_textures.py; egui only places, rotates and lights them.
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{
        self, epaint::PathStroke, epaint::TextShape, epaint::Vertex, text::LayoutJob, Align2, Color32, ColorImage, CornerRadius, FontId, Mesh, Painter, Pos2, Rect, Response, Sense, Shape,
        Stroke, StrokeKind, TextFormat, TextureHandle, TextureOptions, Ui, Vec2,
    },
};
use nyashk_common::analyzer::Spectrum;
use std::f32::consts::PI;
use std::sync::Arc;
use std::time::Duration;

use crate::{dsp, pics, CharacterParam, DauBikoParams, Meters};

pub const BASE_W: f32 = 1000.0;
pub const BASE_H: f32 = 560.0;

// screen-print and lamp colours
const PRINT: Color32 = Color32::from_rgb(0xe6, 0xdc, 0xc4);
const PRINT_DIM: Color32 = Color32::from_rgb(0xa9, 0x9f, 0x8c);
const AMBER: Color32 = Color32::from_rgb(0xff, 0xb3, 0x47);
const AMBER_DIM: Color32 = Color32::from_rgb(0x5a, 0x3a, 0x10);
const VIOLET: Color32 = Color32::from_rgb(0xb0, 0x7c, 0xff);
const RED: Color32 = Color32::from_rgb(0xd8, 0x3c, 0x34);
const INK: Color32 = Color32::from_rgb(0x22, 0x1a, 0x14);

struct Tex {
    picture: TextureHandle,
    face: TextureHandle,
    wood: TextureHandle,
    vu: TextureHandle,
    knob: TextureHandle,
    knob_rot: TextureHandle,
    big: TextureHandle,
    big_rot: TextureHandle,
    tog_up: TextureHandle,
    tog_dn: TextureHandle,
    red_on: TextureHandle,
    red_off: TextureHandle,
    amb_on: TextureHandle,
    amb_off: TextureHandle,
    screw: TextureHandle,
    plate: TextureHandle,
}

struct Gui {
    tex: Option<Tex>,
    spec: Spectrum,
    needle: [f32; 2],
    dyn_show: f32,
    lim_show: f32,
    out_hold: f32,
}

fn load(ctx: &egui::Context) -> Tex {
    let opt = TextureOptions { magnification: egui::TextureFilter::Linear, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: Some(egui::TextureFilter::Linear) };
    let l = |name: &str, img: ColorImage| ctx.load_texture(name, img, opt);
    Tex {
        picture: l("db_pic", pics::jpeg(pics::PICTURE)),
        face: l("db_face", pics::jpeg(pics::FACEPLATE)),
        wood: l("db_wood", pics::jpeg(pics::WOOD)),
        vu: l("db_vu", pics::jpeg(pics::VU_FACE)),
        knob: l("db_knob", pics::png(pics::KNOB_BASE)),
        knob_rot: l("db_knob_rot", pics::png(pics::KNOB_ROT)),
        big: l("db_big", pics::png(pics::BIGKNOB_BASE)),
        big_rot: l("db_big_rot", pics::png(pics::BIGKNOB_ROT)),
        tog_up: l("db_tog_up", pics::png(pics::TOGGLE_UP)),
        tog_dn: l("db_tog_dn", pics::png(pics::TOGGLE_DOWN)),
        red_on: l("db_red_on", pics::png(pics::LAMP_RED_ON)),
        red_off: l("db_red_off", pics::png(pics::LAMP_RED_OFF)),
        amb_on: l("db_amb_on", pics::png(pics::LAMP_AMBER_ON)),
        amb_off: l("db_amb_off", pics::png(pics::LAMP_AMBER_OFF)),
        screw: l("db_screw", pics::png(pics::SCREW)),
        plate: l("db_plate", pics::png(pics::PLATE)),
    }
}

const UV: Rect = Rect { min: Pos2 { x: 0.0, y: 0.0 }, max: Pos2 { x: 1.0, y: 1.0 } };

fn sprite(p: &Painter, t: &TextureHandle, c: Pos2, size: Vec2) {
    p.image(t.id(), Rect::from_center_size(c, size), UV, Color32::WHITE);
}

/// Textured quad rotated by `a` (radians, clockwise on screen) around its centre.
fn sprite_rot(p: &Painter, t: &TextureHandle, c: Pos2, size: Vec2, a: f32, tint: Color32) {
    let q = quad(c, size.x, size.y, a);
    let uvs = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
    let mut mesh = Mesh::with_texture(t.id());
    for i in 0..4 {
        mesh.vertices.push(Vertex { pos: q[i], uv: uvs[i], color: tint });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
}

pub fn create(params: Arc<DauBikoParams>, meters: Arc<Meters>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    let size_state = editor_state.clone();
    create_egui_editor(
        editor_state,
        Gui { tex: None, spec: Spectrum::new(4096), needle: [0.0; 2], dyn_show: 0.0, lim_show: 0.0, out_hold: -90.0 },
        |ctx, _| {
            let mut v = egui::Visuals::dark();
            v.override_text_color = Some(PRINT);
            v.extreme_bg_color = Color32::from_rgb(0x16, 0x10, 0x08);
            v.selection.bg_fill = Color32::from_rgb(0x6a, 0x48, 0x20);
            v.selection.stroke = Stroke::new(1.0_f32, AMBER);
            ctx.set_visuals(v);
        },
        move |ctx, setter, st| {
            ctx.request_repaint_after(Duration::from_millis(33));
            if st.tex.is_none() {
                st.tex = Some(load(ctx));
            }
            st.spec.update(&meters.tap);
            let p = &*params;
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::BLACK)).show(ctx, |ui| {
                let full = ui.max_rect();
                let o = full.min;
                let at = |x: f32, y: f32| Pos2::new(o.x + x, o.y + y);
                let rect = |x: f32, y: f32, w: f32, h: f32| Rect::from_min_size(at(x, y), Vec2::new(w, h));
                let tex = st.tex.as_ref().unwrap();
                let painter = ui.painter().clone();
                let on = p.power.value();

                // ------------------------------------------------ chassis
                painter.image(tex.face.id(), rect(22.0, 0.0, BASE_W - 44.0, BASE_H), UV, Color32::WHITE);
                for x in [0.0, BASE_W - 22.0] {
                    painter.image(tex.wood.id(), rect(x, 0.0, 22.0, BASE_H), UV, Color32::WHITE);
                }
                // seams between wood and panel
                for x in [22.0, BASE_W - 22.0] {
                    painter.vline(o.x + x, o.y..=o.y + BASE_H, Stroke::new(1.5_f32, Color32::from_black_alpha(220)));
                    painter.vline(o.x + x + if x < 100.0 { 1.5 } else { -1.5 }, o.y..=o.y + BASE_H, Stroke::new(1.0_f32, Color32::from_white_alpha(18)));
                }
                for (x, y) in [(34.0, 12.0), (BASE_W - 34.0, 12.0), (34.0, BASE_H - 12.0), (BASE_W - 34.0, BASE_H - 12.0), (500.0, 12.0), (500.0, BASE_H - 12.0)] {
                    sprite(&painter, &tex.screw, at(x, y), Vec2::splat(17.0));
                }

                // ------------------------------------------------ header: brass name plate
                nameplate(ui, tex, rect(56.0, 11.0, 200.0, 42.0));
                print(&painter, at(276.0, 25.0), Align2::LEFT_CENTER, "ANALOG  MASTERING  PROCESSOR", 11.0, PRINT_DIM);
                print(&painter, at(276.0, 42.0), Align2::LEFT_CENTER, "MODEL DB-1    TONE · DYNAMICS · COLOR · LIMIT", 8.5, Color32::from_rgb(0x84, 0x7a, 0x6e));
                groove_h(&painter, o.x + 36.0, o.x + BASE_W - 36.0, o.y + 62.0);

                // power: jewel + toggle
                lamp(&painter, if on { &tex.red_on } else { &tex.red_off }, at(BASE_W - 122.0, 36.0), 7.0);
                print(&painter, at(BASE_W - 138.0, 36.0), Align2::RIGHT_CENTER, "POWER", 9.0, PRINT);
                if toggle(ui, tex, at(BASE_W - 80.0, 36.0), on, "", "", "Switch the processor on or off (click-free, latency stays the same).") {
                    set_b(setter, &p.power, !on);
                }

                // ------------------------------------------------ VU meters
                let vu_in = Meters::get(&meters.vu_in) + 12.0; // 0 VU = -12 dBFS RMS (mastering levels)
                let vu_out = Meters::get(&meters.vu_out) + 12.0;
                for (i, v) in [vu_in, vu_out].into_iter().enumerate() {
                    let target = vu_pos(v);
                    st.needle[i] += (target - st.needle[i]) * 0.28;
                }
                vu_meter(&painter, &tex.vu, rect(38.0, 74.0, 196.0, 124.0), st.needle[0], "INPUT", on);
                vu_meter(&painter, &tex.vu, rect(248.0, 74.0, 196.0, 124.0), st.needle[1], "OUTPUT", on);

                // ------------------------------------------------ tone screen with live spectrum
                let fs = Meters::get(&meters.fs).clamp(8000.0, 384000.0);
                let dp = dsp::Params {
                    lowcut_hz: p.lowcut.value(),
                    low_db: p.low.value(),
                    body_db: p.body.value(),
                    presence_db: p.presence.value(),
                    air_db: p.air.value(),
                    tilt_db: p.tilt.value(),
                    ..dsp::Params::default()
                };
                eq_screen(ui, rect(38.0, 212.0, 406.0, 140.0), fs, &dp, on, &st.spec);

                // ------------------------------------------------ the picture
                polaroid(ui, at(586.0, 212.0), &tex.picture);

                // ------------------------------------------------ DYNAMICS
                let dyn_box = rect(728.0, 72.0, 236.0, 282.0);
                section(&painter, dyn_box, "DYNAMICS");
                let dyn_c = at(846.0, 190.0);
                big_knob(ui, setter, tex, &p.dynamics, dyn_c, 52.0, on, "Turn RIGHT for PUNCH: transients jump out, tails sit lower, the master breathes.\nTurn LEFT for GLUE: smooth bus compression with automatic makeup.\nCentre = off.");
                print(&painter, at(770.0, 270.0), Align2::CENTER_CENTER, "GLUE", 10.5, VIOLET.gamma_multiply(0.85));
                print(&painter, at(922.0, 270.0), Align2::CENTER_CENTER, "PUNCH", 10.5, AMBER.gamma_multiply(0.85));
                arrow(&painter, at(748.0, 270.0), -1.0, VIOLET.gamma_multiply(0.85));
                arrow(&painter, at(946.0, 270.0), 1.0, AMBER.gamma_multiply(0.85));
                value_plate(ui, setter, &p.dynamics, "dyn", Rect::from_center_size(at(846.0, 293.0), Vec2::new(104.0, 18.0)));
                let dyn_now = Meters::get(&meters.dyn_db);
                st.dyn_show += (dyn_now - st.dyn_show) * if dyn_now.abs() > st.dyn_show.abs() { 0.6 } else { 0.15 };
                led_bar_center(&painter, rect(750.0, 316.0, 192.0, 9.0), if on { st.dyn_show } else { 0.0 }, 12.0);
                print(&painter, at(750.0, 337.0), Align2::LEFT_CENTER, "-12", 8.0, PRINT_DIM);
                print(&painter, at(846.0, 337.0), Align2::CENTER_CENTER, "GAIN CHANGE  dB", 8.0, PRINT_DIM);
                print(&painter, at(942.0, 337.0), Align2::RIGHT_CENTER, "+12", 8.0, PRINT_DIM);

                // ------------------------------------------------ lower row
                let ky = 442.0;
                let kr = 23.0;
                section(&painter, rect(36.0, 360.0, 90.0, 178.0), "INPUT");
                knob(ui, setter, tex, &p.input, "LEVEL", at(81.0, ky), kr, true, "Input level into the processor.");

                section(&painter, rect(134.0, 360.0, 404.0, 178.0), "TONE");
                let tone: [(&FloatParam, &str, bool, &str); 6] = [
                    (&p.lowcut, "LOW CUT", false, "Removes sub rumble below the set frequency (12 dB/oct). OFF at the left stop."),
                    (&p.low, "LOW", true, "Low shelf at 80 Hz: weight and warmth of the bass."),
                    (&p.body, "BODY", true, "Bell at 250 Hz: fullness (up) or less mud (down)."),
                    (&p.presence, "PRESENCE", true, "Bell at 3.2 kHz: vocals and snare forward (up) or less harshness (down)."),
                    (&p.air, "AIR", true, "High shelf at 12 kHz: shine and openness on top."),
                    (&p.tilt, "TILT", true, "Tilts the whole spectrum around 700 Hz: right = brighter, left = darker."),
                ];
                for (i, (prm, name, bip, tip)) in tone.into_iter().enumerate() {
                    knob(ui, setter, tex, prm, name, at(170.0 + 66.4 * i as f32, ky), kr, bip, tip);
                }

                section(&painter, rect(546.0, 360.0, 128.0, 178.0), "STEREO");
                knob(ui, setter, tex, &p.width, "WIDTH", at(586.0, ky), kr, true, "Stereo width: 0 % mono, 100 % unchanged, 200 % twice as wide.");
                let bm = p.bass_mono.value();
                if toggle(ui, tex, at(644.0, ky), bm, "MONO", "BASS", "BASS MONO: makes everything below 120 Hz mono for a tight, centred low end.") {
                    set_b(setter, &p.bass_mono, !bm);
                }
                lamp(&painter, if bm && on { &tex.amb_on } else { &tex.amb_off }, at(644.0, ky + 50.0), 4.5);

                section(&painter, rect(682.0, 360.0, 128.0, 178.0), "COLOR");
                knob(ui, setter, tex, &p.drive, "DRIVE", at(722.0, ky), kr, false, "Analog saturation (2x oversampled). Adds harmonics and density.");
                let tube = p.character.value() == CharacterParam::Tube;
                if toggle(ui, tex, at(780.0, ky), tube, "TUBE", "TAPE", "TAPE: smooth odd harmonics, round peaks.\nTUBE: warm even harmonics.") {
                    set_e(setter, &p.character, if tube { CharacterParam::Tape } else { CharacterParam::Tube });
                }

                section(&painter, rect(818.0, 360.0, 146.0, 178.0), "OUTPUT");
                knob(ui, setter, tex, &p.gain, "GAIN", at(852.0, ky), kr, false, "Gain into the limiter: makes the master louder.");
                knob(ui, setter, tex, &p.ceiling, "CEILING", at(926.0, ky), kr, false, "Highest output peak the limiter allows.");
                let lim = p.limiter.value();
                if push_button(ui, tex, at(852.0, 520.0), lim && on, "LIMIT", "Lookahead peak limiter on / off (1.5 ms lookahead).") {
                    set_b(setter, &p.limiter, !lim);
                }
                let lim_now = Meters::get(&meters.lim_db);
                st.lim_show = if lim_now < st.lim_show { lim_now } else { st.lim_show + (lim_now - st.lim_show) * 0.12 };
                led_bar_gr(&painter, rect(896.0, 514.0, 60.0, 6.0), if on && lim { -st.lim_show } else { 0.0 });
                let out_db = Meters::get(&meters.out_db);
                st.out_hold = if out_db > st.out_hold { out_db } else { st.out_hold - 0.4 };
                painter.text(at(926.0, 530.0), Align2::CENTER_CENTER, if st.out_hold > -60.0 { format!("PK {:+.1}", st.out_hold) } else { "PK  --".into() }, FontId::monospace(8.5), if st.out_hold > -0.05 { RED } else { PRINT_DIM });

                resize_grip(ui, ctx, &size_state, full);
            });
        },
    )
}

// ============================================================================ parameter helpers

fn set_b(setter: &ParamSetter, p: &BoolParam, v: bool) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
fn set_e(setter: &ParamSetter, p: &EnumParam<CharacterParam>, v: CharacterParam) {
    setter.begin_set_parameter(p);
    setter.set_parameter(p, v);
    setter.end_set_parameter(p);
}
fn set_norm<P: Param>(setter: &ParamSetter, p: &P, v: f32) {
    setter.begin_set_parameter(p);
    setter.set_parameter_normalized(p, v.clamp(0.0, 1.0));
    setter.end_set_parameter(p);
}


const A0: f32 = PI * 0.75;
const SWEEP: f32 = PI * 1.5;
fn ang(t: f32) -> f32 {
    A0 + t * SWEEP
}
fn polar(c: Pos2, r: f32, a: f32) -> Pos2 {
    Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
}

/// Screen-printed text: slightly translucent ink on the metal, with a hairline shadow.
fn print(p: &Painter, pos: Pos2, align: Align2, text: &str, size: f32, col: Color32) {
    p.text(pos + Vec2::new(0.0, 0.8), align, text, FontId::proportional(size), Color32::from_black_alpha(150));
    p.text(pos, align, text, FontId::proportional(size), col.gamma_multiply(0.92));
}

/// Stamped groove: dark line with a light lip under it.
fn groove_h(p: &Painter, x0: f32, x1: f32, y: f32) {
    p.hline(x0..=x1, y, Stroke::new(1.0_f32, Color32::from_black_alpha(190)));
    p.hline(x0..=x1, y + 1.0, Stroke::new(1.0_f32, Color32::from_white_alpha(20)));
}

/// Section: stamped outline with a screen-printed title.
fn section(p: &Painter, r: Rect, title: &str) {
    p.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.0_f32, Color32::from_black_alpha(170)), StrokeKind::Inside);
    p.rect_stroke(r.translate(Vec2::new(0.0, 1.0)), CornerRadius::same(5), Stroke::new(1.0_f32, Color32::from_white_alpha(16)), StrokeKind::Inside);
    print(p, Pos2::new(r.left() + 10.0, r.top() + 12.0), Align2::LEFT_CENTER, title, 9.5, PRINT_DIM);
}

/// Copper name plate (pre-rendered) with the engraved logo.
fn nameplate(ui: &Ui, tex: &Tex, r: Rect) {
    let p = ui.painter();
    // the texture has a 5 px margin (at layout scale) around the plate for its shadow
    p.image(tex.plate.id(), r.expand(5.0), UV, Color32::WHITE);
    let mut job = LayoutJob::default();
    job.append("DauBiko", 0.0, TextFormat { font_id: FontId::proportional(26.0), color: Color32::WHITE, extra_letter_spacing: 3.0, ..Default::default() });
    let g = ui.fonts(|f| f.layout_job(job));
    let pos = Pos2::new(r.center().x - g.size().x / 2.0, r.center().y - g.size().y / 2.0);
    // engraved: bright lower lip, dark upper wall, deep cut
    p.galley_with_override_text_color(pos + Vec2::new(0.0, 1.2), g.clone(), Color32::from_rgba_unmultiplied(255, 214, 170, 130));
    p.galley_with_override_text_color(pos + Vec2::new(0.0, -0.7), g.clone(), Color32::from_rgba_unmultiplied(30, 12, 4, 150));
    p.galley_with_override_text_color(pos, g, Color32::from_rgb(0x3a, 0x1c, 0x0c));
}

fn lamp(p: &Painter, t: &TextureHandle, c: Pos2, dome_r: f32) {
    // the dome is 9.5 px of a 48 px sprite
    sprite(p, t, c, Vec2::splat(48.0 * dome_r / 9.5));
}

/// Knob mouse handling: drag (shift = fine), wheel, double-click resets.
fn knob_input<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P, hit: Rect, tip: &str) -> Response {
    let resp = ui.allocate_rect(hit, Sense::click_and_drag());
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    let norm = p.unmodulated_normalized_value();
    let raw_id = egui::Id::new(("db_knob_raw", p.name()));
    if resp.drag_started() {
        setter.begin_set_parameter(p);
        ui.data_mut(|d| d.insert_temp(raw_id, norm));
    }
    if resp.dragged() {
        let fine = ui.input(|i| i.modifiers.shift);
        let d = -resp.drag_delta().y / if fine { 1250.0 } else { 240.0 };
        let raw = ui.data(|dd| dd.get_temp::<f32>(raw_id)).unwrap_or(norm) + d;
        ui.data_mut(|dd| dd.insert_temp(raw_id, raw));
        setter.set_parameter_normalized(p, raw.clamp(0.0, 1.0));
    }
    if resp.drag_stopped() {
        setter.end_set_parameter(p);
    }
    if resp.double_clicked() {
        set_norm(setter, p, p.default_normalized_value());
    }
    if resp.hovered() && !resp.dragged() {
        let sc = ui.input(|i| i.smooth_scroll_delta.y);
        if sc != 0.0 {
            set_norm(setter, p, norm + sc / 1500.0);
        }
    }
    resp
}

/// Machined aluminium knob on a bakelite skirt, printed scale and label, amber read-out under it.
#[allow(clippy::too_many_arguments)]
fn knob<P: Param>(ui: &mut Ui, setter: &ParamSetter, tex: &Tex, p: &P, label: &str, c: Pos2, r: f32, bipolar: bool, tip: &str) {
    let hit = Rect::from_center_size(c, Vec2::splat(2.0 * r + 14.0));
    knob_input(ui, setter, p, hit, tip);
    let painter = ui.painter().clone();
    let t = p.unmodulated_normalized_value();
    print(&painter, c - Vec2::new(0.0, r + 28.0), Align2::CENTER_CENTER, label, 9.0, PRINT);
    for i in 0..=10 {
        let tt = i as f32 / 10.0;
        let major = i % 5 == 0;
        let a = ang(tt);
        painter.line_segment([polar(c, r + 5.0, a) + Vec2::new(0.0, 0.7), polar(c, r + if major { 11.0 } else { 8.0 }, a) + Vec2::new(0.0, 0.7)], Stroke::new(if major { 1.5_f32 } else { 1.0 }, Color32::from_black_alpha(140)));
        painter.line_segment([polar(c, r + 5.0, a), polar(c, r + if major { 11.0 } else { 8.0 }, a)], Stroke::new(if major { 1.5_f32 } else { 1.0 }, if major { PRINT } else { PRINT_DIM }));
    }
    if bipolar {
        painter.circle_filled(polar(c, r + 14.5, ang(0.5)), 1.5, PRINT);
    }
    // sprite: knob radius is 0.36 of the sprite size
    // sprite: knob radius is 0.30 of the sprite size (room for the soft shadow)
    let size = Vec2::splat(2.0 * r / 0.60);
    sprite(&painter, &tex.knob, c, size);
    sprite_rot(&painter, &tex.knob_rot, c, size, (t - 0.5) * SWEEP, Color32::WHITE);
    value_plate(ui, setter, p, label, Rect::from_center_size(c + Vec2::new(0.0, r + 25.0), Vec2::new(60.0, 16.0)));
}

/// The big DYNAMICS knob: knurled aluminium with an LED ring (violet = glue, amber = punch).
#[allow(clippy::too_many_arguments)]
fn big_knob(ui: &mut Ui, setter: &ParamSetter, tex: &Tex, p: &FloatParam, c: Pos2, r: f32, lit: bool, tip: &str) {
    let hit = Rect::from_center_size(c, Vec2::splat(2.0 * r + 24.0));
    knob_input(ui, setter, p, hit, tip);
    let painter = ui.painter().clone();
    let t = p.unmodulated_normalized_value();
    let n = 31;
    for i in 0..n {
        let tt = i as f32 / (n - 1) as f32;
        let pos = polar(c, r + 18.0, ang(tt));
        let on = lit && ((tt >= 0.5 && tt <= t + 1e-4) || (tt <= 0.5 && tt >= t - 1e-4));
        let col = if tt < 0.5 { VIOLET } else if tt > 0.5 { AMBER } else { PRINT };
        // socket
        painter.circle_filled(pos, 3.6, Color32::from_rgb(0x0e, 0x0b, 0x0e));
        painter.circle_stroke(pos, 3.6, Stroke::new(0.8_f32, Color32::from_white_alpha(22)));
        if on {
            painter.circle_filled(pos, 7.0, col.gamma_multiply(0.14));
            painter.circle_filled(pos, 2.7, col);
            painter.circle_filled(pos + Vec2::new(-0.8, -0.9), 1.0, Color32::from_white_alpha(200));
        } else {
            painter.circle_filled(pos, 2.6, col.gamma_multiply(0.16).to_opaque());
            painter.circle_filled(pos + Vec2::new(-0.8, -0.9), 0.8, Color32::from_white_alpha(50));
        }
    }
    let size = Vec2::splat(2.0 * r / 0.64);
    sprite(&painter, &tex.big, c, size);
    sprite_rot(&painter, &tex.big_rot, c, size, (t - 0.5) * SWEEP, Color32::WHITE);
}

/// Chrome bat-handle toggle. Up = `on`. Returns true when clicked.
fn toggle(ui: &mut Ui, tex: &Tex, c: Pos2, on: bool, top: &str, bottom: &str, tip: &str) -> bool {
    let hit = Rect::from_center_size(c, Vec2::new(44.0, 56.0));
    let resp = ui.allocate_rect(hit, Sense::click());
    let resp = if tip.is_empty() { resp } else { resp.on_hover_text(tip) };
    let p = ui.painter();
    print(p, c - Vec2::new(0.0, 51.0), Align2::CENTER_CENTER, top, 8.0, if on { PRINT } else { PRINT_DIM });
    if !bottom.is_empty() {
        print(p, c + Vec2::new(0.0, 35.0), Align2::CENTER_CENTER, bottom, 8.0, if on { PRINT_DIM } else { PRINT });
    }
    sprite(p, if on { &tex.tog_up } else { &tex.tog_dn }, c, Vec2::new(44.0, 60.5));
    resp.clicked()
}

/// Square rubber push button with a lit window.
fn push_button(ui: &mut Ui, _tex: &Tex, c: Pos2, on: bool, label: &str, tip: &str) -> bool {
    let r = Rect::from_center_size(c, Vec2::new(50.0, 20.0));
    let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tip);
    let p = ui.painter();
    let pressed = resp.is_pointer_button_down_on();
    p.rect_filled(r.expand(2.0).translate(Vec2::new(0.0, 1.5)), CornerRadius::same(4), Color32::from_black_alpha(170));
    p.rect_filled(r.expand(2.0), CornerRadius::same(4), Color32::from_rgb(0x1a, 0x16, 0x18));
    let face = if pressed { r.translate(Vec2::new(0.0, 1.0)) } else { r };
    let mut m = Mesh::default();
    let (a, b) = if on { (Color32::from_rgb(0x7a, 0x4a, 0x12), Color32::from_rgb(0x4a, 0x2a, 0x08)) } else { (Color32::from_rgb(0x3a, 0x34, 0x36), Color32::from_rgb(0x22, 0x1e, 0x20)) };
    m.colored_vertex(face.left_top(), a);
    m.colored_vertex(face.right_top(), a);
    m.colored_vertex(face.right_bottom(), b);
    m.colored_vertex(face.left_bottom(), b);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
    if on {
        p.rect_filled(face.expand(5.0), CornerRadius::same(6), AMBER.gamma_multiply(0.06));
    }
    p.hline(face.left() + 2.0..=face.right() - 2.0, face.top() + 1.0, Stroke::new(1.0_f32, Color32::from_white_alpha(if on { 60 } else { 30 })));
    p.text(face.center(), Align2::CENTER_CENTER, label, FontId::proportional(9.0), if on { Color32::from_rgb(0xff, 0xd8, 0x90) } else { PRINT_DIM });
    resp.clicked()
}

/// Recessed amber read-out behind a little glass window; click to type a value.
fn value_plate<P: Param>(ui: &mut Ui, setter: &ParamSetter, p: &P, id: &str, vr: Rect) {
    let edit_id = egui::Id::new(("db_edit", id));
    let editing: Option<String> = ui.data(|d| d.get_temp::<String>(edit_id));
    let painter = ui.painter().clone();
    painter.rect_filled(vr.expand(1.5), CornerRadius::same(3), Color32::from_black_alpha(160));
    painter.rect_filled(vr, CornerRadius::same(2), Color32::from_rgb(0x10, 0x0a, 0x05));
    painter.hline(vr.left()..=vr.right(), vr.bottom() + 1.5, Stroke::new(1.0_f32, Color32::from_white_alpha(18)));
    if let Some(mut buf) = editing {
        let fresh_id = egui::Id::new(("db_edit_fresh", id));
        let fresh = ui.data(|d| d.get_temp::<bool>(fresh_id)).unwrap_or(false);
        let te = egui::TextEdit::singleline(&mut buf)
            .font(FontId::monospace(10.5))
            .text_color(AMBER)
            .horizontal_align(egui::Align::Center)
            .desired_width(vr.width())
            .frame(false)
            .margin(egui::Margin::symmetric(2, 1));
        let mut out = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(vr), |ui| te.show(ui)).inner;
        let r_te = out.response.clone();
        if fresh {
            r_te.request_focus();
            let len = buf.chars().count();
            out.state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(len))));
            out.state.store(ui.ctx(), r_te.id);
            ui.data_mut(|d| d.insert_temp(fresh_id, false));
        } else if !r_te.has_focus() && !r_te.lost_focus() {
            r_te.request_focus();
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            ui.data_mut(|d| d.remove::<String>(edit_id));
        } else if !fresh && (enter || r_te.lost_focus()) {
            let txt = buf.trim().replace(',', ".");
            if let Some(nv) = p.string_to_normalized_value(&txt) {
                set_norm(setter, p, nv);
            }
            ui.data_mut(|d| d.remove::<String>(edit_id));
        } else {
            ui.data_mut(|d| d.insert_temp(edit_id, buf));
        }
    } else {
        let resp = ui.allocate_rect(vr, Sense::click()).on_hover_text("Click to type a value");
        let val = p.normalized_value_to_string(p.unmodulated_normalized_value(), true);
        painter.text(vr.center() + Vec2::new(0.0, 0.5), Align2::CENTER_CENTER, &val, FontId::monospace(10.5), Color32::from_rgba_unmultiplied(255, 150, 40, 50));
        painter.text(vr.center(), Align2::CENTER_CENTER, &val, FontId::monospace(10.5), AMBER);
        if resp.clicked() {
            let plain = val.trim().split(' ').next().unwrap_or("").to_string();
            let plain = if p.name() == "Dynamics" { val.clone() } else { plain };
            ui.data_mut(|d| d.insert_temp(edit_id, plain));
            ui.data_mut(|d| d.insert_temp(egui::Id::new(("db_edit_fresh", id)), true));
        }
    }
    // glass sheen
    painter.rect_filled(Rect::from_min_max(vr.left_top(), Pos2::new(vr.right(), vr.center().y)), CornerRadius::same(2), Color32::from_white_alpha(7));
}

/// VU needle position 0..1 for a reading in VU (log-amplitude scale like the real meter face).
fn vu_pos(vu: f32) -> f32 {
    let a = |v: f32| 10f32.powf(v / 20.0);
    ((a(vu.clamp(-30.0, 6.0)) - a(-20.0)) / (a(3.0) - a(-20.0))).clamp(-0.03, 1.04)
}

fn vu_meter(p: &Painter, face: &TextureHandle, r: Rect, pos: f32, label: &str, lit: bool) {
    // black bezel with a chrome lip
    let bz = r.expand(6.0);
    p.rect_filled(bz.translate(Vec2::new(1.5, 3.0)), CornerRadius::same(9), Color32::from_black_alpha(150));
    p.rect_filled(bz, CornerRadius::same(9), Color32::from_rgb(0x15, 0x12, 0x14));
    p.rect_stroke(bz, CornerRadius::same(9), Stroke::new(1.0_f32, Color32::from_white_alpha(30)), StrokeKind::Inside);
    p.rect_stroke(r.expand(2.0), CornerRadius::same(5), Stroke::new(2.0_f32, Color32::from_rgb(0x8a, 0x86, 0x88)), StrokeKind::Inside);
    p.rect_stroke(r.expand(2.0), CornerRadius::same(5), Stroke::new(0.8_f32, Color32::from_white_alpha(110)), StrokeKind::Outside);
    // aged paper, lit from behind
    let tint = if lit { Color32::WHITE } else { Color32::from_rgb(0x7c, 0x74, 0x66) };
    p.image(face.id(), r, UV, tint);
    let clip = p.with_clip_rect(r);
    let pivot = Pos2::new(r.center().x, r.bottom() + r.height() * 0.32);
    let rad = r.height() * 1.0;
    let span = 0.95f32;
    let a_of = |t: f32| -PI / 2.0 - span + 2.0 * span * t;
    let arc = |t0: f32, t1: f32, rr: f32| -> Vec<Pos2> { (0..=40).map(|i| polar(pivot, rr, a_of(t0 + (t1 - t0) * i as f32 / 40.0))).collect() };
    clip.add(Shape::line(arc(0.0, vu_pos(0.0), rad), PathStroke::new(1.3_f32, INK)));
    clip.add(Shape::line(arc(vu_pos(0.0), 1.0, rad + 3.0), PathStroke::new(5.0_f32, RED.gamma_multiply(0.9))));
    for v in [-20.0f32, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0] {
        let t = vu_pos(v);
        let a = a_of(t);
        let major = matches!(v as i32, -20 | -10 | -7 | -5 | -3 | 0 | 3);
        clip.line_segment([polar(pivot, rad, a), polar(pivot, rad + if major { 9.0 } else { 5.0 }, a)], Stroke::new(1.2_f32, if v > 0.0 { RED } else { INK }));
        if major {
            let txt = if v > 0.0 { format!("+{}", v as i32) } else { format!("{}", v.abs() as i32) };
            clip.text(polar(pivot, rad + 17.0, a), Align2::CENTER_CENTER, txt, FontId::proportional(9.0), if v > 0.0 { RED } else { INK });
        }
    }
    clip.text(Pos2::new(r.center().x, r.bottom() - 30.0), Align2::CENTER_CENTER, "VU", FontId::proportional(17.0), INK);
    clip.text(Pos2::new(r.left() + 9.0, r.bottom() - 12.0), Align2::LEFT_CENTER, label, FontId::proportional(8.0), INK.gamma_multiply(0.8));
    clip.text(Pos2::new(r.right() - 9.0, r.bottom() - 12.0), Align2::RIGHT_CENTER, "0 VU = -12 dBFS", FontId::proportional(7.0), INK.gamma_multiply(0.6));
    // needle with its shadow on the paper
    let a = a_of(pos);
    let tip = polar(pivot, rad + 10.0, a);
    let base = polar(pivot, rad * 0.42, a);
    clip.line_segment([base + Vec2::new(3.0, 4.0), tip + Vec2::new(3.0, 4.0)], Stroke::new(1.6_f32, Color32::from_black_alpha(45)));
    clip.line_segment([base, tip], Stroke::new(1.3_f32, Color32::from_rgb(0x1a, 0x0c, 0x08)));
    // pivot cover
    let cover = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 7.0), r.right_bottom());
    p.rect_filled(cover, CornerRadius::ZERO, Color32::from_rgb(0x18, 0x14, 0x14));
    p.hline(cover.x_range(), cover.top(), Stroke::new(1.0_f32, Color32::from_white_alpha(25)));
    // glass: curved reflection + inner shadow
    let mut g = Mesh::default();
    g.colored_vertex(r.left_top(), Color32::from_white_alpha(52));
    g.colored_vertex(r.right_top(), Color32::from_white_alpha(30));
    g.colored_vertex(Pos2::new(r.right(), r.top() + r.height() * 0.28), Color32::from_white_alpha(0));
    g.colored_vertex(Pos2::new(r.left(), r.top() + r.height() * 0.50), Color32::from_white_alpha(0));
    g.add_triangle(0, 1, 2);
    g.add_triangle(0, 2, 3);
    p.add(Shape::mesh(g));
    p.rect_stroke(r, CornerRadius::same(4), Stroke::new(3.0_f32, Color32::from_black_alpha(90)), StrokeKind::Inside);
}

/// Amber CRT with the tone curve over the live output spectrum.
fn eq_screen(ui: &Ui, r: Rect, fs: f32, dp: &dsp::Params, lit: bool, spec: &Spectrum) {
    let p = ui.painter();
    let bz = r.expand(6.0);
    p.rect_filled(bz.translate(Vec2::new(1.5, 3.0)), CornerRadius::same(9), Color32::from_black_alpha(150));
    p.rect_filled(bz, CornerRadius::same(9), Color32::from_rgb(0x15, 0x12, 0x14));
    p.rect_stroke(bz, CornerRadius::same(9), Stroke::new(1.0_f32, Color32::from_white_alpha(30)), StrokeKind::Inside);
    p.rect_filled(r, CornerRadius::same(10), Color32::from_rgb(0x14, 0x0c, 0x04));
    let g = r.shrink2(Vec2::new(30.0, 14.0)).translate(Vec2::new(8.0, -2.0));
    let clip = p.with_clip_rect(r.shrink(2.0));
    let (f_lo, f_hi) = (20.0f32, 20000.0f32);
    let fx = |f: f32| g.left() + g.width() * (f / f_lo).ln() / (f_hi / f_lo).ln();
    let dy = |d: f32| g.center().y - g.height() * 0.5 * (d / 12.0).clamp(-1.1, 1.1);
    // live output spectrum (phosphor afterglow)
    if lit {
        let sy = |d: f32| g.bottom() - g.height() * ((d + 90.0) / 90.0).clamp(-0.05, 1.05);
        let pts = spec.points(fs, f_lo, f_hi.min(fs * 0.49), 160, 4.5, fx, sy);
        nyashk_common::analyzer::draw_spectrum(&clip, g, &pts, Color32::from_rgb(0x3a, 0x22, 0x08), Color32::from_rgb(0x7a, 0x4c, 0x14));
    }
    let grid = AMBER_DIM.gamma_multiply(0.8);
    for d in [-12.0f32, -6.0, 0.0, 6.0, 12.0] {
        clip.hline(g.left()..=g.right(), dy(d), Stroke::new(if d == 0.0 { 1.0_f32 } else { 0.6 }, grid));
        clip.text(Pos2::new(g.left() - 6.0, dy(d)), Align2::RIGHT_CENTER, format!("{:+}", d as i32), FontId::monospace(8.0), AMBER_DIM.gamma_multiply(1.8));
    }
    for (f, t) in [(50.0f32, "50"), (100.0, "100"), (250.0, "250"), (1000.0, "1k"), (3200.0, "3k2"), (12000.0, "12k")] {
        clip.vline(fx(f), g.top()..=g.bottom(), Stroke::new(0.6_f32, grid));
        clip.text(Pos2::new(fx(f), g.bottom() + 9.0), Align2::CENTER_CENTER, t, FontId::monospace(8.0), AMBER_DIM.gamma_multiply(1.8));
    }
    let coefs = dsp::eq_coefs(fs, dp);
    let pts: Vec<Pos2> = (0..=180)
        .map(|i| {
            let f = f_lo * (f_hi / f_lo).powf(i as f32 / 180.0);
            let d: f32 = if lit { coefs.iter().map(|c| c.response_db(fs, f)).sum() } else { 0.0 };
            Pos2::new(fx(f), dy(d))
        })
        .collect();
    let col = if lit { AMBER } else { AMBER_DIM };
    clip.add(Shape::line(pts.clone(), PathStroke::new(6.0_f32, col.gamma_multiply(0.12))));
    clip.add(Shape::line(pts.clone(), PathStroke::new(3.0_f32, col.gamma_multiply(0.3))));
    clip.add(Shape::line(pts, PathStroke::new(1.5_f32, col)));
    let mut y = r.top() + 1.0;
    while y < r.bottom() {
        clip.hline(r.x_range(), y, Stroke::new(1.0_f32, Color32::from_black_alpha(34)));
        y += 3.0;
    }
    clip.text(Pos2::new(r.right() - 12.0, r.top() + 11.0), Align2::RIGHT_CENTER, "TONE  ·  SPECTRUM", FontId::monospace(8.0), col.gamma_multiply(0.75));
    // curved glass: vignette + reflection
    for k in 0..6 {
        p.rect_stroke(r.shrink(k as f32 * 2.0), CornerRadius::same(10), Stroke::new(2.0_f32, Color32::from_black_alpha(70 - 10 * k as u8)), StrokeKind::Inside);
    }
    // soft reflection band across the top of the glass
    let band = Rect::from_min_max(r.left_top() + Vec2::new(6.0, 4.0), Pos2::new(r.right() - 6.0, r.top() + 34.0));
    let mut gl = Mesh::default();
    gl.colored_vertex(band.left_top(), Color32::from_white_alpha(14));
    gl.colored_vertex(band.right_top(), Color32::from_white_alpha(6));
    gl.colored_vertex(band.right_bottom(), Color32::from_white_alpha(0));
    gl.colored_vertex(band.left_bottom(), Color32::from_white_alpha(0));
    gl.add_triangle(0, 1, 2);
    gl.add_triangle(0, 2, 3);
    p.add(Shape::mesh(gl));
}

fn rot(c: Pos2, v: Vec2, a: f32) -> Pos2 {
    let (s, co) = a.sin_cos();
    Pos2::new(c.x + v.x * co - v.y * s, c.y + v.x * s + v.y * co)
}

fn quad(c: Pos2, w: f32, h: f32, a: f32) -> [Pos2; 4] {
    [rot(c, Vec2::new(-w / 2.0, -h / 2.0), a), rot(c, Vec2::new(w / 2.0, -h / 2.0), a), rot(c, Vec2::new(w / 2.0, h / 2.0), a), rot(c, Vec2::new(-w / 2.0, h / 2.0), a)]
}

fn fill_quad(p: &Painter, q: [Pos2; 4], col: Color32) {
    p.add(Shape::convex_polygon(q.to_vec(), col, Stroke::NONE));
}

/// The picture: an old instant photo taped onto the panel.
fn polaroid(ui: &mut Ui, c: Pos2, tex: &TextureHandle) {
    let a = -0.045f32;
    let (fw, fh) = (232.0f32, 268.0f32);
    let p = ui.painter();
    for (k, al) in [(12.0f32, 22u8), (7.0, 40), (3.0, 70)] {
        fill_quad(p, quad(c + Vec2::new(5.0, 8.0), fw + k, fh + k, a), Color32::from_black_alpha(al));
    }
    // paper: slightly yellowed, darker to the bottom right
    let frame = quad(c, fw, fh, a);
    let mut m = Mesh::default();
    for (q, col) in frame.iter().zip([Color32::from_rgb(0xf2, 0xed, 0xe0), Color32::from_rgb(0xec, 0xe5, 0xd4), Color32::from_rgb(0xd8, 0xcf, 0xbc), Color32::from_rgb(0xe6, 0xdf, 0xcd)]) {
        m.colored_vertex(*q, col);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
    // photo (cover crop to a square, keep the top)
    let ph = 208.0f32;
    let pc = rot(c, Vec2::new(0.0, -fh / 2.0 + 12.0 + ph / 2.0), a);
    let q = quad(pc, ph, ph, a);
    let [tw, th] = tex.size();
    let (tw, th) = (tw as f32, th as f32);
    let (u0, u1, v0, v1) = if tw > th {
        let mm = (1.0 - th / tw) / 2.0;
        (mm, 1.0 - mm, 0.0, 1.0)
    } else {
        let mm = (1.0 - tw / th) / 2.0;
        (0.0, 1.0, mm * 0.6, 1.0 - mm * 1.4)
    };
    fill_quad(p, quad(pc, ph + 2.0, ph + 2.0, a), Color32::from_rgb(0x20, 0x1c, 0x1e));
    let mut mesh = Mesh::with_texture(tex.id());
    // a touch of warm fade, like an old print
    let tint = [Color32::from_rgb(0xf6, 0xee, 0xe2), Color32::from_rgb(0xf2, 0xea, 0xdc), Color32::from_rgb(0xe8, 0xdc, 0xca), Color32::from_rgb(0xf0, 0xe6, 0xd6)];
    let uvs = [Pos2::new(u0, v0), Pos2::new(u1, v0), Pos2::new(u1, v1), Pos2::new(u0, v1)];
    for i in 0..4 {
        mesh.vertices.push(Vertex { pos: q[i], uv: uvs[i], color: tint[i] });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
    let mut g = Mesh::default();
    g.colored_vertex(q[0], Color32::from_white_alpha(30));
    g.colored_vertex(q[1], Color32::from_white_alpha(0));
    g.colored_vertex(q[2], Color32::from_black_alpha(26));
    g.colored_vertex(q[3], Color32::from_white_alpha(6));
    g.add_triangle(0, 1, 2);
    g.add_triangle(0, 2, 3);
    p.add(Shape::mesh(g));
    // caption, hand-written look
    let cap_pos = rot(c, Vec2::new(0.0, fh / 2.0 - 25.0), a);
    let galley = ui.fonts(|f| f.layout_no_wrap("DauBiko".into(), FontId::proportional(17.0), Color32::from_rgb(0x3e, 0x26, 0x5c)));
    let half = galley.size() / 2.0;
    let start = cap_pos - rot(Pos2::ZERO, half, a).to_vec2();
    p.add(TextShape::new(start, galley, Color32::from_rgb(0x3e, 0x26, 0x5c)).with_angle(a));
    // yellowed tape strips
    for (dx, aa) in [(-fw / 2.0 + 18.0, 0.65f32), (fw / 2.0 - 18.0, -0.65)] {
        let tc = rot(c, Vec2::new(dx, -fh / 2.0 + 4.0), a);
        fill_quad(p, quad(tc + Vec2::new(1.0, 1.5), 50.0, 16.0, a + aa), Color32::from_black_alpha(25));
        fill_quad(p, quad(tc, 50.0, 16.0, a + aa), Color32::from_rgba_unmultiplied(0xe2, 0xd2, 0xa0, 150));
    }
}

fn arrow(p: &Painter, c: Pos2, dir: f32, col: Color32) {
    let pts = vec![c + Vec2::new(5.0 * dir, 0.0), c + Vec2::new(-4.0 * dir, -5.0), c + Vec2::new(-4.0 * dir, 5.0)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
}

/// Centre-zero LED bar: negative = violet (left), positive = amber (right). `range` dB per side.
fn led_bar_center(p: &Painter, r: Rect, db: f32, range: f32) {
    let n = 12;
    let seg_w = r.width() / (2 * n + 1) as f32;
    p.rect_filled(r.expand(3.0), CornerRadius::same(3), Color32::from_rgb(0x0c, 0x0a, 0x0c));
    for i in 0..(2 * n + 1) {
        let k = i as i32 - n as i32;
        let x = r.left() + seg_w * i as f32;
        let sr = Rect::from_min_size(Pos2::new(x + 1.0, r.top()), Vec2::new(seg_w - 2.0, r.height()));
        let thr = k as f32 * range / n as f32;
        let (col, lit) = if k == 0 {
            (PRINT, true)
        } else if k < 0 {
            (VIOLET, db <= thr + range / n as f32 * 0.5 && db < -0.2)
        } else {
            (AMBER, db >= thr - range / n as f32 * 0.5 && db > 0.2)
        };
        if lit {
            p.rect_filled(sr.expand(2.5), CornerRadius::same(2), col.gamma_multiply(0.12));
            p.rect_filled(sr, CornerRadius::same(1), col);
            p.hline(sr.left()..=sr.right(), sr.top() + 1.0, Stroke::new(1.0_f32, Color32::from_white_alpha(90)));
        } else {
            p.rect_filled(sr, CornerRadius::same(1), col.gamma_multiply(0.13).to_opaque());
        }
    }
}

/// Limiter gain reduction, 10 segments (1 dB each), red.
fn led_bar_gr(p: &Painter, r: Rect, gr: f32) {
    let n = 10;
    let w = r.width() / n as f32;
    p.rect_filled(r.expand(2.5), CornerRadius::same(2), Color32::from_rgb(0x0c, 0x0a, 0x0c));
    for i in 0..n {
        let sr = Rect::from_min_size(Pos2::new(r.left() + w * i as f32 + 0.5, r.top()), Vec2::new(w - 1.0, r.height()));
        let lit = gr >= i as f32 + 0.3;
        p.rect_filled(sr, CornerRadius::same(1), if lit { RED } else { Color32::from_rgb(0x2c, 0x12, 0x10) });
    }
    print(p, Pos2::new(r.center().x, r.top() - 9.0), Align2::CENTER_CENTER, "LIMIT  GR", 7.5, PRINT_DIM);
}

/// Bottom-right grip that resizes the window keeping its proportions.
fn resize_grip(ui: &mut Ui, ctx: &egui::Context, size_state: &nih_plug_egui::EguiState, full: Rect) {
    let grip = Rect::from_min_size(full.max - Vec2::splat(22.0), Vec2::splat(22.0));
    let r = ui.interact(grip, egui::Id::new("db_resize_grip"), Sense::drag()).on_hover_text("Drag to resize");
    if r.dragged() {
        if let Some(pos) = r.interact_pointer_pos() {
            let ppp = if cfg!(target_os = "macos") { size_state.size().0 as f32 / BASE_W } else { ctx.pixels_per_point() };
            let w = ((pos.x - full.min.x + 11.0) * ppp).clamp(700.0, 3000.0);
            size_state.set_requested_size((w.round() as u32, (w * BASE_H / BASE_W).round() as u32));
        }
    }
    let gc = if r.hovered() || r.dragged() { PRINT } else { PRINT_DIM };
    for k in [6.0f32, 11.0, 16.0] {
        ui.painter().line_segment([Pos2::new(full.max.x - 3.0, full.max.y - k), Pos2::new(full.max.x - k, full.max.y - 3.0)], Stroke::new(1.0_f32, gc));
    }
}
