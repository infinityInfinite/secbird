//! Lighting rendered on the PC: layers of effects, drawn as frames and streamed to the device
//! while the tray runs (Synapse's Chroma Studio). Any device can run any effect this way,
//! including the ones its firmware can't (the Basilisk V3 runs only static, spectrum and wave
//! by itself). With the tray closed the device keeps its zone settings
//! ([`crate::profile::Zone`]), which the tray puts back on exit.
//!
//! [`render`] is pure: a scene, the device's lights with their places, a time and the inputs
//! (key presses, sound level, screen colours) give one colour per light.

use crate::commands as cmd;
use crate::db::Lighting;
use crate::device::Device;
use crate::error::{Error, Result};
use crate::features::Rgb;
use serde::{Deserialize, Serialize};

/// A profile's PC-rendered lighting: layers drawn bottom first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    #[serde(default)]
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub fx: Fx,
    #[serde(default)]
    pub target: Target,
    #[serde(default)]
    pub blend: Blend,
    /// 0-100 %.
    #[serde(default = "full")]
    pub opacity: u8,
}

fn yes() -> bool {
    true
}
fn full() -> u8 {
    100
}
fn mid() -> u8 {
    5
}

impl Layer {
    pub fn new(fx: Fx) -> Self {
        Layer { name: String::new(), enabled: true, fx, target: Target::All, blend: Blend::Normal, opacity: 100 }
    }
}

/// Which lights a layer covers.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "list", rename_all = "kebab-case")]
pub enum Target {
    #[default]
    All,
    /// Lighting zones by name (`logo`, `underglow`; `backlight` is every key of a keyboard).
    Zones(Vec<String>),
    /// Cells of the device's per-key grid, row-major.
    Keys(Vec<u16>),
}

/// How a layer goes over the ones below it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blend {
    /// Covers what's below (black parts of the layer cover too).
    #[default]
    Normal,
    /// Lit parts cover, dark parts let the layers below show: for reactive, ripple, starlight.
    Lit,
    Add,
    Multiply,
    /// The brighter of the two.
    Max,
    /// The layer shows only where the layers below are lit, at their brightness.
    Mask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Static,
    Gradient,
    Breathing,
    Spectrum,
    Wave,
    Wheel,
    Blink,
    Reactive,
    Ripple,
    Starlight,
    Fire,
    Rain,
    Audio,
    Screen,
    Battery,
}

impl Kind {
    pub const ALL: [Kind; 15] = [
        Kind::Static,
        Kind::Gradient,
        Kind::Breathing,
        Kind::Spectrum,
        Kind::Wave,
        Kind::Wheel,
        Kind::Blink,
        Kind::Reactive,
        Kind::Ripple,
        Kind::Starlight,
        Kind::Fire,
        Kind::Rain,
        Kind::Audio,
        Kind::Screen,
        Kind::Battery,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Static => "static",
            Kind::Gradient => "gradient",
            Kind::Breathing => "breathing",
            Kind::Spectrum => "spectrum",
            Kind::Wave => "wave",
            Kind::Wheel => "wheel",
            Kind::Blink => "blink",
            Kind::Reactive => "reactive",
            Kind::Ripple => "ripple",
            Kind::Starlight => "starlight",
            Kind::Fire => "fire",
            Kind::Rain => "rain",
            Kind::Audio => "audio",
            Kind::Screen => "screen",
            Kind::Battery => "battery",
        }
    }

    /// Changes over time (or with input), so it needs frames streamed.
    pub fn animated(self) -> bool {
        !matches!(self, Kind::Static | Kind::Gradient)
    }
}

/// Audio: how the sound level shows. Screen: which part of the screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// Audio: a bar that fills along the angle. Screen: each light takes the part of the
    /// screen it sits under. Battery: a bar.
    #[default]
    Meter,
    /// Audio: everything brightens with the level. Screen: the whole screen's average.
    /// Battery: one colour, green to red.
    Pulse,
    /// Audio: left lights follow the left channel, right lights the right one.
    Stereo,
}

/// One effect and its settings. Settings that an effect doesn't use are ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fx {
    pub kind: Kind,
    /// Colours or gradient stops; empty means rainbow (or random for breathing, starlight,
    /// reactive, ripple, rain).
    #[serde(default)]
    pub colors: Vec<Rgb>,
    /// 1 (slow) to 10 (fast).
    #[serde(default = "mid")]
    pub speed: u8,
    /// Direction in degrees: 0 left to right, 90 top to bottom.
    #[serde(default)]
    pub angle: u16,
    /// 1-10: wave length, ripple and reactive reach, fire height.
    #[serde(default = "mid")]
    pub size: u8,
    /// 1-10: how many stars, drops or sparks.
    #[serde(default = "mid")]
    pub density: u8,
    /// 1-10: how long a press, star or drop takes to fade.
    #[serde(default = "mid")]
    pub fade: u8,
    #[serde(default)]
    pub mode: Mode,
    /// Wave and wheel the other way.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reverse: bool,
}

impl Fx {
    pub fn new(kind: Kind) -> Self {
        Fx { kind, colors: Vec::new(), speed: 5, angle: 0, size: 5, density: 5, fade: 5, mode: Mode::Meter, reverse: false }
    }

    pub fn with_colors(mut self, colors: &[Rgb]) -> Self {
        self.colors = colors.to_vec();
        self
    }

    fn check(&self) -> Result<()> {
        let range = |v: u8, what: &str| {
            if (1..=10).contains(&v) { Ok(()) } else { Err(Error::InvalidArg(format!("{what} is 1-10"))) }
        };
        range(self.speed, "speed")?;
        range(self.size, "size")?;
        range(self.density, "density")?;
        range(self.fade, "fade")?;
        if self.angle >= 360 {
            return Err(Error::InvalidArg("angle is 0-359".into()));
        }
        if self.colors.len() > 16 {
            return Err(Error::InvalidArg("at most 16 colours".into()));
        }
        Ok(())
    }
}

impl Scene {
    pub fn check(&self) -> Result<()> {
        if self.layers.len() > 32 {
            return Err(Error::InvalidArg("at most 32 layers".into()));
        }
        for l in &self.layers {
            l.fx.check()?;
            if l.opacity > 100 {
                return Err(Error::InvalidArg("opacity is 0-100".into()));
            }
        }
        Ok(())
    }

    fn active(&self) -> impl Iterator<Item = &Layer> {
        self.layers.iter().filter(|l| l.enabled && l.opacity > 0)
    }

    /// Something to draw at all.
    pub fn is_on(&self) -> bool {
        self.active().next().is_some()
    }

    pub fn animated(&self) -> bool {
        self.active().any(|l| l.fx.kind.animated())
    }

    pub fn uses(&self, kind: Kind) -> bool {
        self.active().any(|l| l.fx.kind == kind)
    }
}

impl Layer {
    /// The CLI's form: `kind[:RRGGBB,...|random]` then `key=value` options, like
    /// `wave:FF0000,0000FF speed=7 angle=90 blend=lit opacity=60 keys=17,18 zones=logo reverse`.
    pub fn parse(args: &[String]) -> Result<Layer> {
        let first = args.first().ok_or_else(|| Error::InvalidArg("which effect?".into()))?.to_ascii_lowercase();
        let (kind, colors) = first.split_once(':').unwrap_or((&first, ""));
        let kind = Kind::ALL.into_iter().find(|k| k.name() == kind).ok_or_else(|| {
            let names: Vec<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
            Error::InvalidArg(format!("unknown effect '{kind}' ({})", names.join(", ")))
        })?;
        let mut fx = Fx::new(kind);
        if !colors.is_empty() && colors != "random" && colors != "rainbow" {
            fx.colors = colors.split(',').map(str::parse).collect::<Result<_>>()?;
        }
        let mut layer = Layer::new(fx);
        for opt in &args[1..] {
            let (k, v) = opt.split_once('=').unwrap_or((opt, ""));
            let num = |max: u16| -> Result<u16> {
                v.parse::<u16>().ok().filter(|n| *n <= max).ok_or_else(|| Error::InvalidArg(format!("bad {k} '{v}'")))
            };
            let list = || v.split(',').map(str::trim).filter(|s| !s.is_empty());
            match k {
                "speed" => layer.fx.speed = num(10)? as u8,
                "angle" => layer.fx.angle = num(359)?,
                "size" => layer.fx.size = num(10)? as u8,
                "density" => layer.fx.density = num(10)? as u8,
                "fade" => layer.fx.fade = num(10)? as u8,
                "opacity" => layer.opacity = num(100)? as u8,
                "reverse" => layer.fx.reverse = true,
                "name" => layer.name = v.to_string(),
                "mode" => {
                    layer.fx.mode = serde_json::from_value(serde_json::Value::String(v.into()))
                        .map_err(|_| Error::InvalidArg(format!("mode is meter, pulse or stereo, not '{v}'")))?
                }
                "blend" => {
                    layer.blend = serde_json::from_value(serde_json::Value::String(v.into())).map_err(|_| {
                        Error::InvalidArg(format!("blend is normal, lit, add, multiply, max or mask, not '{v}'"))
                    })?
                }
                "zones" => layer.target = Target::Zones(list().map(String::from).collect()),
                "keys" => {
                    layer.target = Target::Keys(
                        list().map(|n| n.parse().map_err(|_| Error::InvalidArg(format!("bad key cell '{n}'")))).collect::<Result<_>>()?,
                    )
                }
                _ => return Err(Error::InvalidArg(format!("unknown option '{opt}'"))),
            }
        }
        layer.fx.check()?;
        Ok(layer)
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fx = &self.fx;
        write!(f, "{}", fx.kind.name())?;
        if !fx.colors.is_empty() {
            write!(f, ":{}", fx.colors.iter().map(Rgb::to_string).collect::<Vec<_>>().join(","))?;
        }
        write!(f, " speed={} angle={} size={} density={} fade={}", fx.speed, fx.angle, fx.size, fx.density, fx.fade)?;
        if matches!(fx.kind, Kind::Audio | Kind::Screen | Kind::Battery) {
            let mode = serde_json::to_value(fx.mode).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            write!(f, " mode={mode}")?;
        }
        if fx.reverse {
            write!(f, " reverse")?;
        }
        if self.blend != Blend::Normal {
            let blend = serde_json::to_value(self.blend).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            write!(f, " blend={blend}")?;
        }
        if self.opacity != 100 {
            write!(f, " opacity={}", self.opacity)?;
        }
        match &self.target {
            Target::All => {}
            Target::Zones(z) => write!(f, " zones={}", z.join(","))?,
            Target::Keys(k) => write!(f, " keys={}", k.iter().map(u16::to_string).collect::<Vec<_>>().join(","))?,
        }
        if !self.name.is_empty() {
            write!(f, " name={}", self.name)?;
        }
        if !self.enabled {
            write!(f, " (off)")?;
        }
        Ok(())
    }
}

/// Where a light is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Out {
    /// Cell of the per-key grid, row-major.
    Cell(u16),
    /// A whole lighting zone (LED id).
    Zone(u8),
}

/// One light: its place on the device (0-1 from the top left) and where it is written.
#[derive(Debug, Clone, PartialEq)]
pub struct Pixel {
    pub x: f32,
    pub y: f32,
    /// The zone it belongs to (`backlight` for the keys of a grid).
    pub zone: String,
    /// Key name (`w`, `space`) when the layout is known.
    pub key: Option<String>,
    pub out: Out,
}

/// A key's place, as the editor draws it: matrix cell and position in key units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyPos {
    pub cell: u16,
    #[serde(default)]
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Canvas {
    pub pixels: Vec<Pixel>,
    /// Grid size when the device takes whole frames.
    pub grid: Option<(u8, u8)>,
}

/// Where a zone sits: spread left to right in the device's order (as the editor draws them,
/// so a wave crosses them in that order), and top to bottom where the name says.
fn zone_place(name: &str, i: usize, n: usize) -> (f32, f32) {
    let x = (i as f32 + 0.5) / n as f32;
    let y = match name {
        "scroll" => 0.15,
        "profile" => 0.35,
        "logo" => 0.85,
        _ => 0.5,
    };
    (x, y)
}

impl Canvas {
    /// The lights of a device: every cell of its grid, else one per lighting zone.
    /// `layout` places the keys (from the editor's drawing); cells it doesn't list sit on
    /// the plain grid.
    pub fn for_device(dev: &Device, layout: &[KeyPos]) -> Canvas {
        let leds: Vec<(String, u8)> =
            dev.spec.iter().flat_map(|s| s.leds.iter().map(|l| (l.name.clone(), l.id))).collect();
        Canvas::build(dev.matrix().map(|m| (m.rows, m.cols)), &leds, layout)
    }

    pub fn build(grid: Option<(u8, u8)>, leds: &[(String, u8)], layout: &[KeyPos]) -> Canvas {
        if let Some((rows, cols)) = grid {
            let (w, h) = layout.iter().fold((0f32, 0f32), |(w, h), k| (w.max(k.x + 1.0), h.max(k.y + 1.0)));
            let pixels = (0..rows as u16 * cols as u16)
                .map(|cell| {
                    let (r, c) = (cell / cols as u16, cell % cols as u16);
                    let placed = layout.iter().find(|k| k.cell == cell);
                    let (x, y) = match placed {
                        Some(k) if w > 0.0 => ((k.x + 0.5) / w, (k.y + 0.5) / h),
                        _ => ((c as f32 + 0.5) / cols as f32, (r as f32 + 0.5) / rows as f32),
                    };
                    Pixel {
                        x,
                        y,
                        zone: "backlight".into(),
                        key: placed.map(|k| k.name.clone()).filter(|n| !n.is_empty()),
                        out: Out::Cell(cell),
                    }
                })
                .collect();
            return Canvas { pixels, grid };
        }
        let pixels = leds
            .iter()
            .enumerate()
            .map(|(i, (name, id))| {
                let (x, y) = zone_place(name, i, leds.len());
                Pixel { x, y, zone: name.clone(), key: None, out: Out::Zone(*id) }
            })
            .collect();
        Canvas { pixels, grid: None }
    }

    fn covers(&self, target: &Target, i: usize) -> bool {
        let p = &self.pixels[i];
        match target {
            Target::All => true,
            Target::Zones(z) => z.contains(&p.zone),
            Target::Keys(k) => matches!(p.out, Out::Cell(c) if k.contains(&c)),
        }
    }
}

/// A key press or click, for reactive and ripple.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Press {
    /// Seconds on the scene clock.
    pub at: f32,
    /// The light pressed, as an index into [`Canvas::pixels`]; `None` for the whole device
    /// (a mouse click, or a key whose place isn't known).
    pub pixel: Option<usize>,
}

/// What effects react to besides time.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub presses: Vec<Press>,
    /// Sound level 0-1 of what the PC plays, overall and per channel.
    pub audio: (f32, f32, f32),
    /// The screen scaled down: width, height, colours row-major.
    pub screen: Option<(usize, usize, Vec<Rgb>)>,
    pub battery: Option<u8>,
}

const WHITE: Rgb = Rgb(255, 255, 255);

#[derive(Debug, Clone, Copy, PartialEq)]
struct C(f32, f32, f32);

impl From<Rgb> for C {
    fn from(c: Rgb) -> C {
        C(c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0)
    }
}

impl C {
    fn to_rgb(self) -> Rgb {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        Rgb(b(self.0), b(self.1), b(self.2))
    }
    fn scale(self, k: f32) -> C {
        C(self.0 * k, self.1 * k, self.2 * k)
    }
    fn mix(self, o: C, k: f32) -> C {
        C(self.0 + (o.0 - self.0) * k, self.1 + (o.1 - self.1) * k, self.2 + (o.2 - self.2) * k)
    }
    fn luma(self) -> f32 {
        self.0.max(self.1).max(self.2)
    }
}

/// Hue 0-1 at full saturation.
fn hue(h: f32) -> C {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => C(1.0, x, 0.0),
        1 => C(x, 1.0, 0.0),
        2 => C(0.0, 1.0, x),
        3 => C(0.0, x, 1.0),
        4 => C(x, 0.0, 1.0),
        _ => C(1.0, 0.0, x),
    }
}

/// Stable pseudo-random 0-1 from a few numbers.
fn rand(a: u64, b: u64) -> f32 {
    let mut z = a.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ b.wrapping_add(0xBF58_476D_1CE4_E5B9).rotate_left(31);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth 1D value noise.
fn noise(x: f32, seed: u64) -> f32 {
    let i = x.floor();
    let f = x - i;
    let a = rand(i as i64 as u64, seed);
    let b = rand((i as i64 + 1) as u64, seed);
    let s = f * f * (3.0 - 2.0 * f);
    a + (b - a) * s
}

impl Fx {
    /// Colour at `p` (0-1) along the colours as a looping gradient, or the rainbow.
    fn palette(&self, p: f32) -> C {
        match self.colors.len() {
            0 => hue(p),
            1 => self.colors[0].into(),
            n => {
                let p = p.rem_euclid(1.0) * n as f32;
                let i = p as usize % n;
                C::from(self.colors[i]).mix(self.colors[(i + 1) % n].into(), p.fract())
            }
        }
    }

    /// Colour along the colours without looping (gradient, meters).
    fn ramp(&self, p: f32) -> C {
        match self.colors.len() {
            0 => hue(p * 0.83),
            1 => self.colors[0].into(),
            n => {
                let p = p.clamp(0.0, 1.0) * (n - 1) as f32;
                let i = (p as usize).min(n - 2);
                C::from(self.colors[i]).mix(self.colors[i + 1].into(), p - i as f32)
            }
        }
    }

    /// The n-th pick: the colours in turn, or a random hue when there are none.
    fn pick(&self, n: u64) -> C {
        match self.colors.len() {
            0 => hue(rand(n, 7)),
            len => self.colors[(n % len as u64) as usize].into(),
        }
    }

    fn first(&self) -> C {
        self.colors.first().copied().unwrap_or(WHITE).into()
    }

    /// Speed as a multiplier: 1 is 0.2x, 5 is 1x, 10 is 2x... roughly doubling every 2.5 steps.
    fn rate(&self) -> f32 {
        2f32.powf((self.speed as f32 - 5.0) / 2.5)
    }

    /// Seconds a press, star or drop lasts.
    fn fade_secs(&self) -> f32 {
        0.15 + self.fade as f32 * 0.25
    }

    /// Position of a light along the direction, 0-1.
    fn along(&self, x: f32, y: f32) -> f32 {
        let a = (self.angle as f32).to_radians();
        let (c, s) = (a.cos(), a.sin());
        // Project onto the direction and rescale so the device's corners span 0-1.
        let min = 0f32.min(c) + 0f32.min(s);
        let max = 0f32.max(c) + 0f32.max(s);
        let v = x * c + y * s;
        if max > min { (v - min) / (max - min) } else { 0.0 }
    }

    fn color_at(&self, canvas: &Canvas, i: usize, t: f32, input: &Inputs) -> C {
        let p = &canvas.pixels[i];
        let dir = if self.reverse { -1.0 } else { 1.0 };
        let rate = self.rate();
        match self.kind {
            Kind::Static => self.first(),
            Kind::Gradient => self.ramp(self.along(p.x, p.y)),
            Kind::Breathing => {
                let phase = t * rate / 4.0;
                let cycle = phase.floor() as u64;
                let k = (std::f32::consts::PI * phase.fract()).sin().powi(2);
                self.pick(cycle).scale(k)
            }
            Kind::Spectrum => self.palette(t * rate / 10.0),
            Kind::Wave => {
                let length = self.size as f32 / 5.0;
                self.palette(self.along(p.x, p.y) / length - dir * t * rate / 4.0)
            }
            Kind::Wheel => {
                let a = (p.y - 0.5).atan2(p.x - 0.5) / std::f32::consts::TAU;
                let turns = 5.0 / self.size as f32;
                self.palette(a * turns.round().max(1.0) - dir * t * rate / 4.0)
            }
            Kind::Blink => {
                let phase = t * rate * 1.5;
                if phase.fract() < 0.5 { self.pick(phase.floor() as u64) } else { C(0.0, 0.0, 0.0) }
            }
            Kind::Reactive => {
                let life = self.fade_secs();
                let reach = self.size as f32 * 0.04;
                let mut out = C(0.0, 0.0, 0.0);
                for (n, press) in input.presses.iter().enumerate() {
                    let age = t - press.at;
                    if !(0.0..life).contains(&age) {
                        continue;
                    }
                    let near = match press.pixel {
                        None => 1.0,
                        Some(j) if j == i => 1.0,
                        Some(j) => {
                            let q = &canvas.pixels[j];
                            let d = ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt();
                            (1.0 - d / reach.max(0.001)).max(0.0)
                        }
                    };
                    let k = near * (1.0 - age / life);
                    let c = self.pick(press_seed(press, n)).scale(k);
                    out = C(out.0.max(c.0), out.1.max(c.1), out.2.max(c.2));
                }
                out
            }
            Kind::Ripple => {
                let life = self.fade_secs() * 1.5;
                let width = 0.06 + self.size as f32 * 0.02;
                let mut out = C(0.0, 0.0, 0.0);
                for (n, press) in input.presses.iter().enumerate() {
                    let age = t - press.at;
                    if !(0.0..life).contains(&age) {
                        continue;
                    }
                    let (ox, oy) = press.pixel.map_or((0.5, 0.5), |j| (canvas.pixels[j].x, canvas.pixels[j].y));
                    let d = ((p.x - ox).powi(2) + (p.y - oy).powi(2)).sqrt();
                    let radius = age * rate * 0.8;
                    // A device with one light (a mouse zone) gets a flash instead of a ring.
                    let ring = if canvas.pixels.len() < 4 { 1.0 } else { (1.0 - (d - radius).abs() / width).max(0.0) };
                    let k = ring * (1.0 - age / life);
                    let c = self.pick(press_seed(press, n)).scale(k);
                    out = C(out.0.max(c.0), out.1.max(c.1), out.2.max(c.2));
                }
                out
            }
            Kind::Starlight => {
                // Each light twinkles in slots of `life` seconds, with a random offset, and
                // in each slot is a star with a chance set by the density.
                let life = self.fade_secs() * 2.0 / rate.sqrt();
                let offset = rand(i as u64, 11) * life;
                let slot = ((t + offset) / life).floor();
                let f = ((t + offset) / life).fract();
                let chance = self.density as f32 / 12.0;
                if rand(i as u64, slot as i64 as u64) < chance {
                    let k = (std::f32::consts::PI * f).sin();
                    self.pick(slot as i64 as u64 ^ (i as u64) << 20).scale(k)
                } else {
                    C(0.0, 0.0, 0.0)
                }
            }
            Kind::Fire => {
                // Heat rises from the bottom; flicker from noise over x and time.
                let height = self.size as f32 / 10.0;
                let flick = noise(p.x * 6.0 + t * rate * 2.0, 3) * 0.6 + noise(t * rate * 5.0 + p.x * 13.0, 5) * 0.4;
                let heat = (p.y - 1.0 + height * 1.2 + flick * 0.5).clamp(0.0, 1.0);
                if self.colors.is_empty() {
                    // Black, red, orange, yellow.
                    C((heat * 3.0).min(1.0), (heat * 3.0 - 1.0).clamp(0.0, 1.0) * 0.8, (heat * 3.0 - 2.0).clamp(0.0, 1.0) * 0.5)
                } else {
                    self.ramp(heat).scale(heat.sqrt())
                }
            }
            Kind::Rain => {
                // Drops fall down columns; each column has its own timing.
                let cols = 12.0 + self.density as f32 * 2.0;
                let col = (p.x * cols).floor() as u64;
                let speed = rate * 0.7;
                let period = 1.0 + rand(col, 21) * 2.0 / (self.density as f32 / 5.0);
                let phase = (t * speed / period + rand(col, 22)).fract() * period;
                let head = phase * 1.4 - 0.2;
                let tail = 0.1 + self.fade as f32 * 0.04;
                let d = head - p.y;
                if (0.0..tail).contains(&d) {
                    let n = ((t * speed / period + rand(col, 22)).floor()) as u64;
                    self.pick(col * 1000 + n).scale(1.0 - d / tail)
                } else {
                    C(0.0, 0.0, 0.0)
                }
            }
            Kind::Audio => {
                let (level, left, right) = input.audio;
                match self.mode {
                    Mode::Meter => {
                        let a = self.along(p.x, p.y);
                        if a <= level { self.ramp_audio(a) } else { C(0.0, 0.0, 0.0) }
                    }
                    Mode::Pulse => self.first_or_ramp(level).scale(level),
                    Mode::Stereo => {
                        let lvl = if p.x < 0.5 { left } else { right };
                        let from_centre = (p.x - 0.5).abs() * 2.0;
                        if canvas.pixels.len() < 4 || from_centre <= lvl { self.ramp_audio(from_centre).scale(if canvas.pixels.len() < 4 { lvl } else { 1.0 }) } else { C(0.0, 0.0, 0.0) }
                    }
                }
            }
            Kind::Screen => {
                let Some((w, h, px)) = &input.screen else { return C(0.0, 0.0, 0.0) };
                if *w == 0 || *h == 0 || px.len() < w * h {
                    return C(0.0, 0.0, 0.0);
                }
                let c = match self.mode {
                    Mode::Pulse => average(px),
                    _ if canvas.pixels.len() < 4 => average(px),
                    _ => {
                        let sx = ((p.x * *w as f32) as usize).min(w - 1);
                        let sy = ((p.y * *h as f32) as usize).min(h - 1);
                        px[sy * w + sx].into()
                    }
                };
                saturate(c, 1.0 + self.size as f32 / 10.0)
            }
            Kind::Battery => {
                let Some(pct) = input.battery else { return C(0.0, 0.0, 0.0) };
                let c: C = crate::effects::Effect::battery_color(pct).into();
                match self.mode {
                    Mode::Meter if canvas.pixels.len() >= 4 => {
                        if self.along(p.x, p.y) <= pct as f32 / 100.0 { c } else { C(0.0, 0.0, 0.0) }
                    }
                    _ => c,
                }
            }
        }
    }

    /// Meter colours: the colours from the start to the end, or green to red.
    fn ramp_audio(&self, a: f32) -> C {
        if self.colors.is_empty() {
            C(a.min(0.5) * 2.0, (1.0 - a).min(0.5) * 2.0, 0.0)
        } else {
            self.ramp(a)
        }
    }

    fn first_or_ramp(&self, a: f32) -> C {
        if self.colors.len() > 1 { self.ramp(a) } else { self.first() }
    }
}

fn press_seed(press: &Press, n: usize) -> u64 {
    (press.at * 1000.0) as u64 ^ (n as u64) << 32
}

fn average(px: &[Rgb]) -> C {
    let n = px.len().max(1) as f32;
    let (r, g, b) = px.iter().fold((0f32, 0f32, 0f32), |(r, g, b), c| (r + c.0 as f32, g + c.1 as f32, b + c.2 as f32));
    C(r / n / 255.0, g / n / 255.0, b / n / 255.0)
}

/// Push a colour away from grey: screens are mostly greyish, LEDs look washed out with it.
fn saturate(c: C, k: f32) -> C {
    let grey = (c.0 + c.1 + c.2) / 3.0;
    C(grey + (c.0 - grey) * k, grey + (c.1 - grey) * k, grey + (c.2 - grey) * k)
}

fn blend(below: C, top: C, mode: Blend, opacity: f32) -> C {
    let mixed = match mode {
        Blend::Normal => top,
        Blend::Lit => return below.mix(top, opacity * top.luma()),
        Blend::Add => C(below.0 + top.0, below.1 + top.1, below.2 + top.2),
        Blend::Multiply => C(below.0 * top.0, below.1 * top.1, below.2 * top.2),
        Blend::Max => C(below.0.max(top.0), below.1.max(top.1), below.2.max(top.2)),
        Blend::Mask => top.scale(below.luma()),
    };
    below.mix(mixed, opacity)
}

/// One colour per light of `canvas` at time `t` (seconds).
pub fn render(scene: &Scene, canvas: &Canvas, t: f32, input: &Inputs) -> Vec<Rgb> {
    let mut out = vec![C(0.0, 0.0, 0.0); canvas.pixels.len()];
    for layer in scene.active() {
        let opacity = layer.opacity as f32 / 100.0;
        for (i, px) in out.iter_mut().enumerate() {
            if canvas.covers(&layer.target, i) {
                let c = layer.fx.color_at(canvas, i, t, input);
                *px = blend(*px, c, layer.blend, opacity);
            }
        }
    }
    out.into_iter().map(C::to_rgb).collect()
}

impl Device {
    /// [`Device::send_only`] to the device or, for a dock's own light, to the dock.
    fn send_only_led(&self, led: u8, command: crate::proto::Command, size: u8, args: &[u8]) -> Result<()> {
        if self.is_dock_led(led) {
            self.send_only_as(crate::device::DOCK_TXID, command, size, args)
        } else {
            self.send_only(command, size, args)
        }
    }

    /// Get the device ready for streamed frames: the custom-frame effect on grids, the static
    /// effect on older single lights. Waits for the replies, so a device that doesn't take
    /// frames says so once instead of on every frame.
    pub fn start_stream(&self, canvas: &Canvas, first: &[Rgb]) -> Result<()> {
        match canvas.grid {
            Some(_) => self.show_frame(first),
            None => {
                for (p, c) in canvas.pixels.iter().zip(first) {
                    if let Out::Zone(led) = p.out {
                        self.set_live_color(led, *c)?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Send one frame without waiting for replies. `before` is the last frame sent: only
    /// rows or zones that changed are written, which matters for wireless devices.
    pub fn stream(&self, canvas: &Canvas, frame: &[Rgb], before: Option<&[Rgb]>) -> Result<()> {
        if let Some((rows, cols)) = canvas.grid {
            let cols = cols as usize;
            for row in 0..rows as usize {
                let colors = &frame[row * cols..(row + 1) * cols];
                if before.is_some_and(|b| b.len() == frame.len() && &b[row * cols..(row + 1) * cols] == colors) {
                    continue;
                }
                self.stream_row(row as u8, colors)?;
            }
            return Ok(());
        }
        for (i, (p, c)) in canvas.pixels.iter().zip(frame).enumerate() {
            if before.is_some_and(|b| b.get(i) == Some(c)) {
                continue;
            }
            if let Out::Zone(led) = p.out {
                self.stream_zone(led, *c)?;
            }
        }
        Ok(())
    }

    fn stream_row(&self, row: u8, colors: &[Rgb]) -> Result<()> {
        for (i, chunk) in colors.chunks(cmd::MATRIX_FRAME_MAX_KEYS).enumerate() {
            let first = (i * cmd::MATRIX_FRAME_MAX_KEYS) as u8;
            let last = first + chunk.len() as u8 - 1;
            let rgb = chunk.iter().flat_map(|c| [c.0, c.1, c.2]);
            match self.lighting() {
                Lighting::Extended => {
                    let mut args = vec![0x00, 0x00, row, first, last];
                    args.extend(rgb);
                    self.send_only(cmd::MATRIX_FRAME, args.len() as u8, &args)?;
                }
                Lighting::Standard => {
                    let mut args = vec![0xFF, row, first, last];
                    args.extend(rgb);
                    self.send_only(cmd::GRID_FRAME, cmd::GRID_FRAME_SIZE, &args)?;
                }
                _ => return Err(Error::Unsupported(format!("streamed lighting on {}", self.name()))),
            }
        }
        Ok(())
    }

    /// A zone's colour, shown now and not saved, without waiting for the reply.
    fn stream_zone(&self, led: u8, Rgb(r, g, b): Rgb) -> Result<()> {
        match self.lighting_for(led) {
            Lighting::Extended => {
                let args = [cmd::NOSTORE, led, cmd::EFFECT_STATIC, 0x00, 0x00, 0x01, r, g, b];
                self.send_only_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args)
            }
            Lighting::Standard => self.send_only(cmd::LED_RGB, 5, &[cmd::VARSTORE, led, r, g, b]),
            Lighting::MouseExtended => {
                self.send_only(cmd::MOUSE_LED_EFFECT, 6, &[cmd::VARSTORE, led, cmd::GRID_STATIC, r, g, b])
            }
            Lighting::Kraken => Err(Error::Unsupported(format!("streamed lighting on {}", self.name()))),
        }
    }

    /// After streaming: show what the device has saved again (its effect for the slot in
    /// use, read back with `0F:82`), so it doesn't stay on the last frame when the profile
    /// has no zone settings of its own to send. Devices with the older lighting commands
    /// saved every write anyway. Returns how many zones were put back.
    pub fn show_saved_lighting(&self) -> usize {
        let leds: Vec<u8> = self.spec.iter().flat_map(|s| s.leds.iter().map(|l| l.id)).collect();
        leds.into_iter()
            .filter(|&led| self.lighting_for(led) == Lighting::Extended)
            .filter(|&led| {
                let store = self.store_for(led);
                let Ok(r) = self.transact_led(led, cmd::MATRIX_EFFECT.get(), 0x0C, &[store, led]) else { return false };
                let a = r.args();
                let n = (6 + 3 * a[5] as usize).min(a.len());
                let mut args = a[..n].to_vec();
                args[0] = store;
                args[1] = led;
                self.transact_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args).is_ok()
            })
            .count()
    }

    /// A zone's colour shown now and not saved where the device has that (the newer lighting
    /// commands); the older ones only have the saved write.
    pub fn set_live_color(&self, led: u8, c: Rgb) -> Result<()> {
        match self.lighting_for(led) {
            Lighting::Extended => {
                let args = [cmd::NOSTORE, led, cmd::EFFECT_STATIC, 0x00, 0x00, 0x01, c.0, c.1, c.2];
                self.transact_led(led, cmd::MATRIX_EFFECT, args.len() as u8, &args)?;
                Ok(())
            }
            _ => self.set_static_color(led, c),
        }
    }
}

/// How fast the tray streams: wired devices every tick, wireless ones less often so they can
/// still sleep and their batteries last.
pub const FPS: u32 = 30;
pub const WIRELESS_EVERY: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(n: usize) -> Canvas {
        Canvas {
            pixels: (0..n)
                .map(|i| Pixel {
                    x: (i as f32 + 0.5) / n as f32,
                    y: 0.5,
                    zone: "backlight".into(),
                    key: None,
                    out: Out::Cell(i as u16),
                })
                .collect(),
            grid: Some((1, n as u8)),
        }
    }

    fn scene(fx: Fx) -> Scene {
        Scene { layers: vec![Layer::new(fx)] }
    }

    const RED: Rgb = Rgb(255, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);

    #[test]
    fn static_and_gradient() {
        let c = strip(5);
        let f = render(&scene(Fx::new(Kind::Static).with_colors(&[RED])), &c, 0.0, &Inputs::default());
        assert!(f.iter().all(|x| *x == RED));
        let g = render(&scene(Fx::new(Kind::Gradient).with_colors(&[RED, BLUE])), &c, 0.0, &Inputs::default());
        assert!(g[0].0 > 200 && g[0].2 < 50, "{:?}", g[0]);
        assert!(g[4].2 > 200 && g[4].0 < 50, "{:?}", g[4]);
        let mut fx = Fx::new(Kind::Gradient).with_colors(&[RED, BLUE]);
        fx.angle = 180;
        let back = render(&scene(fx), &c, 0.0, &Inputs::default());
        assert_eq!(back[0], g[4]);
    }

    #[test]
    fn breathing_fades_out_and_in() {
        let c = strip(1);
        let s = scene(Fx::new(Kind::Breathing).with_colors(&[RED]));
        let dark = render(&s, &c, 0.0, &Inputs::default());
        let lit = render(&s, &c, 2.0, &Inputs::default());
        assert_eq!(dark[0], Rgb(0, 0, 0));
        assert!(lit[0].0 > 250);
    }

    #[test]
    fn wave_moves_and_reverses() {
        let c = strip(8);
        let fx = Fx::new(Kind::Wave);
        let a = render(&scene(fx.clone()), &c, 0.0, &Inputs::default());
        let b = render(&scene(fx.clone()), &c, 0.5, &Inputs::default());
        assert_ne!(a, b);
        let mut rev = fx;
        rev.reverse = true;
        assert_ne!(render(&scene(rev), &c, 0.5, &Inputs::default()), b);
    }

    #[test]
    fn reactive_lights_the_pressed_key_and_fades() {
        let c = strip(10);
        let mut fx = Fx::new(Kind::Reactive).with_colors(&[RED]);
        fx.size = 1;
        let s = scene(fx);
        let input = Inputs { presses: vec![Press { at: 1.0, pixel: Some(3) }], ..Default::default() };
        let now = render(&s, &c, 1.0, &input);
        assert_eq!(now[3], RED);
        assert_eq!(now[8], Rgb(0, 0, 0));
        let later = render(&s, &c, 1.5, &input);
        assert!(later[3].0 < 255 && later[3].0 > 0);
        assert_eq!(render(&s, &c, 10.0, &input)[3], Rgb(0, 0, 0));
        // A click lights everything.
        let click = Inputs { presses: vec![Press { at: 1.0, pixel: None }], ..Default::default() };
        assert!(render(&s, &c, 1.0, &click).iter().all(|x| *x == RED));
    }

    #[test]
    fn ripple_ring_moves_out() {
        let c = strip(20);
        let s = scene(Fx::new(Kind::Ripple).with_colors(&[BLUE]));
        let input = Inputs { presses: vec![Press { at: 0.0, pixel: Some(0) }], ..Default::default() };
        let early = render(&s, &c, 0.05, &input);
        let late = render(&s, &c, 0.6, &input);
        let peak = |f: &[Rgb]| f.iter().enumerate().max_by_key(|(_, x)| x.2).unwrap().0;
        assert!(peak(&late) > peak(&early));
    }

    #[test]
    fn starlight_twinkles_some_lights() {
        let c = strip(60);
        let s = scene(Fx::new(Kind::Starlight));
        let lit = (0..10).map(|n| render(&s, &c, n as f32 * 0.3, &Inputs::default()).iter().filter(|x| x.0 as u32 + x.1 as u32 + x.2 as u32 > 30).count()).sum::<usize>();
        assert!(lit > 20 && lit < 550, "{lit}");
    }

    #[test]
    fn fire_is_warm_and_moves() {
        let c = Canvas::build(Some((6, 22)), &[], &[]);
        let s = scene(Fx::new(Kind::Fire));
        let a = render(&s, &c, 0.0, &Inputs::default());
        assert!(a.iter().all(|x| x.2 <= x.0), "fire has no blue over red");
        assert_ne!(a, render(&s, &c, 0.3, &Inputs::default()));
        let bottom: u32 = a[5 * 22..].iter().map(|x| x.0 as u32).sum();
        let top: u32 = a[..22].iter().map(|x| x.0 as u32).sum();
        assert!(bottom > top);
    }

    #[test]
    fn audio_meter_follows_level() {
        let c = strip(10);
        let s = scene(Fx::new(Kind::Audio));
        let lit = |l: f32| {
            render(&s, &c, 0.0, &Inputs { audio: (l, l, l), ..Default::default() }).iter().filter(|x| **x != Rgb(0, 0, 0)).count()
        };
        assert_eq!(lit(0.0), 0);
        assert_eq!(lit(0.5), 5);
        assert_eq!(lit(1.0), 10);
        let mut pulse = Fx::new(Kind::Audio).with_colors(&[RED]);
        pulse.mode = Mode::Pulse;
        let half = render(&scene(pulse), &c, 0.0, &Inputs { audio: (0.5, 0.5, 0.5), ..Default::default() });
        assert!((120..=135).contains(&half[0].0));
    }

    #[test]
    fn screen_mirror_and_average() {
        let c = strip(2);
        let screen = Some((2, 1, vec![RED, BLUE]));
        let input = Inputs { screen, ..Default::default() };
        let mut fx = Fx::new(Kind::Screen);
        fx.size = 1;
        let m = render(&scene(fx.clone()), &c, 0.0, &input);
        // Two lights is a "small device": average.
        assert_eq!(m[0], m[1]);
        let c4 = strip(4);
        let m4 = render(&scene(fx), &c4, 0.0, &input);
        assert!(m4[0].0 > 200 && m4[3].2 > 200);
    }

    #[test]
    fn layers_blend_and_target() {
        let c = strip(4);
        let base = Layer::new(Fx::new(Kind::Static).with_colors(&[BLUE]));
        let mut top = Layer::new(Fx::new(Kind::Static).with_colors(&[RED]));
        top.target = Target::Keys(vec![1, 2]);
        let f = render(&Scene { layers: vec![base.clone(), top.clone()] }, &c, 0.0, &Inputs::default());
        assert_eq!(f, vec![BLUE, RED, RED, BLUE]);
        top.blend = Blend::Add;
        let f = render(&Scene { layers: vec![base.clone(), top.clone()] }, &c, 0.0, &Inputs::default());
        assert_eq!(f[1], Rgb(255, 0, 255));
        top.blend = Blend::Normal;
        top.opacity = 50;
        let f = render(&Scene { layers: vec![base.clone(), top.clone()] }, &c, 0.0, &Inputs::default());
        assert!((126..=129).contains(&f[1].0) && (126..=129).contains(&f[1].2));
        top.enabled = false;
        let f = render(&Scene { layers: vec![base, top] }, &c, 0.0, &Inputs::default());
        assert!(f.iter().all(|x| *x == BLUE));
    }

    #[test]
    fn lit_blend_lets_dark_through() {
        let c = strip(10);
        let base = Layer::new(Fx::new(Kind::Static).with_colors(&[BLUE]));
        let mut top = Layer::new(Fx::new(Kind::Reactive).with_colors(&[RED]));
        top.blend = Blend::Lit;
        let input = Inputs { presses: vec![Press { at: 0.0, pixel: Some(0) }], ..Default::default() };
        let f = render(&Scene { layers: vec![base, top] }, &c, 0.0, &input);
        assert_eq!(f[0], RED);
        assert_eq!(f[9], BLUE);
    }

    #[test]
    fn zones_canvas_and_targets() {
        let leds = [("scroll".to_string(), 1), ("logo".to_string(), 4), ("underglow".to_string(), 10)];
        let c = Canvas::build(None, &leds, &[]);
        assert_eq!(c.pixels.len(), 3);
        assert!(c.pixels[0].y < c.pixels[2].y && c.pixels[2].y < c.pixels[1].y);
        let mut l = Layer::new(Fx::new(Kind::Static).with_colors(&[RED]));
        l.target = Target::Zones(vec!["logo".into()]);
        let f = render(&Scene { layers: vec![l] }, &c, 0.0, &Inputs::default());
        assert_eq!(f, vec![Rgb(0, 0, 0), RED, Rgb(0, 0, 0)]);
    }

    #[test]
    fn layout_places_keys() {
        let layout = [KeyPos { cell: 1, name: "esc".into(), x: 0.0, y: 0.0 }, KeyPos { cell: 2, name: "f1".into(), x: 9.0, y: 0.0 }];
        let c = Canvas::build(Some((1, 3)), &[], &layout);
        assert_eq!(c.pixels[1].key.as_deref(), Some("esc"));
        assert!((c.pixels[1].x - 0.05).abs() < 1e-3);
        assert!((c.pixels[2].x - 0.95).abs() < 1e-3);
    }

    #[test]
    fn scene_json_round_trip_and_defaults() {
        let json = r#"{"layers":[{"fx":{"kind":"wave","colors":["FF0000","0000FF"],"angle":90}},
            {"fx":{"kind":"reactive"},"target":{"type":"keys","list":[3,4]},"blend":"lit","opacity":60}]}"#;
        let s: Scene = serde_json::from_str(json).unwrap();
        assert_eq!(s.layers.len(), 2);
        assert_eq!(s.layers[0].fx.speed, 5);
        assert_eq!(s.layers[0].opacity, 100);
        assert!(s.layers[0].enabled);
        assert_eq!(s.layers[1].target, Target::Keys(vec![3, 4]));
        s.check().unwrap();
        let back: Scene = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert!(s.animated() && s.uses(Kind::Reactive) && !s.uses(Kind::Audio));
        let mut bad = s.clone();
        bad.layers[0].fx.speed = 11;
        assert!(bad.check().is_err());
    }

    #[test]
    fn layer_text_round_trips() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        let l = Layer::parse(&args("wave:FF0000,0000FF speed=7 angle=90 blend=lit opacity=60 keys=17,18 reverse")).unwrap();
        assert_eq!(l.fx.kind, Kind::Wave);
        assert_eq!(l.fx.colors, vec![RED, BLUE]);
        assert_eq!((l.fx.speed, l.fx.angle, l.opacity, l.blend), (7, 90, 60, Blend::Lit));
        assert_eq!(l.target, Target::Keys(vec![17, 18]));
        assert!(l.fx.reverse);
        let text = l.to_string();
        let back = Layer::parse(&text.split_whitespace().map(String::from).collect::<Vec<_>>()).unwrap();
        assert_eq!(back, l);
        let a = Layer::parse(&args("audio mode=stereo zones=logo,underglow")).unwrap();
        assert_eq!((a.fx.mode, a.target.clone()), (Mode::Stereo, Target::Zones(vec!["logo".into(), "underglow".into()])));
        for bad in ["sparkle", "wave speed=11", "wave angle=400", "wave blend=glow", "wave colour=1", "static:XYZ"] {
            assert!(Layer::parse(&args(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn streams_frames_and_zones_on_the_sim() {
        let sim = crate::sim::Sim::mouse();
        let dev = sim.open(crate::sim::MOUSE_PID, crate::sim::Sim::spec_for(crate::sim::MOUSE_PID)).unwrap();
        let canvas = Canvas::for_device(&dev, &[]);
        assert!(!canvas.pixels.is_empty() && canvas.grid.is_none());
        let s = scene(Fx::new(Kind::Spectrum));
        let a = render(&s, &canvas, 0.0, &Inputs::default());
        dev.start_stream(&canvas, &a).unwrap();
        let before = sim.state().log.len();
        dev.stream(&canvas, &a, Some(&a)).unwrap();
        assert_eq!(sim.state().log.len(), before, "unchanged frame sends nothing");
        let b = render(&s, &canvas, 1.0, &Inputs::default());
        dev.stream(&canvas, &b, Some(&a)).unwrap();
        assert!(sim.state().log.len() > before);
    }

    #[test]
    fn saved_lighting_comes_back_after_streaming() {
        let sim = crate::sim::Sim::mouse();
        let dev = sim.open(crate::sim::MOUSE_PID, crate::sim::Sim::spec_for(crate::sim::MOUSE_PID)).unwrap();
        let led = dev.spec.as_ref().unwrap().leds[0].id;
        dev.set_static_color(led, RED).unwrap();
        let canvas = Canvas::for_device(&dev, &[]);
        let frame = vec![BLUE; canvas.pixels.len()];
        dev.start_stream(&canvas, &frame).unwrap();
        assert_eq!(sim.state().effects[&led][4..7], [0, 0, 255]);
        assert!(dev.show_saved_lighting() >= 1);
        assert_eq!(sim.state().effects[&led][4..7], [255, 0, 0], "saved red is shown again");
    }
}
