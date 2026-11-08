// Blood Particles 1.1.0, a Horizon native mod by Bogdan_Tokarev: a hurt mob
// sprays blood — droplets that fly out of the wound, arc under gravity and
// spatter on the ground, a mist of its own colour, and a cloud for a heavy
// blow. The blow that kills throws three times as much.
//
// The client is told about every hit it can see: the server's damage packet
// (and the hurt animation) restart the mob's hurt flash, `LivingEntity.hurtTime`,
// which is counted down one a tick, and the mob's health arrives as synched
// data (`LivingEntity.DATA_HEALTH_ID`). So the mod needs no server and no
// packet of its own: a flash that went up is a hit, and the health that went
// down by it is how hard it was.
//
// This file is both the mod's Cargo library (its tests, `cargo test`) and the
// module the bundle adds to the game as `crate::blood_particles`. Everything
// here is engine-free on purpose: numbers, geometry and its own small state, so
// it can be tested without the game. `tick.rs`, `settings.rs`, `command.rs`
// and `list.rs` are what touch the engine.
//
// The game's own particles draw it, so the bundle ships no textures and no
// models: a droplet is the game's block-crumb particle (gravity, drag and
// terrain collision included) taken from a red block, the mist is its
// `dust_color_transition`, and the cloud is its `dust`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// The mod's id (its row, its settings folder), name, version and author.
pub const ID: &str = "blood_particles";
pub const NAME: &str = "Blood Particles";
pub const VERSION: &str = "1.2.0";
pub const AUTHOR: &str = "Bogdan_Tokarev";
pub const DESCRIPTION: &str = "Realistic blood: droplets that arc and spatter, a mist that dries darker, and three times as much on the blow that kills. Every part of it is configurable.";

/// What `/bloodparticles help` says, in one line: every setting of the page and
/// the command, and what kind of value each takes.
pub const HELP: &str = "droplets, mist, clouds, scaling (off/soft/full), death, spread, arc, \
colours (on/off), crumbs (on/off), reach, wounds, particles: /bloodparticles <setting> <value>, \
or reset.";

/// `LivingEntity.hurtTime` of a fresh hurt: the client restarts the flash at
/// this value on every hit it hears and counts it down one a tick.
pub const HURT_FLASH: i32 = 10;

/// A hit of this much damage (three hearts) throws the configured number of
/// particles; a lighter one throws fewer, a heavier one more.
pub const REFERENCE_DAMAGE: f32 = 6.0;
/// The least and the most a hit's damage may scale a burst by, however light
/// or heavy it was: a punch still spits, a mace still looks like a spray and
/// not like a fountain.
pub const DAMAGE_MIN_FACTOR: f32 = 0.35;
pub const DAMAGE_MAX_FACTOR: f32 = 2.5;
/// The blow that kills multiplies every count by this, by default.
pub const DEATH_MULTIPLIER: f32 = 3.0;
/// A wound waits this many ticks for the mob's new health to arrive before it
/// is drawn with the un-scaled count: the health of a hit comes in the same
/// tick's synched data, and a tick's delay is not visible.
pub const WOUND_WAIT: u32 = 3;
/// Wounds remembered at once, waiting for their health; past it the oldest is
/// drawn as it is.
pub const PENDING_MAX: usize = 64;
/// How far from the centre a position may be for blood to be drawn at it: the
/// game's own build limit sideways, and a generous height of the space a
/// dimension is built in. A mob outside it is a packet the client cannot place,
/// and its blood would be painted in the wrong world.
pub const WORLD_LIMIT: f64 = 3.0e7;
pub const HEIGHT_LIMIT: f64 = 1.0e6;
/// Mobs remembered at once.
pub const REMEMBERED_MAX: usize = 4096;
/// Ceilings nothing can raise a burst past, whatever the settings say.
pub const DROPLET_CAP: usize = 64;
pub const MIST_CAP: usize = 48;
pub const CLOUD_CAP: usize = 12;

thread_local! {
    /// The mod's switch, as Options > Mods has it.
    static ON: Cell<bool> = const { Cell::new(true) };
    /// Ticks since the world was opened, counted by the mod's own hook.
    static TICKS: Cell<u32> = const { Cell::new(0) };
    /// The player's settings.
    static CONFIG: RefCell<Config> = const { RefCell::new(Config::DEFAULT) };
    /// Whether the settings were read from the game's file yet.
    static LOADED: Cell<bool> = const { Cell::new(false) };
    /// Each mob's last flash, death and health.
    static SEEN: RefCell<HashMap<u32, Seen>> = RefCell::new(HashMap::new());
    /// Wounds whose health has not come in yet.
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
}

// ---------------------------------------------------------------- settings

/// How a hit's damage changes the size of its burst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    /// Every hit throws the same amount.
    Off,
    /// Half as much of the difference: a light hit is a little smaller, a heavy
    /// one a little bigger.
    Soft,
    /// The whole difference: a hit of three hearts throws the configured
    /// amount, a lighter hit less, a heavier one more.
    Full,
}

impl Scaling {
    pub const ALL: [Scaling; 3] = [Self::Off, Self::Soft, Self::Full];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Soft => "soft",
            Self::Full => "full",
        }
    }

    pub fn from_label(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.label() == text)
    }

    /// How much a hit of `damage` multiplies the counts by (1.0 for a hit that
    /// names no damage, which is a hit the client was not told the health of).
    pub fn factor(self, damage: Option<f32>) -> f32 {
        let Some(damage) = damage.filter(|d| d.is_finite() && *d > 0.0) else {
            return 1.0;
        };
        let raw = match self {
            Self::Off => return 1.0,
            Self::Soft => 1.0 + (damage / REFERENCE_DAMAGE - 1.0) * 0.5,
            Self::Full => damage / REFERENCE_DAMAGE,
        };
        raw.clamp(DAMAGE_MIN_FACTOR, DAMAGE_MAX_FACTOR)
    }
}

/// Everything the player can change, from the mod's page in Options > Mods or
/// with `/bloodparticles`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Droplets thrown by a hit of [`REFERENCE_DAMAGE`].
    pub droplets: usize,
    /// Puffs of mist around it.
    pub mist: usize,
    /// Clouds of spray around it: the biggest particles, for a heavy blow.
    pub clouds: usize,
    /// Whether a hit's damage sizes the burst.
    pub scaling: Scaling,
    /// What the blow that kills multiplies every count by.
    pub death: f32,
    /// How wide the spray fans out, 0.4 (a jet) to 1.6 (a splash).
    pub spread: f32,
    /// How much of the spray goes up rather than out, 0.2 to 1.5.
    pub arc: f32,
    /// Each mob's own blood colour, or red for all.
    pub colours: bool,
    /// Droplets as crumbs of a block (on) or mist alone (off).
    pub crumbs: bool,
    /// Hits farther than this from the player are not drawn.
    pub reach: f64,
    /// Wounds drawn in one tick at most.
    pub wounds: usize,
    /// Particles one tick may ask for at most.
    pub particles: usize,
}

/// The value lists the settings page cycles through, and the defaults.
pub const DROPLET_STEPS: [usize; 8] = [0, 4, 6, 8, 12, 16, 20, 26];
pub const MIST_STEPS: [usize; 6] = [0, 2, 4, 6, 8, 12];
pub const CLOUD_STEPS: [usize; 5] = [0, 1, 2, 3, 4];
pub const DEATH_STEPS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 5.0];
pub const SPREAD_STEPS: [f32; 3] = [0.4, 1.0, 1.6];
pub const ARC_STEPS: [f32; 3] = [0.3, 0.75, 1.2];
pub const REACH_STEPS: [f64; 5] = [16.0, 24.0, 32.0, 40.0, 64.0];
pub const WOUND_STEPS: [usize; 5] = [2, 4, 8, 12, 16];
pub const PARTICLE_STEPS: [usize; 5] = [32, 64, 112, 160, 256];

impl Config {
    pub const DEFAULT: Self = Self {
        droplets: 8,
        mist: 6,
        clouds: 2,
        scaling: Scaling::Full,
        death: DEATH_MULTIPLIER,
        spread: 1.0,
        arc: 0.75,
        colours: true,
        crumbs: true,
        reach: 40.0,
        wounds: 8,
        particles: 112,
    };

    /// The settings as one line, which is what the game keeps for the mod.
    pub fn encode(&self) -> String {
        format!(
            "droplets={};mist={};clouds={};scaling={};death={};spread={};arc={};colours={};\
             crumbs={};reach={};wounds={};particles={}",
            self.droplets,
            self.mist,
            self.clouds,
            self.scaling.label(),
            self.death,
            self.spread,
            self.arc,
            self.colours,
            self.crumbs,
            self.reach,
            self.wounds,
            self.particles
        )
    }

    /// Reads [`Config::encode`]'s line. Anything missing, unknown or
    /// impossible keeps its default, so a damaged line still loads.
    pub fn decode(text: &str) -> Self {
        let mut config = Self::DEFAULT;
        for field in text.split(';') {
            let Some((key, value)) = field.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "droplets" => {
                    if let Ok(v) = value.parse() {
                        config.droplets = step_usize(&DROPLET_STEPS, v);
                    }
                }
                "mist" => {
                    if let Ok(v) = value.parse() {
                        config.mist = step_usize(&MIST_STEPS, v);
                    }
                }
                "clouds" => {
                    if let Ok(v) = value.parse() {
                        config.clouds = step_usize(&CLOUD_STEPS, v);
                    }
                }
                "scaling" => {
                    if let Some(s) = Scaling::from_label(value) {
                        config.scaling = s;
                    }
                }
                "death" => {
                    if let Ok(v) = value.parse() {
                        config.death = step_f32(&DEATH_STEPS, v);
                    }
                }
                "spread" => {
                    if let Ok(v) = value.parse() {
                        config.spread = step_f32(&SPREAD_STEPS, v);
                    }
                }
                "arc" => {
                    if let Ok(v) = value.parse() {
                        config.arc = step_f32(&ARC_STEPS, v);
                    }
                }
                "colours" => config.colours = value != "false",
                "crumbs" => config.crumbs = value != "false",
                "reach" => {
                    if let Ok(v) = value.parse() {
                        config.reach = step_f64(&REACH_STEPS, v);
                    }
                }
                "wounds" => {
                    if let Ok(v) = value.parse() {
                        config.wounds = step_usize(&WOUND_STEPS, v);
                    }
                }
                "particles" => {
                    if let Ok(v) = value.parse() {
                        config.particles = step_usize(&PARTICLE_STEPS, v);
                    }
                }
                _ => {}
            }
        }
        config
    }

    /// The labels of the settings page, in the order [`Config::cycle`] takes
    /// them. `button` is the index the game reports a click with.
    pub fn labels(&self) -> Vec<String> {
        vec![
            format!("Droplets a hit: {}", self.droplets),
            format!("Mist a hit: {}", self.mist),
            format!("Clouds a hit: {}", self.clouds),
            format!("Damage sizes the burst: {}", self.scaling.label()),
            format!("The killing blow throws: x{}", trim(self.death)),
            format!("Spread: {}", spread_label(self.spread)),
            format!("Arc: {}", arc_label(self.arc)),
            format!("Each mob's own colour: {}", on_off(self.colours)),
            format!("Droplets are block crumbs: {}", on_off(self.crumbs)),
            format!("Drawn within: {} blocks", trim(self.reach as f32)),
            format!("Wounds a tick: {}", self.wounds),
            format!("Particles a tick: {}", self.particles),
            "Reset to defaults".to_string(),
        ]
    }

    /// What button `button` of the page does: moves its setting to its next
    /// value, or resets everything on the last one.
    pub fn cycle(&mut self, button: usize) -> String {
        match button {
            0 => self.droplets = next(&DROPLET_STEPS, self.droplets),
            1 => self.mist = next(&MIST_STEPS, self.mist),
            2 => self.clouds = next(&CLOUD_STEPS, self.clouds),
            3 => self.scaling = next(&Scaling::ALL, self.scaling),
            4 => self.death = next(&DEATH_STEPS, self.death),
            5 => self.spread = next(&SPREAD_STEPS, self.spread),
            6 => self.arc = next(&ARC_STEPS, self.arc),
            7 => self.colours = !self.colours,
            8 => self.crumbs = !self.crumbs,
            9 => self.reach = next(&REACH_STEPS, self.reach),
            10 => self.wounds = next(&WOUND_STEPS, self.wounds),
            11 => self.particles = next(&PARTICLE_STEPS, self.particles),
            _ => {
                *self = Self::DEFAULT;
                return "everything back to its defaults".to_string();
            }
        }
        self.labels().get(button).cloned().unwrap_or_default()
    }
}

/// What the settings page and `/bloodparticles` call the settings.
pub const KEYS: [&str; 12] = [
    "droplets",
    "mist",
    "clouds",
    "scaling",
    "death",
    "spread",
    "arc",
    "colours",
    "crumbs",
    "reach",
    "wounds",
    "particles",
];

impl Config {
    /// Changes one setting by name, the way `/bloodparticles <key> <value>`
    /// does. Values snap to the nearest one the page can show.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        let number = |text: &str| {
            text.parse::<f64>()
                .map_err(|_| format!("'{text}' is not a number"))
        };
        let yes_no = |text: &str| match text {
            "on" | "true" | "yes" | "1" => Ok(true),
            "off" | "false" | "no" | "0" => Ok(false),
            other => Err(format!("'{other}' is not on or off")),
        };
        match key {
            "droplets" => self.droplets = step_usize(&DROPLET_STEPS, number(value)? as usize),
            "mist" => self.mist = step_usize(&MIST_STEPS, number(value)? as usize),
            "clouds" => self.clouds = step_usize(&CLOUD_STEPS, number(value)? as usize),
            "scaling" => {
                self.scaling = Scaling::from_label(value)
                    .ok_or_else(|| format!("'{value}' is not off, soft or full"))?
            }
            "death" => self.death = step_f32(&DEATH_STEPS, number(value)? as f32),
            "spread" => self.spread = step_f32(&SPREAD_STEPS, number(value)? as f32),
            "arc" => self.arc = step_f32(&ARC_STEPS, number(value)? as f32),
            "colours" | "colors" => self.colours = yes_no(value)?,
            "crumbs" => self.crumbs = yes_no(value)?,
            "reach" => self.reach = step_f64(&REACH_STEPS, number(value)?),
            "wounds" => self.wounds = step_usize(&WOUND_STEPS, number(value)? as usize),
            "particles" => self.particles = step_usize(&PARTICLE_STEPS, number(value)? as usize),
            _ => return Err(format!("there is no setting called '{key}'")),
        }
        Ok(())
    }

    /// The settings in a few lines, for `/bloodparticles`.
    pub fn describe(&self) -> Vec<String> {
        let labels = self.labels();
        vec![
            labels[0..4].join(", "),
            labels[4..8].join(", "),
            labels[8..12].join(", "),
        ]
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The nearest value a list holds, so neither a hand-edited file nor a
/// command can set something the settings page cannot show.
fn step_usize(steps: &[usize], value: usize) -> usize {
    *steps
        .iter()
        .min_by_key(|step| step.abs_diff(value))
        .unwrap_or(&value)
}

fn step_f32(steps: &[f32], value: f32) -> f32 {
    steps
        .iter()
        .copied()
        .min_by(|a, b| {
            let key = |v: f32| {
                if v.is_finite() {
                    (v - value).abs()
                } else {
                    f32::MAX
                }
            };
            key(*a)
                .partial_cmp(&key(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(value)
}

fn step_f64(steps: &[f64], value: f64) -> f64 {
    steps
        .iter()
        .copied()
        .min_by(|a, b| {
            let key = |v: f64| {
                if v.is_finite() {
                    (v - value).abs()
                } else {
                    f64::MAX
                }
            };
            key(*a)
                .partial_cmp(&key(*b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(value)
}

fn next<T: PartialEq + Copy>(steps: &[T], value: T) -> T {
    match steps.iter().position(|s| *s == value) {
        Some(i) if i + 1 < steps.len() => steps[i + 1],
        _ => steps[0],
    }
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn trim(value: f32) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches(".0").to_string()
}

fn spread_label(value: f32) -> &'static str {
    match value {
        v if v < 0.7 => "jet",
        v if v > 1.3 => "splash",
        _ => "normal",
    }
}

fn arc_label(value: f32) -> &'static str {
    match value {
        v if v < 0.5 => "low",
        v if v > 1.0 => "high",
        _ => "normal",
    }
}

/// The player's settings.
pub fn config() -> Config {
    CONFIG.with(|config| *config.borrow())
}

/// Replaces the player's settings.
pub fn set_config(config: Config) {
    CONFIG.with(|cell| *cell.borrow_mut() = config);
}

/// Changes one setting, as a click on the page does, and says what it became.
pub fn cycle(button: usize) -> String {
    CONFIG.with(|cell| {
        let mut config = cell.borrow_mut();
        config.cycle(button)
    })
}

/// Whether the settings were read from the game's file yet.
pub fn loaded() -> bool {
    LOADED.with(Cell::get)
}

/// Reads the settings the game kept for the mod (nothing kept yet means the
/// defaults), and remembers that it did.
pub fn load(text: Option<&str>) {
    let config = text.map(Config::decode).unwrap_or(Config::DEFAULT);
    set_config(config);
    LOADED.with(|loaded| loaded.set(true));
}

// ------------------------------------------------------------------ blood

/// The mod's switch, as Options > Mods has it.
pub fn set_on(on: bool) {
    ON.with(|cell| cell.set(on));
}

pub fn on() -> bool {
    ON.with(Cell::get)
}

/// Starts a tick: the mod's clock goes up by one, and the tick number the
/// bursts of this tick are timed against is returned.
pub fn begin_tick() -> u32 {
    TICKS.with(|ticks| {
        let now = ticks.get().wrapping_add(1);
        ticks.set(now);
        now
    })
}

/// Forgets every mob, wound and hit the mod has drawn: what a new world needs,
/// because a mob id starts again from there and the old flash it is compared
/// with belongs to another world's mob.
pub fn forget_all() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    PENDING.with(|pending| pending.borrow_mut().clear());
    BLED.with(|bled| bled.borrow_mut().clear());
}

/// How many mobs are remembered on this thread.
pub fn remembered() -> usize {
    SEEN.with(|seen| seen.borrow().len())
}

/// How many wounds wait for their health.
pub fn waiting() -> usize {
    PENDING.with(|pending| pending.borrow().len())
}

/// Which blood a mob has.
///
/// Most mobs bleed red. The ones the game gives a body of another kind keep
/// their own colour, so a slime sprays slime, a blaze fire and an enderman the
/// violet of the End; the undead that are only bones have no blood at all and
/// shed pale chips of themselves instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blood {
    Red,
    Slime,
    Fire,
    Ichor,
    Bone,
}

/// Every kind of blood, for the command and the preview.
pub const BLOODS: [Blood; 5] = [
    Blood::Red,
    Blood::Slime,
    Blood::Fire,
    Blood::Ichor,
    Blood::Bone,
];

impl Blood {
    /// The mist's colour at the wound: dark, wet and saturated, the way blood
    /// leaves a body.
    pub fn mist(self) -> [f32; 3] {
        match self {
            Self::Red => [0.42, 0.02, 0.02],
            Self::Slime => [0.28, 0.52, 0.16],
            Self::Fire => [0.86, 0.34, 0.04],
            Self::Ichor => [0.38, 0.08, 0.52],
            Self::Bone => [0.78, 0.76, 0.68],
        }
    }

    /// The colour it dries to before it goes.
    pub fn mist_to(self) -> [f32; 3] {
        match self {
            Self::Red => [0.13, 0.01, 0.01],
            Self::Slime => [0.11, 0.22, 0.05],
            Self::Fire => [0.32, 0.09, 0.01],
            Self::Ichor => [0.14, 0.02, 0.20],
            Self::Bone => [0.33, 0.31, 0.27],
        }
    }

    /// The colour of the cloud of spray: the mist's, a little brighter, since a
    /// cloud is thinner than the jet it comes from.
    pub fn cloud(self) -> [f32; 3] {
        let mist = self.mist();
        [
            (mist[0] * 1.35).min(1.0),
            (mist[1] * 1.35).min(1.0),
            (mist[2] * 1.35).min(1.0),
        ]
    }

    /// The average colour of that block's texture, which is what the game tints
    /// its crumbs with (and then darkens to 60%), as close as a preview can
    /// come to what the pack draws. The mod itself never needs it: the game
    /// reads the pack.
    pub fn crumb(self, bright: bool) -> [f32; 3] {
        match (self, bright) {
            (Self::Red, false) => [0.45, 0.06, 0.08],
            (Self::Red, true) => [0.62, 0.04, 0.05],
            (Self::Slime, false) => [0.47, 0.68, 0.27],
            (Self::Slime, true) => [0.55, 0.85, 0.2],
            (Self::Fire, false) => [0.48, 0.22, 0.09],
            (Self::Fire, true) => [0.9, 0.45, 0.08],
            (Self::Ichor, false) => [0.67, 0.46, 0.68],
            (Self::Ichor, true) => [0.5, 0.19, 0.7],
            (Self::Bone, false) => [0.85, 0.82, 0.71],
            (Self::Bone, true) => [0.95, 0.94, 0.9],
        }
    }

    /// The block whose crumbs the droplets are: the game's block-crumb particle
    /// draws the block's texture, falls under gravity and stops on the ground,
    /// which is what makes a droplet look like a droplet. The dark one is most
    /// of the spray, the bright one the highlights in it; every one of these is
    /// a plain block the base game always has.
    pub fn block(self, bright: bool) -> &'static str {
        match (self, bright) {
            (Self::Red, false) => "minecraft:nether_wart_block",
            (Self::Red, true) => "minecraft:redstone_block",
            (Self::Slime, false) => "minecraft:slime_block",
            (Self::Slime, true) => "minecraft:lime_concrete",
            (Self::Fire, false) => "minecraft:magma_block",
            (Self::Fire, true) => "minecraft:orange_concrete",
            (Self::Ichor, false) => "minecraft:purpur_block",
            (Self::Ichor, true) => "minecraft:purple_concrete",
            (Self::Bone, false) => "minecraft:bone_block",
            (Self::Bone, true) => "minecraft:quartz_block",
        }
    }

    /// The kind's name, for chat and for the preview.
    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Slime => "slime",
            Self::Fire => "fire",
            Self::Ichor => "ichor",
            Self::Bone => "bone chips",
        }
    }
}

/// The blood of the mob of type `kind` (`minecraft:zombie`).
pub fn blood_of(kind: &str) -> Blood {
    match kind.strip_prefix("minecraft:").unwrap_or(kind) {
        "slime" => Blood::Slime,
        "blaze" | "magma_cube" | "strider" => Blood::Fire,
        "enderman" | "endermite" | "shulker" | "ender_dragon" => Blood::Ichor,
        "skeleton" | "stray" | "bogged" | "wither_skeleton" | "skeleton_horse"
        | "zombified_piglin" => Blood::Bone,
        _ => Blood::Red,
    }
}

// ------------------------------------------------------------------ bursts

/// What one particle of a burst is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    /// A droplet: a crumb of the dark block, under gravity.
    Droplet,
    /// A droplet of the bright block: the highlight in the spray.
    Highlight,
    /// A puff of mist: the tinted dust particle, drifting and drying.
    Mist,
    /// A cloud of spray: the tinted dust particle at its largest.
    Cloud,
}

/// One particle of a burst: where it starts, how it leaves, what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drop {
    pub kind: DropKind,
    pub position: [f64; 3],
    pub velocity: [f64; 3],
}

/// Everything one wound throws.
#[derive(Clone, Debug, PartialEq)]
pub struct Burst {
    pub blood: Blood,
    pub death: bool,
    /// The damage the burst was sized by, when the client was told it.
    pub damage: Option<f32>,
    pub drops: Vec<Drop>,
}

impl Burst {
    pub fn of(&self, kind: DropKind) -> impl Iterator<Item = &Drop> {
        self.drops.iter().filter(move |d| d.kind == kind)
    }
}

/// A small deterministic generator (xorshift64\*), so a burst is the same
/// burst every time it is planned from the same seed: the mod never draws from
/// the game's own random streams, whose order the rest of the game depends on.
pub struct Rng(u64);

impl Rng {
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A value in `0.0..1.0`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A value in `lo..hi`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// A unit vector, uniformly spread over the sphere.
    fn direction(&mut self) -> [f64; 3] {
        let mut v = [
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
            self.range(-1.0, 1.0),
        ];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 1.0e-6 {
            v = [v[0] / l, v[1] / l, v[2] / l];
            v
        } else {
            [0.0, 1.0, 0.0]
        }
    }
}

/// The seed of one burst: the mob, the tick and the flash together, so two mobs
/// hit on the same tick bleed differently and the same hit drawn twice throws
/// the same drops.
pub fn burst_seed(id: u32, now: u32, flash: i32) -> u64 {
    (u64::from(id) << 32) ^ (u64::from(now) << 8) ^ (flash as u32 as u64).wrapping_mul(0x9E37_79B9)
}

/// What the mod reads of one mob this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sighting<'a> {
    /// The entity id.
    pub id: u32,
    /// `LivingEntity.hurtTime`: restarted at [`HURT_FLASH`] by every hit the
    /// client hears, counted down one a tick.
    pub flash: i32,
    /// `LivingEntity.deathTime`: the death animation has started.
    pub death_time: i32,
    /// `LivingEntity.getHealth` as the server last sent it.
    pub health: f32,
    /// The mob's type (`minecraft:zombie`), which chooses its blood.
    pub kind: &'a str,
    /// Its feet.
    pub feet: [f64; 3],
    /// Its bounding box's height: the wound sits in its upper body.
    pub height: f64,
}

/// What the mod remembers about one mob.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    /// `LivingEntity.hurtTime` when the mob was last looked at: a flash that
    /// went up since means the client heard a hit.
    flash: i32,
    /// Whether the death animation had started, so it is only seen once.
    dead: bool,
    /// The health the server had last sent: the top of the next hit's damage.
    health: f32,
}

/// A wound the client has heard of, waiting to learn how hard it was.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pending {
    id: u32,
    blood: Blood,
    /// The wound itself, and the way the blood leaves it.
    feet: [f64; 3],
    height: f64,
    away: [f64; 3],
    /// The health before the hit, and thus the top of its damage.
    health: f32,
    death: bool,
    tick: u32,
    seed: u64,
}

/// Looks at one mob this tick: a hurt flash that went up is a hit, and a death
/// that has just started is the blow that killed it. The burst itself waits
/// for the mob's new health, so it can be sized by the damage.
pub fn look(mob: Sighting<'_>, player: [f64; 3], now: u32) {
    if !on() {
        // Off means off, and it means the queue too: a wound left waiting while
        // the switch is off would be drawn the tick it is turned back on, which
        // is blood for a hit from before the player asked for none.
        if remembered() > 0 || waiting() > 0 {
            forget_all();
        }
        return;
    }
    // A position the world cannot hold — no number at all, a number that never
    // ends, or a coordinate past the build limit — is dropped here instead of
    // passed on: a NaN in `ParticleEngine::add` is a particle that never lands,
    // and a float big enough turns into the wrong block when the droplet asks
    // for the crumbs of the block it fell on. Skipping the wound whole is also
    // what a mob the server has not positioned yet wants.
    if !drawable(&mob.feet) || !mob.height.is_finite() || !drawable(&player) {
        return;
    }
    let config = config();
    let dx = mob.feet[0] - player[0];
    let dy = mob.feet[1] - player[1];
    let dz = mob.feet[2] - player[2];
    if (dx * dx + dy * dy + dz * dz).sqrt() > config.reach {
        return;
    }
    let dead = mob.death_time > 0;
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let previous = seen.get(&mob.id).copied();
        if seen.len() >= REMEMBERED_MAX && previous.is_none() {
            seen.clear();
        }
        seen.insert(
            mob.id,
            Seen {
                flash: mob.flash,
                dead,
                health: mob.health,
            },
        );
        let Some(previous) = previous else {
            return;
        };
        let blood = if config.colours {
            blood_of(mob.kind)
        } else {
            Blood::Red
        };
        let push = |death: bool, seed: u64| {
            let wound = Pending {
                id: mob.id,
                blood,
                feet: mob.feet,
                height: mob.height,
                away: away_from(mob.feet, mob.height, player),
                health: if death { 0.0 } else { previous.health },
                death,
                tick: now,
                seed,
            };
            PENDING.with(|pending| {
                let mut pending = pending.borrow_mut();
                if pending.len() >= PENDING_MAX {
                    pending.remove(0);
                }
                pending.push(wound);
            });
        };
        if dead && !previous.dead {
            // The blow that kills: its own burst, whatever the flash says.
            push(true, burst_seed(mob.id, now, HURT_FLASH));
        } else if !dead && mob.flash > previous.flash && !bled_recently(mob.id, now) {
            // A hit the client heard: the flash restarted.
            push(false, burst_seed(mob.id, now, mob.flash));
        }
    });
}

thread_local! {
    /// When each mob last bled, so a packet and an animation for the same blow
    /// make one burst.
    static BLED: RefCell<HashMap<u32, u32>> = RefCell::new(HashMap::new());
}

fn bled_recently(id: u32, now: u32) -> bool {
    BLED.with(|bled| {
        let mut bled = bled.borrow_mut();
        let recent = bled
            .get(&id)
            .is_some_and(|tick| now.wrapping_sub(*tick) < 3);
        if !recent {
            if bled.len() >= REMEMBERED_MAX {
                bled.clear();
            }
            bled.insert(id, now);
        }
        recent
    })
}

/// The direction a wound sprays in: away from the player, flat. Blood leaves
/// the far side of a blow, and the player is who the mob was hit by.
fn away_from(feet: [f64; 3], height: f64, player: [f64; 3]) -> [f64; 3] {
    let middle = [feet[0], feet[1] + height * 0.62, feet[2]];
    let mut away = [middle[0] - player[0], 0.0, middle[2] - player[2]];
    let length = (away[0] * away[0] + away[2] * away[2]).sqrt();
    if length > 1.0e-4 {
        away[0] /= length;
        away[2] /= length;
        away
    } else {
        [1.0, 0.0, 0.0]
    }
}

/// Whether a position is one the world can hold: `x` and `z` inside the game's
/// own build limit and `y` inside the space a dimension is built in. The
/// comparisons are what makes this also reject a NaN and an infinity, which are
/// never `<=` anything.
fn drawable(v: &[f64; 3]) -> bool {
    v[0].abs() <= WORLD_LIMIT && v[2].abs() <= WORLD_LIMIT && v[1].abs() <= HEIGHT_LIMIT
}

/// Ends a tick: every wound whose damage is known, or whose wait is over,
/// becomes a burst. `health` is the health of the mobs the client has now.
pub fn resolve(health: &[(u32, f32)], now: u32) -> Vec<Burst> {
    if !on() {
        forget_all();
        return Vec::new();
    }
    let config = config();
    let mut ready = Vec::new();
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let mut waiting = Vec::with_capacity(pending.len());
        for wound in pending.drain(..) {
            let now_health = health
                .iter()
                .find(|(id, _)| *id == wound.id)
                .map(|(_, h)| *h)
                .filter(|h| h.is_finite());
            let damage = now_health
                .map(|h| (wound.health - h).max(0.0))
                .filter(|d| *d > 0.0);
            let expired = now.wrapping_sub(wound.tick) >= WOUND_WAIT;
            // Drawn as soon as the damage is known, and otherwise when the
            // wait is over: a hit that took nothing is still a hit.
            if wound.death || damage.is_some() || expired {
                ready.push(burst(&config, &wound, damage));
            } else {
                waiting.push(wound);
            }
        }
        *pending = waiting;
    });
    let mut ready: Vec<Burst> = ready;
    ready.truncate(config.wounds);
    // The page's `particles` is what the game may be asked to draw in one tick,
    // and these drops are exactly that ask. Trimming them here — rather than
    // only in the draw loop that reads the number — is what makes the promise
    // the mod's own, testable without a window and true for whoever calls
    // `resolve`, since a caller that forgot the cap could not blow the limit
    // up even by accident.
    let mut left = config.particles;
    ready.retain_mut(|burst| {
        if left == 0 {
            return false;
        }
        let keep = burst.drops.len().min(left);
        burst.drops.truncate(keep);
        left -= keep;
        true
    });
    ready
}

/// How many of each particle a wound throws.
pub fn counts(config: &Config, damage: Option<f32>, death: bool) -> (usize, usize, usize) {
    let factor = config.scaling.factor(damage);
    let multiplier = if death { config.death } else { 1.0 };
    let scale = |base: usize, cap: usize| -> usize {
        let scaled = (base as f32 * factor * multiplier).round();
        if !scaled.is_finite() || scaled <= 0.0 {
            0
        } else {
            (scaled as usize).min(cap)
        }
    };
    (
        scale(config.droplets, DROPLET_CAP),
        scale(config.mist, MIST_CAP),
        scale(config.clouds, CLOUD_CAP),
    )
}

fn burst(config: &Config, wound: &Pending, damage: Option<f32>) -> Burst {
    let (droplets, mist, clouds) = counts(config, damage, wound.death);
    // A death throws wider and slower: a body coming apart, not a cut.
    let spread = if wound.death {
        config.spread * 1.4
    } else {
        config.spread
    };
    let mut rng = Rng::new(wound.seed);
    let mut drops = Vec::with_capacity(droplets + mist + clouds);
    let wound_at = [
        wound.feet[0],
        wound.feet[1] + wound.height * 0.62,
        wound.feet[2],
    ];
    if config.crumbs {
        for i in 0..droplets {
            let v = rng.direction();
            // Every fourth droplet is a bright one: the highlights in a spray.
            let kind = if i % 4 == 3 {
                DropKind::Highlight
            } else {
                DropKind::Droplet
            };
            drops.push(drop(kind, wound_at, wound.away, spread, config.arc, v));
        }
    }
    for _ in 0..mist {
        let v = rng.direction();
        drops.push(drop(
            DropKind::Mist,
            wound_at,
            wound.away,
            spread,
            config.arc,
            v,
        ));
    }
    for _ in 0..clouds {
        let v = rng.direction();
        drops.push(drop(
            DropKind::Cloud,
            wound_at,
            wound.away,
            spread,
            config.arc,
            v,
        ));
    }
    Burst {
        blood: wound.blood,
        death: wound.death,
        damage,
        drops,
    }
}

/// One particle: pulled towards the far side of the blow, fanned out by
/// `spread`, lifted by `arc`, and started inside the wound.
fn drop(
    kind: DropKind,
    wound: [f64; 3],
    away: [f64; 3],
    spread: f32,
    arc: f32,
    v: [f64; 3],
) -> Drop {
    let out = f64::from(spread);
    let up = f64::from(arc);
    // How far from the jet a particle may stray, and how big it is.
    let (weight, speed, reach, lift) = match kind {
        DropKind::Droplet => (1.0, 0.30, 0.26, 1.0),
        DropKind::Highlight => (1.15, 0.42, 0.30, 1.1),
        DropKind::Mist => (0.55, 0.09, 0.16, 0.5),
        DropKind::Cloud => (0.45, 0.06, 0.10, 0.35),
    };
    let scale = f64::from(spread.max(0.1));
    let velocity = [
        (v[0] * 0.6 * out + away[0] * weight) * speed * scale,
        (v[1] * 0.6 + up * lift) * speed,
        (v[2] * 0.6 * out + away[2] * weight) * speed * scale,
    ];
    Drop {
        kind,
        position: [
            wound[0] + v[0] * reach,
            wound[1] + v[1] * reach * 0.7,
            wound[2] + v[2] * reach,
        ],
        velocity,
    }
}

/// The scale the game draws a mist puff at (`DustColorTransitionOptions.scale`).
pub fn mist_scale() -> f32 {
    0.85
}

/// The scale the game draws a cloud of spray at: large enough to read as a
/// cloud of red at arm's length, small enough to fade in a second or three
/// (the game's dust lives `8/(r*0.8+0.2) * scale` ticks, so 12 to 64 here).
pub fn cloud_scale() -> f32 {
    1.6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset() {
        forget_all();
        set_on(true);
        LOADED.with(|l| l.set(true));
        set_config(Config::DEFAULT);
        BLED.with(|bled| bled.borrow_mut().clear());
    }

    fn sighting(id: u32, flash: i32, death: i32, health: f32) -> Sighting<'static> {
        Sighting {
            id,
            flash,
            death_time: death,
            health,
            kind: "minecraft:zombie",
            feet: [0.0, 64.0, 0.0],
            height: 1.95,
        }
    }

    /// A hit that took `damage` off a mob of `health`, drawn at once.
    fn hit(id: u32, health: f32, damage: f32, now: u32) -> Vec<Burst> {
        look(
            sighting(id, 0, 0, health),
            [2.0, 64.0, 0.0],
            now.wrapping_sub(1),
        );
        look(sighting(id, HURT_FLASH, 0, health), [2.0, 64.0, 0.0], now);
        resolve(&[(id, health - damage)], now)
    }

    #[test]
    fn a_hurt_mob_bleeds_its_own_kind_of_blood() {
        assert_eq!(blood_of("minecraft:zombie"), Blood::Red);
        assert_eq!(blood_of("minecraft:slime"), Blood::Slime);
        assert_eq!(blood_of("minecraft:magma_cube"), Blood::Fire);
        assert_eq!(blood_of("minecraft:enderman"), Blood::Ichor);
        assert_eq!(blood_of("minecraft:skeleton"), Blood::Bone);
        assert_eq!(blood_of("my_mod:golem"), Blood::Red);
        assert_eq!(blood_of("zombie"), Blood::Red);
    }

    #[test]
    fn every_kind_of_blood_has_its_own_colours_and_blocks() {
        for (i, a) in BLOODS.iter().enumerate() {
            for bright in [false, true] {
                assert!(a.block(bright).starts_with("minecraft:"), "{a:?}");
            }
            assert_ne!(a.block(false), a.block(true), "{a:?}");
            assert_ne!(a.mist(), a.mist_to(), "a mist that never dries: {a:?}");
            for c in a
                .mist()
                .iter()
                .chain(a.mist_to().iter())
                .chain(a.cloud().iter())
            {
                assert!((0.0..=1.0).contains(c), "{a:?}: {c}");
            }
            for b in &BLOODS[i + 1..] {
                assert_ne!(a.mist(), b.mist(), "{a:?} and {b:?} look alike");
                assert_ne!(
                    a.block(false),
                    b.block(false),
                    "{a:?} and {b:?} crumble alike"
                );
            }
        }
    }

    #[test]
    fn a_hit_throws_what_the_settings_say() {
        reset();
        let config = Config::DEFAULT;
        let (droplets, mist, clouds) = counts(&config, Some(REFERENCE_DAMAGE), false);
        assert_eq!(
            (droplets, mist, clouds),
            (config.droplets, config.mist, config.clouds)
        );
    }

    #[test]
    fn the_harder_the_hit_the_more_blood() {
        let config = Config::DEFAULT;
        let light = counts(&config, Some(1.0), false).0;
        let normal = counts(&config, Some(REFERENCE_DAMAGE), false).0;
        let heavy = counts(&config, Some(20.0), false).0;
        assert!(light < normal, "{light} < {normal}");
        assert!(normal < heavy, "{normal} < {heavy}");
        // A hit lighter than the floor still spits, one heavier than the
        // ceiling still looks like a spray and not a fountain.
        assert_eq!(
            counts(&config, Some(0.1), false).0,
            (config.droplets as f32 * DAMAGE_MIN_FACTOR).round() as usize
        );
        assert_eq!(
            counts(&config, Some(1000.0), false).0,
            DROPLET_CAP.min((config.droplets as f32 * DAMAGE_MAX_FACTOR).round() as usize)
        );
        // Off means every hit is the same.
        let flat = Config {
            scaling: Scaling::Off,
            ..config
        };
        assert_eq!(
            counts(&flat, Some(1.0), false).0,
            counts(&flat, Some(20.0), false).0
        );
        // Soft is between off and full.
        let soft = Config {
            scaling: Scaling::Soft,
            ..config
        };
        assert!(counts(&soft, Some(20.0), false).0 < heavy);
        assert!(counts(&soft, Some(20.0), false).0 > normal);
    }

    #[test]
    fn the_killing_blow_throws_three_times_as_much() {
        let config = Config::DEFAULT;
        for damage in [None, Some(2.0), Some(REFERENCE_DAMAGE)] {
            // Three times the drops of the same blow, rounded once, at the end.
            let factor = config.scaling.factor(damage);
            let drops = (config.droplets as f32 * factor * 3.0).round() as usize;
            let mist = (config.mist as f32 * factor * 3.0).round() as usize;
            let death = counts(&config, damage, true);
            assert_eq!(death.0, drops.min(DROPLET_CAP), "{damage:?}");
            assert_eq!(death.1, mist.min(MIST_CAP), "{damage:?}");
            // A blow that killed throws far more than the same blow that did not.
            let hit = counts(&config, damage, false);
            assert!(
                death.0 >= hit.0 * 3 - 1 && death.1 >= hit.1 * 3 - 1,
                "{damage:?}"
            );
        }
        // Whatever the player set, and whatever the ceilings are.
        let big = Config {
            death: 5.0,
            droplets: 26,
            ..config
        };
        assert_eq!(counts(&big, Some(20.0), true).0, DROPLET_CAP);
    }

    #[test]
    fn a_burst_is_droplets_mist_and_clouds_around_the_wound() {
        reset();
        let bursts = hit(1, 20.0, REFERENCE_DAMAGE, 10);
        assert_eq!(bursts.len(), 1);
        let burst = &bursts[0];
        let wound = [0.0, 64.0 + 1.95 * 0.62, 0.0];
        assert_eq!(burst.of(DropKind::Droplet).count(), 6); // 8 minus the highlights
        assert_eq!(burst.of(DropKind::Highlight).count(), 2);
        assert_eq!(burst.of(DropKind::Mist).count(), Config::DEFAULT.mist);
        assert_eq!(burst.of(DropKind::Cloud).count(), Config::DEFAULT.clouds);
        assert_eq!(burst.damage, Some(REFERENCE_DAMAGE));
        assert!(!burst.death);
        for drop in &burst.drops {
            for (i, at) in wound.iter().enumerate() {
                assert!(drop.position[i].is_finite() && drop.velocity[i].is_finite());
                assert!((drop.position[i] - at).abs() < 0.4, "{drop:?}");
            }
        }
    }

    #[test]
    fn droplets_leave_the_far_side_and_go_up() {
        reset();
        let bursts = hit(1, 20.0, REFERENCE_DAMAGE, 10);
        let droplets: Vec<&Drop> = bursts[0]
            .drops
            .iter()
            .filter(|d| matches!(d.kind, DropKind::Droplet | DropKind::Highlight))
            .collect();
        // The player stands at x=2, the mob at x=0: blood flies towards -x.
        for drop in &droplets {
            assert!(drop.velocity[0] < 0.0, "{drop:?}");
            assert!(drop.velocity[1] > 0.0, "{drop:?}");
        }
        // Mist and clouds are the slow ones.
        for drop in bursts[0].drops.iter().filter(|d| d.kind == DropKind::Mist) {
            let speed =
                (drop.velocity[0].powi(2) + drop.velocity[1].powi(2) + drop.velocity[2].powi(2))
                    .sqrt();
            assert!(speed < 0.15, "mist drifts: {drop:?}");
        }
    }

    #[test]
    fn a_wound_waits_for_the_health_and_uses_it() {
        reset();
        let now = 5;
        look(sighting(7, 0, 0, 20.0), [2.0, 64.0, 0.0], now - 1);
        look(sighting(7, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], now);
        assert_eq!(waiting(), 1);
        // The health has not come yet: nothing is drawn.
        assert!(resolve(&[(7, 20.0)], now).is_empty());
        assert_eq!(waiting(), 1);
        // Now it has: one burst, sized by the twelve points that came off.
        let bursts = resolve(&[(7, 8.0)], now + 1);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].damage, Some(12.0));
        assert_eq!(waiting(), 0);
        assert_eq!(
            bursts[0].of(DropKind::Droplet).count() + bursts[0].of(DropKind::Highlight).count(),
            counts(&Config::DEFAULT, Some(12.0), false).0
        );
    }

    #[test]
    fn a_hit_that_took_nothing_is_still_drawn() {
        reset();
        let now = 20;
        look(sighting(8, 0, 0, 20.0), [2.0, 64.0, 0.0], now - 1);
        look(sighting(8, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], now);
        for tick in 0..WOUND_WAIT {
            assert!(resolve(&[(8, 20.0)], now + tick).is_empty());
        }
        let bursts = resolve(&[(8, 20.0)], now + WOUND_WAIT);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].damage, None);
        assert_eq!(
            bursts[0].of(DropKind::Droplet).count() + bursts[0].of(DropKind::Highlight).count(),
            Config::DEFAULT.droplets
        );
    }

    #[test]
    fn the_blow_that_kills_throws_three_times_as_much_at_once() {
        reset();
        let now = 30;
        look(sighting(9, 0, 0, 4.0), [2.0, 64.0, 0.0], now - 1);
        // The death animation starts: no flash needed, and no wait.
        look(sighting(9, 0, 1, 0.0), [2.0, 64.0, 0.0], now);
        let bursts = resolve(&[(9, 0.0)], now);
        assert_eq!(bursts.len(), 1);
        assert!(bursts[0].death);
        let thrown =
            bursts[0].of(DropKind::Droplet).count() + bursts[0].of(DropKind::Highlight).count();
        assert_eq!(
            thrown,
            (Config::DEFAULT.droplets as f32 * 3.0).round() as usize
        );
        // A death already seen is not drawn again.
        look(sighting(9, 0, 5, 0.0), [2.0, 64.0, 0.0], now + 1);
        assert!(resolve(&[(9, 0.0)], now + 1).is_empty());
    }

    #[test]
    fn one_blow_bleeds_once_even_if_it_is_heard_twice() {
        reset();
        let now = 40;
        look(sighting(10, 0, 0, 20.0), [2.0, 64.0, 0.0], now - 1);
        look(sighting(10, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], now);
        // The animation for the same blow, a tick later.
        look(sighting(10, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], now + 1);
        assert_eq!(waiting(), 1);
        let bursts = resolve(&[(10, 14.0)], now + 1);
        assert_eq!(bursts.len(), 1);
    }

    #[test]
    fn the_settings_survive_a_round_trip_and_damage_cannot_break_them() {
        let config = Config {
            droplets: 20,
            mist: 4,
            clouds: 3,
            scaling: Scaling::Soft,
            death: 4.0,
            spread: 1.6,
            arc: 0.3,
            colours: false,
            crumbs: false,
            reach: 64.0,
            wounds: 12,
            particles: 256,
        };
        assert_eq!(Config::decode(&config.encode()), config);
        // Garbage, missing fields and impossible numbers.
        let broken = Config::decode("droplets=9999;mist=x;scaling=nonsense;reach=-3;");
        assert_eq!(broken.droplets, *DROPLET_STEPS.last().unwrap());
        assert_eq!(broken.mist, Config::DEFAULT.mist);
        assert_eq!(broken.scaling, Config::DEFAULT.scaling);
        assert_eq!(broken.reach, REACH_STEPS[0]);
        assert_eq!(Config::decode(""), Config::DEFAULT);
    }

    #[test]
    fn every_button_of_the_page_changes_something_and_the_last_resets() {
        let mut config = Config::DEFAULT;
        let count = config.labels().len();
        assert_eq!(count, 13, "twelve settings and a reset button");
        for button in 0..count - 1 {
            let before = config;
            let label = config.cycle(button);
            assert!(!label.is_empty(), "button {button} says nothing");
            let name = |text: &str| text.split(':').next().unwrap_or(text).to_string();
            assert_eq!(
                name(&label),
                name(&before.labels()[button]),
                "button {button} shows its setting: {label}"
            );
            assert_ne!(config, before, "button {button} does nothing");
            // Every setting wraps back to the value it started on.
            let mut again = config;
            for _ in 0..64 {
                if again == before {
                    break;
                }
                again.cycle(button);
            }
            assert_eq!(again, before, "button {button} does not come back");
        }
        // The last button is the way out of a configuration gone wrong.
        assert_eq!(config.cycle(count - 1), "everything back to its defaults");
        assert_eq!(config, Config::DEFAULT);
    }

    #[test]
    fn the_command_writes_every_setting_the_page_has() {
        let mut config = Config::DEFAULT;
        assert_eq!(config.set("droplets", "20"), Ok(()));
        assert_eq!(config.droplets, 20);
        assert_eq!(config.set("scaling", "off"), Ok(()));
        assert_eq!(config.scaling, Scaling::Off);
        assert_eq!(config.set("death", "2.0"), Ok(()));
        assert_eq!(config.death, 2.0);
        assert_eq!(config.set("colours", "off"), Ok(()));
        assert!(!config.colours);
        assert_eq!(config.set("reach", "48"), Ok(()));
        assert_eq!(config.reach, 40.0);
        // Out of range snaps to the nearest value the page can show, and a
        // setting the mod does not have is refused, not ignored.
        assert_eq!(config.set("droplets", "1000"), Ok(()));
        assert_eq!(config.droplets, *DROPLET_STEPS.last().unwrap());
        assert!(config.set("wobble", "3").is_err());
        assert!(config.set("droplets", "many").is_err());
        assert!(config.set("scaling", "sideways").is_err());
        assert!(!config.describe().is_empty());
        // The page and the command agree on the names of the settings.
        for (index, key) in KEYS.iter().enumerate() {
            let mut cycled = Config::DEFAULT;
            cycled.cycle(index);
            assert!(cycled.encode().contains(key), "the file writes {key}");
            let mut named = Config::DEFAULT;
            assert!(
                named.set(key, "2").is_ok()
                    || named.set(key, "on").is_ok()
                    || named.set(key, "full").is_ok(),
                "the command knows {key}"
            );
        }
    }

    #[test]
    fn the_switch_stops_everything() {
        reset();
        set_on(false);
        look(sighting(11, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], 50);
        assert_eq!(waiting(), 0);
        set_on(true);
        look(sighting(11, 0, 0, 20.0), [2.0, 64.0, 0.0], 51);
        look(sighting(11, HURT_FLASH, 0, 20.0), [2.0, 64.0, 0.0], 52);
        assert_eq!(waiting(), 1);
    }

    #[test]
    fn a_hit_out_of_reach_is_not_drawn() {
        reset();
        let far = [Config::DEFAULT.reach + 1.0, 64.0, 0.0];
        look(sighting(12, 0, 0, 20.0), far, 60);
        look(sighting(12, HURT_FLASH, 0, 20.0), far, 61);
        assert_eq!(waiting(), 0);
    }

    #[test]
    fn a_crowd_cannot_grow_the_tables_without_end() {
        reset();
        for id in 0..(REMEMBERED_MAX as u32 + 50) {
            look(sighting(id, 0, 0, 20.0), [1.0, 64.0, 0.0], 1);
        }
        assert!(remembered() <= REMEMBERED_MAX);
        // Nor can a long fight queue up wounds for ever.
        for id in 0..(PENDING_MAX as u32 * 3) {
            look(sighting(id, 0, 0, 20.0), [1.0, 64.0, 0.0], 1);
            look(
                sighting(id, HURT_FLASH, 0, 20.0),
                [1.0, 64.0, 0.0],
                100 + id,
            );
        }
        assert!(waiting() <= PENDING_MAX, "{}", waiting());
    }

    #[test]
    fn a_burst_is_the_same_burst_every_time() {
        reset();
        let a = hit(13, 20.0, REFERENCE_DAMAGE, 70);
        reset();
        let b = hit(13, 20.0, REFERENCE_DAMAGE, 70);
        assert_eq!(a, b);
        assert_ne!(
            burst_seed(7, 100, HURT_FLASH),
            burst_seed(7, 101, HURT_FLASH)
        );
        assert_ne!(
            burst_seed(7, 100, HURT_FLASH),
            burst_seed(8, 100, HURT_FLASH)
        );
    }

    #[test]
    fn the_clock_only_goes_up() {
        let a = begin_tick();
        let b = begin_tick();
        assert!(b > a);
    }

    #[test]
    fn the_generator_stays_inside_its_bounds() {
        let mut rng = Rng::new(1);
        let mut low = f64::MAX;
        let mut high = f64::MIN;
        for _ in 0..10_000 {
            let u = rng.unit();
            assert!((0.0..1.0).contains(&u), "{u}");
            let r = rng.range(-2.0, 3.0);
            low = low.min(r);
            high = high.max(r);
            let d = rng.direction();
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            assert!((length - 1.0).abs() < 1.0e-9, "{d:?}");
        }
        assert!(low < -1.9 && high > 2.9, "{low} {high}");
        assert_ne!(Rng::new(0).next_u64(), 0);
    }
}
