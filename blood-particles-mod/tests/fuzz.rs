//! Adversarial soak for the mod's logic: the same entry points the game's hooks
//! call every tick, fed with the worst numbers a client can be handed.
//!
//! These tests exist because a panic inside `crate::blood_particles` is not a
//! caught error in a log — it is the window's game dying mid-frame, and a
//! non-finite coordinate is a stain painted a million blocks away. So every test
//! here drives the real API through hostile input and asserts what the game
//! would otherwise have to survive on its own: nothing panics, nothing but
//! finite, in-world numbers ever leaves the mod, and no table grows past the
//! ceiling the README promises.
//!
//! The same file is what to run under Miri, which is the only way to know a
//! `RefCell` is never double-borrowed on a path the game can take:
//!
//! ```text
//! cargo +nightly miri test            # the whole crate, UB-checked
//! ```

use std::sync::Arc;

use horizon_mod_blood_particles as m;

/// How much to run: Miri interprets every instruction, so a soak that takes a
/// second natively takes it an hour there. The light count still covers every
/// branch — it is the volume that needs the heavy one.
fn rounds(light: u64, heavy: u64) -> u64 {
    if cfg!(miri) { light } else { heavy }
}

/// A generator of its own, so the soak does not lean on the code under test.
struct Dice(u64);

impl Dice {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 { 0 } else { self.next() % bound }
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }

    fn sign(&mut self) -> f64 {
        if self.next() & 1 == 1 { 1.0 } else { -1.0 }
    }

    /// A coordinate. One in eight of them is not a number the game can put a
    /// mob at, on purpose.
    fn coordinate(&mut self) -> f64 {
        match self.below(8) {
            0 => f64::NAN,
            1 => f64::INFINITY,
            2 => f64::NEG_INFINITY,
            3 => f64::MAX,
            4 => 0.0,
            5 => 1.0e300 * self.sign(),
            _ => -3.0e7 + (self.next() % 60_000_000) as f64 / 100.0,
        }
    }

    /// An `i32` of synched data: both ends, the values a countdown is ever seen
    /// at, and nonsense between them.
    fn counter(&mut self) -> i32 {
        match self.below(6) {
            0 => i32::MIN,
            1 => i32::MAX,
            2 => -1,
            3 => 0,
            _ => self.next() as i32,
        }
    }

    fn health(&mut self) -> f32 {
        match self.below(6) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => -1.0e30,
            4 => f32::MAX,
            _ => (self.next() % 400) as f32 / (self.below(300) + 1) as f32,
        }
    }

    fn pick<'a>(&mut self, pool: &'a [&'a str]) -> &'a str {
        pool[self.below(pool.len() as u64) as usize]
    }
}

/// Mob types, including ones the game has never heard of and text no id is.
const KINDS: &[&str] = &[
    "minecraft:zombie",
    "zombie",
    "minecraft:slime",
    "minecraft:blaze",
    "minecraft:enderman",
    "minecraft:skeleton",
    "minecraft:bogged",
    "minecraft:chicken",
    "minecraft:wither",
    "minecraft:ender_dragon",
    "",
    ":",
    "minecraft:",
    "Моби_Тут",
    "\u{1}\u{2}\u{3}",
    "\u{0}",
    "a]a{a[a;a:a.a,a/a\\a",
    "no-such-mob-but-that-is-fine",
];

/// Names and values a player can type, typed wrong.
const WORDS: &[&str] = &[
    "droplets",
    "mist",
    "clouds",
    "scaling",
    "death",
    "spread",
    "arc",
    "colours",
    "colors",
    "crumbs",
    "reach",
    "wounds",
    "particles",
    "",
    " ",
    "DROP",
    "the answer",
    "\u{0}",
    "key\nwith\nnewlines",
    "4",
    "0",
    "-1",
    "8",
    "12",
    "9999999999999999999999",
    "-9999999999999999999999",
    "1e400",
    "-1e400",
    "NaN",
    "inf",
    "-inf",
    "infinity",
    "on",
    "off",
    "true",
    "false",
    "yes",
    "no",
    "1",
    "full",
    "soft",
    "splash",
    "jet",
    "normal",
    "low",
    "high",
    "x3",
    "3.0",
    "  7  ",
    "7;8;9",
    "1;2",
    ";",
    "=",
    "a=b",
    "3e-400",
    "\u{1F4A7}",
    "9007199254740993",
    "0.0000001",
    "-0.0",
];

/// A drop's numbers as the game will receive them: finite, in the world, and
/// small enough to be a droplet and not a teleport.
fn assert_sane(bursts: &[m::Burst], config: &m::Config, where_: &str) {
    // The whole tick's ask, which is what the game's own queue limit is
    // measured against.
    let asked: usize = bursts.iter().map(|burst| burst.drops.len()).sum();
    assert!(
        asked <= config.particles,
        "{where_}: {asked} particles were asked for on a page that allows {}",
        config.particles
    );
    assert!(
        bursts.len() <= config.wounds,
        "{} bursts a tick past a ceiling of {}, {where_}",
        bursts.len(),
        config.wounds
    );
    for burst in bursts {
        assert!(
            burst.drops.len() <= m::DROPLET_CAP + m::MIST_CAP + m::CLOUD_CAP,
            "a burst of {} drops past the caps, {where_}",
            burst.drops.len()
        );
        for drop in &burst.drops {
            for axis in 0..3 {
                let p = drop.position[axis];
                let v = drop.velocity[axis];
                assert!(
                    p.is_finite(),
                    "a position that is not a number: {:?}, {where_}",
                    drop.position
                );
                assert!(
                    v.is_finite(),
                    "a velocity that is not a number: {:?}, {where_}",
                    drop.velocity
                );
                assert!(p.abs() <= 4.0e7, "a drop painted at {p}, {where_}");
                assert!(v.abs() < 1.0e3, "a velocity of {v}, {where_}");
            }
        }
        for kind in [
            m::DropKind::Droplet,
            m::DropKind::Highlight,
            m::DropKind::Mist,
            m::DropKind::Cloud,
        ] {
            let of = burst.of(kind).count();
            let cap = match kind {
                m::DropKind::Cloud => m::CLOUD_CAP,
                m::DropKind::Mist => m::MIST_CAP,
                _ => m::DROPLET_CAP,
            };
            assert!(
                of <= cap,
                "{of} {:?} past a ceiling of {cap}, {where_}",
                kind
            );
        }
        // The colours go to `ParticleOptions` as ratios. The engine would clamp
        // them; the mod must not need that, and must not hand it a NaN.
        for colour in [
            burst.blood.mist(),
            burst.blood.mist_to(),
            burst.blood.cloud(),
        ] {
            for channel in colour {
                assert!(
                    (0.0..=1.0).contains(&channel),
                    "a colour channel of {channel}, {where_}"
                );
            }
        }
        for bright in [false, true] {
            for channel in burst.blood.crumb(bright) {
                assert!(
                    (0.0..=1.0).contains(&channel),
                    "a crumb channel of {channel}, {where_}"
                );
            }
            assert!(
                !burst.blood.block(bright).is_empty(),
                "a block name the game cannot look up"
            );
        }
        for scale in [m::mist_scale(), m::cloud_scale()] {
            assert!(
                (0.01..=4.0).contains(&scale),
                "a dust scale of {scale}, {where_}"
            );
        }
    }
}

/// A configuration that the page could be showing: every value on its ladder,
/// and every label the game will draw.
fn assert_ladder(config: &m::Config, where_: &str) {
    assert!(
        m::DROPLET_STEPS.contains(&config.droplets),
        "droplets {}, {where_}",
        config.droplets
    );
    assert!(
        m::MIST_STEPS.contains(&config.mist),
        "mist {}, {where_}",
        config.mist
    );
    assert!(
        m::CLOUD_STEPS.contains(&config.clouds),
        "clouds {}, {where_}",
        config.clouds
    );
    assert!(
        m::DEATH_STEPS.contains(&config.death),
        "death {}, {where_}",
        config.death
    );
    assert!(
        m::SPREAD_STEPS.contains(&config.spread),
        "spread {}, {where_}",
        config.spread
    );
    assert!(
        m::ARC_STEPS.contains(&config.arc),
        "arc {}, {where_}",
        config.arc
    );
    assert!(
        m::REACH_STEPS.contains(&config.reach),
        "reach {}, {where_}",
        config.reach
    );
    assert!(
        m::WOUND_STEPS.contains(&config.wounds),
        "wounds {}, {where_}",
        config.wounds
    );
    assert!(
        m::PARTICLE_STEPS.contains(&config.particles),
        "particles {}, {where_}",
        config.particles
    );
    assert!(
        config.scaling.label() == "off"
            || config.scaling.label() == "soft"
            || config.scaling.label() == "full",
        "a scaling called {}, {where_}",
        config.scaling.label()
    );
    assert_eq!(
        config.labels().len(),
        13,
        "the page has {} buttons, {where_}",
        config.labels().len()
    );
    for label in config.labels() {
        assert!(!label.is_empty(), "an empty button, {where_}");
        // A number that escaped into a label is a number the page cannot put
        // back: it would not match any ladder when clicked.
        assert!(
            !label.contains("NaN") && !label.contains("inf"),
            "a label of {label}, {where_}"
        );
    }
    assert_eq!(
        config.describe().len(),
        3,
        "the command speaks in three lines, {where_}"
    );
    let text = config.encode();
    assert_eq!(
        &m::Config::decode(&text),
        config,
        "the file cannot read back {text}, {where_}"
    );
}

/// One hit, the way the client actually sees it: a tick of the mob standing
/// unharmed, then the flash going up. A mob seen for the first time never
/// bleeds — there is no last tick to say the flash restarted — so every test
/// that wants a wound has to show the game one.
fn hit_zombie(id: u32, now: u32) {
    let standing = m::Sighting {
        id,
        flash: 0,
        death_time: 0,
        health: 20.0,
        kind: "minecraft:zombie",
        feet: [0.0, 64.0, 0.0],
        height: 1.95,
    };
    m::look(standing, [0.0, 64.0, 0.0], now);
    m::look(
        m::Sighting {
            flash: m::HURT_FLASH,
            ..standing
        },
        [0.0, 64.0, 0.0],
        now,
    );
}

#[test]
fn a_mob_seen_for_the_first_time_does_not_bleed() {
    // ...and a mob seen for the first time with its flash already up is still a
    // hit worth drawing, since the client may hear the packet and the countdown
    // in the same tick it learns the mob exists.
    for flash in [0, 1, m::HURT_FLASH, 40] {
        m::set_on(true);
        m::forget_all();
        m::set_config(m::Config::DEFAULT);
        let now = m::begin_tick();
        m::look(
            m::Sighting {
                id: 900,
                flash,
                death_time: 0,
                health: 20.0,
                kind: "minecraft:zombie",
                feet: [0.0, 64.0, 0.0],
                height: 1.95,
            },
            [0.0, 64.0, 0.0],
            now,
        );
        let drawn = m::resolve(&[(900, 14.0)], now);
        assert_sane(&drawn, &m::config(), "a first sighting");
        assert!(
            drawn.len() <= 1,
            "one mob drawn {} times out of a single look",
            drawn.len()
        );
    }
    m::forget_all();
}

#[test]
fn a_new_world_does_not_inherit_the_old_one() {
    // Entity ids start again from one. A wound of the old world must not be
    // compared with the flash of a new one's mob that happens to share the id.
    m::set_on(true);
    m::forget_all();
    m::set_config(m::Config::DEFAULT);
    let now = m::begin_tick();
    hit_zombie(1, now);
    assert_eq!(m::waiting(), 1, "and the hit was not even queued");
    assert_eq!(m::remembered(), 1, "and the mob was not remembered");
    // What `tick.rs` does when the player is gone, which is every world change.
    m::forget_all();
    assert_eq!(m::remembered(), 0, "a world change left mobs remembered");
    assert_eq!(m::waiting(), 0, "a world change left a wound in the queue");
    // The same id in the new world, hit: the old flash must not suppress it.
    let now = m::begin_tick();
    hit_zombie(1, now);
    let drawn = m::resolve(&[(1, 14.0)], now);
    assert_sane(&drawn, &m::config(), "the same id in a new world");
    assert_eq!(
        drawn.len(),
        1,
        "the new world's mob was swallowed by the old one"
    );
    m::forget_all();
}

#[test]
fn a_world_of_hostile_mobs_never_breaks_the_burst() {
    for seed in 0..rounds(3, 400) {
        let mut dice = Dice::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xB100_D000);
        m::set_on(true);
        m::forget_all();
        let mut config = m::Config::DEFAULT;
        for _ in 0..dice.below(12) {
            let _ = config.cycle(dice.below(14) as usize);
        }
        m::set_config(config);
        assert_ladder(&config, "after cycling");

        let player = [dice.coordinate(), dice.coordinate(), dice.coordinate()];
        for _ in 0..rounds(4, 40) {
            let now = m::begin_tick();
            let mobs = dice.below(if cfg!(miri) { 12 } else { 300 });
            for _ in 0..mobs {
                let ids = if dice.chance(4) { 4 } else { 4096 };
                m::look(
                    m::Sighting {
                        id: dice.below(ids) as u32,
                        flash: dice.counter(),
                        death_time: dice.counter(),
                        health: dice.health(),
                        kind: dice.pick(KINDS),
                        feet: [dice.coordinate(), dice.coordinate(), dice.coordinate()],
                        height: dice.coordinate(),
                    },
                    player,
                    now,
                );
            }
            let health: Vec<(u32, f32)> = (0..mobs)
                .map(|_| (dice.below(4096) as u32, dice.health()))
                .collect();
            let bursts = m::resolve(&health, now);
            assert_sane(&bursts, &config, "in the hostile world");
            assert!(
                m::remembered() <= m::REMEMBERED_MAX,
                "{} mobs remembered",
                m::remembered()
            );
            assert!(
                m::waiting() <= m::PENDING_MAX,
                "{} wounds waiting",
                m::waiting()
            );
        }
        m::forget_all();
        m::set_config(m::Config::DEFAULT);
    }
}

#[test]
fn a_wound_that_never_gets_its_health_is_still_drawn() {
    // `resolve` can be handed a mob that has left the client's list, a health
    // that is not a number, or no list at all. Each has to end up drawn (or
    // dropped) — never stuck in the queue, where it would be redrawn forever.
    m::set_on(true);
    m::forget_all();
    m::set_config(m::Config::DEFAULT);
    let first = m::begin_tick();
    m::look(
        m::Sighting {
            id: 7,
            flash: 0,
            death_time: 0,
            health: 20.0,
            kind: "minecraft:zombie",
            feet: [0.0, 64.0, 0.0],
            height: 1.95,
        },
        [-2.0, 64.0, 0.0],
        first,
    );
    m::look(
        m::Sighting {
            id: 7,
            flash: m::HURT_FLASH,
            death_time: 0,
            health: 20.0,
            kind: "minecraft:zombie",
            feet: [0.0, 64.0, 0.0],
            height: 1.95,
        },
        [-2.0, 64.0, 0.0],
        first,
    );
    assert_eq!(m::waiting(), 1, "the wound did not queue");
    for _ in 0..m::WOUND_WAIT + 3 {
        let now = m::begin_tick();
        let drawn = m::resolve(&[(7, f32::NAN)], now);
        assert_sane(&drawn, &m::config(), "a health that is not a number");
        if !drawn.is_empty() {
            assert_eq!(m::waiting(), 0, "a drawn wound stayed in the queue");
            break;
        }
        assert!(m::waiting() <= 1, "one hit cannot queue twice");
    }
    assert_eq!(m::waiting(), 0, "a wound waited past its own expiry");

    // And a mob that has gone from the list entirely.
    let now = m::begin_tick();
    hit_zombie(7, now);
    let mut drawn_any = false;
    for _ in 0..m::WOUND_WAIT + 2 {
        let now = m::begin_tick();
        let drawn = m::resolve(&[], now);
        assert_sane(&drawn, &m::config(), "no health at all");
        drawn_any |= !drawn.is_empty();
    }
    assert!(drawn_any, "a wound with no mob was never drawn");
    assert_eq!(m::waiting(), 0, "and it is still in the queue");
    m::forget_all();
}

#[test]
fn a_mob_the_server_has_not_positioned_bleeds_nowhere() {
    // The guard in `look`: no number, no wound. Whatever the mix of bad numbers,
    // nothing that leaves the mod may be un-drawable.
    for feet in [
        [f64::NAN, 64.0, 0.0],
        [0.0, f64::INFINITY, 0.0],
        [0.0, 64.0, f64::NEG_INFINITY],
        [f64::MAX, 0.0, 0.0],
        [0.0, 0.0, 4.0e9],
        [-4.0e9, 0.0, 0.0],
    ] {
        for height in [1.95, f64::NAN, f64::INFINITY, -1.0, 1.0e300] {
            for player in [
                [0.0, 0.0, 0.0],
                [f64::NAN, 0.0, 0.0],
                [1.0e308, 0.0, 0.0],
                [0.0, 9.0e9, 0.0],
            ] {
                m::set_on(true);
                m::forget_all();
                let now = m::begin_tick();
                m::look(
                    m::Sighting {
                        id: 1,
                        flash: m::HURT_FLASH,
                        death_time: 0,
                        health: 20.0,
                        kind: "minecraft:zombie",
                        feet,
                        height,
                    },
                    player,
                    now,
                );
                let drawn = m::resolve(&[(1, 14.0)], now);
                assert_sane(
                    &drawn,
                    &m::config(),
                    &format!("feet {feet:?}, height {height}, player {player:?}"),
                );
            }
        }
    }
    m::forget_all();
}

#[test]
fn the_page_cycles_for_ever_without_escaping_its_ladder() {
    m::set_on(true);
    m::set_config(m::Config::DEFAULT);
    let mut config = m::config();
    let mut dice = Dice::new(0xC0FF_EE7C);
    for _ in 0..rounds(150, 60_000) {
        // Past the last button on purpose: the game can ask about a button a
        // mod with a longer page has, and nothing may be indexed out of.
        let button = dice.below(20) as usize;
        let label = config.cycle(button);
        assert!(!label.is_empty(), "button {button} answered with nothing");
        assert_ladder(&config, "after a click");
    }
    m::set_config(m::Config::DEFAULT);
}

#[test]
fn the_command_accepts_anything_a_player_can_type() {
    m::set_on(true);
    m::set_config(m::Config::DEFAULT);
    for seed in 0..rounds(60, 6_000) {
        let mut dice = Dice::new(seed.wrapping_mul(0xD1CE) ^ 0xBEEF);
        let key = dice.pick(WORDS);
        let value = dice.pick(WORDS);
        let mut config = m::config();
        // Either it changes, or it says why not. Never a panic, never a mess.
        let _ = config.set(key, value);
        assert_ladder(&config, "after a typed value");
        m::set_config(config);
        assert_eq!(m::config(), config, "the command did not stick");
    }
    m::set_config(m::Config::DEFAULT);
}

/// Every key against every word the game or a player can hand it: the
/// configuration ends up on the ladder whatever came in, the page and the
/// command still print it, and the burst it paints is still a sane one. The
/// whole safety argument for the caps rests on this: an absurd number must
/// become a step, never a `usize::MAX` of particles.
#[test]
fn every_key_with_every_value_stays_on_the_ladder() {
    m::set_on(true);
    for key in m::KEYS {
        for value in WORDS {
            m::forget_all();
            m::set_config(m::Config::DEFAULT);
            let mut config = m::config();
            let said = config.set(key, value);
            assert_ladder(&config, &format!("{key} = {value:?}"));
            m::set_config(config);
            // What the page and the command show for it, and what the file
            // would hold, must be as well-formed as the numbers themselves.
            let described = config.describe();
            assert!(!described.is_empty(), "{key}: nothing to describe");
            let labels = config.labels();
            assert_eq!(labels.len(), 13, "{key}: the page grew a button");
            let round = m::Config::decode(&config.encode());
            assert_eq!(round, config, "{key} = {value:?}: the file lost it");
            // And the same configuration still paints a burst the game can draw.
            let now = m::begin_tick();
            hit_zombie(7, now);
            let later = m::begin_tick();
            let bursts = m::resolve(&[(7, 4.0)], later);
            assert_sane(&bursts, &config, &format!("{key} = {value:?}"));
            if let Err(why) = said {
                assert!(!why.is_empty(), "{key} = {value:?}: a silent refusal");
            }
        }
    }
    m::set_config(m::Config::DEFAULT);
}

#[test]
fn a_config_file_of_noise_is_still_a_config_file() {
    m::set_on(true);
    for seed in 0..rounds(60, 6_000) {
        let mut dice = Dice::new(seed.wrapping_mul(0x5DE3_CE09) ^ 0xA11C_1E00);
        let mut text = String::new();
        for _ in 0..dice.below(24) {
            match dice.below(6) {
                0 => text.push_str(dice.pick(WORDS)),
                1 => text.push_str(m::KEYS[dice.below(m::KEYS.len() as u64) as usize]),
                2 => text.push('='),
                3 => text.push(';'),
                4 => {
                    text.push_str(dice.pick(WORDS));
                    text.push('=');
                    text.push_str(dice.pick(WORDS));
                }
                _ => {
                    for _ in 0..dice.below(12) {
                        // Raw text, as a hand-edited file can be.
                        let ch = u32::try_from(dice.below(0x11_00_00)).unwrap_or(0x78);
                        text.push(char::from_u32(ch).unwrap_or('x'));
                    }
                }
            }
        }
        let config = m::Config::decode(&text);
        assert_ladder(&config, "from noise");
        m::load(Some(&text));
        assert_eq!(m::config(), config, "the file and the loader disagree");
        // Reading the same file twice must be the same configuration: the game
        // asks again for every world, and options may not drift between them.
        assert_eq!(
            m::Config::decode(&text),
            config,
            "decoding is not a function of the text"
        );
    }
    m::load(None);
    assert_eq!(
        m::config(),
        m::Config::DEFAULT,
        "nothing kept must mean the defaults"
    );
}

#[test]
fn the_same_sighting_paints_the_same_burst() {
    // The burst is planned on one tick and drawn over many, so a second report
    // of the same hit has to look like the first — and a hit that is not a
    // second report must not be swallowed by the first.
    for seed in 0..rounds(40, 400) {
        let mut dice = Dice::new(seed ^ 0x5EED);
        m::set_on(true);
        m::forget_all();
        m::set_config(m::Config::DEFAULT);
        let now = 1234;
        let sighting = m::Sighting {
            id: 3,
            flash: 0,
            death_time: 0,
            health: 20.0,
            kind: dice.pick(KINDS),
            feet: [1.5, 64.0, -2.25],
            height: 1.95,
        };
        let mut wanted: Option<m::Burst> = None;
        for _ in 0..3 {
            m::forget_all();
            m::look(sighting, [0.0, 64.0, 4.0], now);
            m::look(
                m::Sighting {
                    flash: m::HURT_FLASH,
                    ..sighting
                },
                [0.0, 64.0, 4.0],
                now,
            );
            let mut bursts = m::resolve(&[(3, 14.0)], now);
            let burst = bursts.pop().expect("the hit was not drawn");
            assert_sane(
                std::slice::from_ref(&burst),
                &m::config(),
                "in the determinism soak",
            );
            match &wanted {
                None => wanted = Some(burst),
                Some(first) => assert_eq!(*first, burst, "the same hit painted differently"),
            }
        }
    }
    m::forget_all();
}

#[test]
fn the_tick_counter_wrapping_is_not_an_incident() {
    // `u32` ticks wrap after about four years of uptime, and the mod compares
    // tick numbers for both its dedup and its wait. The wrap may neither freeze
    // the mod nor make it repeat a burst.
    m::set_on(true);
    m::forget_all();
    m::set_config(m::Config::DEFAULT);
    let mut drawn = 0;
    let mut dice = Dice::new(0xFF_FF_FF_FF);
    for step in 0..rounds(12, 64) {
        // Four ticks apart, which is past the three a hit is remembered for:
        // each one is a fresh blow, and the point is that the wrap does not
        // make any of them look like the last.
        let now = u32::MAX.wrapping_sub(4).wrapping_add(step as u32 * 4);
        hit_zombie(11, now);
        let health: Vec<(u32, f32)> = if dice.chance(2) {
            vec![]
        } else {
            vec![(11u32, 14.0f32)]
        };
        let bursts = m::resolve(&health, now);
        assert_sane(&bursts, &m::config(), "across the wrap");
        drawn += bursts.len();
    }
    assert!(drawn > 0, "nothing was drawn across the wrap");
    assert!(
        m::remembered() <= 4,
        "the wrap left {} mobs remembered",
        m::remembered()
    );
    assert_eq!(m::waiting(), 0, "the wrap left a wound waiting forever");
    m::forget_all();
}

#[test]
fn four_threads_hammer_it_and_nothing_is_shared() {
    // The state is thread-local, so one thread's mobs may not be in another's
    // table, and no borrow may be open on two of them at once. The game only
    // calls the hooks from the thread that draws the world; this is the same
    // code walked from four.
    let rounds_ = rounds(40, 3_000);
    let handles: Vec<_> = (0..4)
        .map(|worker| {
            std::thread::spawn(move || {
                let mut dice = Dice::new(0xABCD ^ (worker as u64 * 7919));
                m::set_on(true);
                m::forget_all();
                for _ in 0..rounds_ {
                    let now = m::begin_tick();
                    let config = m::config();
                    for _ in 0..dice.below(20) {
                        m::look(
                            m::Sighting {
                                id: dice.below(64) as u32,
                                flash: dice.counter(),
                                death_time: dice.counter(),
                                health: dice.health(),
                                kind: KINDS[dice.below(KINDS.len() as u64) as usize],
                                feet: [dice.coordinate(), dice.coordinate(), dice.coordinate()],
                                height: 1.95,
                            },
                            [0.0, 64.0, 0.0],
                            now,
                        );
                    }
                    let health: Vec<(u32, f32)> = (0..16).map(|i| (i, dice.health())).collect();
                    let bursts = m::resolve(&health, now);
                    assert_sane(&bursts, &config, "on a worker thread");
                    if dice.chance(64) {
                        m::forget_all();
                    }
                    if dice.chance(128) {
                        let _ = m::cycle(dice.below(16) as usize);
                    }
                }
                (m::remembered(), m::waiting())
            })
        })
        .collect();
    let last: Vec<(usize, usize)> = handles
        .into_iter()
        .map(|h| h.join().expect("a thread died"))
        .collect();
    let last = Arc::new(last);
    for (remembered, waiting) in last.iter() {
        assert!(
            *remembered <= m::REMEMBERED_MAX,
            "{remembered} mobs in one thread's table"
        );
        assert!(
            *waiting <= m::PENDING_MAX,
            "{waiting} wounds in one thread's queue"
        );
    }
}

#[test]
fn a_far_storm_is_planned_only_while_it_is_close() {
    // `Drawn within` is the whole reason a crowd two hundred blocks away costs
    // nothing: check it is the reach that stops it, at every value the page has.
    for reach in m::REACH_STEPS {
        let mut config = m::Config::DEFAULT;
        config.reach = reach;
        m::set_on(true);
        m::forget_all();
        m::set_config(config);
        let now = m::begin_tick();
        for distance in [
            1.0,
            reach * 0.5,
            reach * 0.9,
            reach * 1.1,
            reach * 40.0,
            1.0e9,
        ] {
            m::look(
                m::Sighting {
                    id: 0x1000 + distance as u32,
                    flash: m::HURT_FLASH,
                    death_time: 0,
                    health: 20.0,
                    kind: "minecraft:zombie",
                    feet: [distance, 64.0, 0.0],
                    height: 1.95,
                },
                [0.0, 64.0, 0.0],
                now,
            );
        }
        assert!(
            m::remembered() <= 5,
            "reach {reach} kept {} mobs",
            m::remembered()
        );
        let bursts = m::resolve(&[], now);
        assert_sane(&bursts, &config, "at reach {reach}");
        m::forget_all();
    }
    m::set_config(m::Config::DEFAULT);
}

#[test]
fn the_switch_turning_off_mid_fight_leaves_nothing_behind() {
    // Off in Options > Mods has to stop the queue too, not only new wounds: a
    // burst drawn a tick after the switch went off is a burst the player asked
    // to stop seeing.
    m::set_on(true);
    m::forget_all();
    m::set_config(m::Config::DEFAULT);
    let now = m::begin_tick();
    hit_zombie(5, now);
    assert_eq!(m::waiting(), 1, "and the hit was not queued to start with");
    m::set_on(false);
    let now = m::begin_tick();
    hit_zombie(5, now);
    assert_eq!(m::remembered(), 0, "an off mod still remembers mobs");
    assert_eq!(m::waiting(), 0, "an off mod still queues wounds");
    // And what was already queued goes with the switch: `resolve` on an off mod
    // draws nothing and empties the queue, so turning it back on does not paint
    // the blood of every hit taken while it was off.
    let drawn = m::resolve(&[(5, 14.0)], now);
    assert!(drawn.is_empty(), "an off mod drew {} bursts", drawn.len());
    for _ in 0..m::WOUND_WAIT + 2 {
        let now = m::begin_tick();
        assert!(
            m::resolve(&[(5, 14.0)], now).is_empty(),
            "an off mod drew blood"
        );
    }
    assert_eq!(m::waiting(), 0, "an off mod kept queueing");
    m::set_on(true);
    m::forget_all();
}

#[test]
fn a_minute_of_the_worst_fight_costs_nothing_unbounded() {
    // A thousand mobs, every one hit, every tick, for a minute: what has to
    // stay flat is the work of a tick, and this is the case that would show a
    // table growing with a farm.
    m::set_on(true);
    m::forget_all();
    let mut config = m::Config::DEFAULT;
    config.droplets = *m::DROPLET_STEPS.last().unwrap();
    config.mist = *m::MIST_STEPS.last().unwrap();
    config.clouds = *m::CLOUD_STEPS.last().unwrap();
    config.wounds = *m::WOUND_STEPS.last().unwrap();
    config.particles = *m::PARTICLE_STEPS.last().unwrap();
    m::set_config(config);

    let (ticks, mobs) = if cfg!(miri) { (12, 40) } else { (1_200, 1_000) };
    let started = std::time::Instant::now();
    let mut bursts = 0;
    let mut drops = 0;
    for tick in 0..ticks as u32 {
        let now = m::begin_tick();
        for mob in 0..mobs as u32 {
            m::look(
                m::Sighting {
                    id: mob,
                    flash: if tick % 2 == 0 { m::HURT_FLASH } else { 5 },
                    death_time: (mob % 40) as i32,
                    health: 20.0 - (mob % 19) as f32,
                    kind: KINDS[(mob as usize) % KINDS.len()],
                    feet: [mob as f64 * 0.5, 64.0, 0.0],
                    height: 1.95,
                },
                [0.0, 64.0, 0.0],
                now,
            );
        }
        let health: Vec<(u32, f32)> = (0..mobs as u32).map(|mob| (mob, 14.0)).collect();
        let drawn = m::resolve(&health, now);
        assert_sane(&drawn, &config, "in the storm");
        drops += drawn.iter().map(|burst| burst.drops.len()).sum::<usize>();
        bursts += drawn.len();
    }
    let took = started.elapsed();
    println!(
        "{ticks} ticks x {mobs} mobs: {bursts} bursts and {drops} drops in {:.2?} ({:.1} us a tick)",
        took,
        took.as_micros() as f64 / ticks as f64
    );
    assert!(bursts > 0, "a thousand mobs a tick drew nothing");
    assert!(
        m::remembered() <= m::REMEMBERED_MAX,
        "{} remembered",
        m::remembered()
    );
    // Loose so a slow machine does not fail it, tight enough to catch work that
    // grows with the crowd instead of with the ceiling.
    assert!(
        took.as_secs_f64() < 120.0,
        "{bursts} bursts took {took:.1?}, which is too much"
    );
    m::forget_all();
    m::set_config(m::Config::DEFAULT);
}
