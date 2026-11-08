//! Every particle this mod can ask for, pushed through the game's own particle
//! engine: `add`, then `tick` against a real (empty) world, then the same
//! `render` the window draws with. It is the check that the numbers the mod
//! invents are numbers the game draws, ages, culls and forgets — without a GPU,
//! a window or a server.
//!
//! This file is not part of the mod and is never shipped in the `.hnmod`: the
//! mod's own code is engine-independent so that it can be tested on its own, and
//! this test needs the engine. `tools/engine-soak.sh` stages the mod into a
//! Horizon checkout, drops this file in as `crates/horizon-client/tests/
//! blood_particles_soak.rs` with a `[[test]]` entry for it, and runs it;
//! VERIFICATION.md keeps the output.
extern crate horizon_client_lib as horizon_client;

use horizon_client::{
    blocks::BlockFacts,
    blood_particles as m,
    hud::Rect,
    particles::{ParticleEngine, ParticleLevel, ParticleOptions, ParticleType},
    world::World,
};
use horizon_world::{
    DATA_VERSION,
    block_collision::BlockCollisionRegistry,
    collision::CachedCollisionRegistry,
    nbt::Tag,
    registry::{BlockRegistry, BlockStateId},
};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

/// The options the mod's draw helper hands the engine for one drop: the same
/// three expressions as the loop in `game::blood_particles_tick`, kept here
/// because that loop needs a window for the sprite of a crumb.
fn options(blood: m::Blood, drop: &m::Drop) -> ParticleOptions {
    match drop.kind {
        m::DropKind::Mist => {
            ParticleOptions::dust_transition(blood.mist(), blood.mist_to(), m::mist_scale())
        }
        m::DropKind::Cloud => {
            let colour = blood.cloud();
            let mut cloud = ParticleOptions::colored(
                ParticleType::Dust,
                [colour[0], colour[1], colour[2], 1.0],
            );
            cloud.scale = m::cloud_scale();
            cloud
        }
        // The sprite name is the game's own (`game.particle_sprite` for this
        // block); a redstone block stands in for it, tinted as the mod tints it.
        m::DropKind::Droplet | m::DropKind::Highlight => ParticleOptions::block(
            ParticleType::Block,
            "block/redstone_block".to_owned(),
            0x8b_00_00,
        ),
    }
}

/// One blow, as the client hears it: the hurt flash rising in one packet, the
/// new health in the next. `look` only remembers a mob it has not seen before,
/// so the flash has to be seen twice to count as a hit.
fn hit(id: u32, feet: [f64; 3], eye: [f64; 3], now: u32) {
    let standing = m::Sighting {
        id,
        flash: 0,
        death_time: 0,
        health: 20.0,
        kind: "minecraft:zombie",
        feet,
        height: 1.95,
    };
    m::look(standing, eye, now);
    m::look(
        m::Sighting {
            flash: m::HURT_FLASH,
            ..standing
        },
        eye,
        now,
    );
}

/// The empty world every particle falls through, as the engine's own tests
/// build one: one air state, no chunks, so every block lookup misses.
struct Arena {
    registry: BlockRegistry,
    world: World,
    facts: BlockFacts,
}

impl Arena {
    fn collision(&self) -> BlockCollisionRegistry<'_> {
        let cached = CachedCollisionRegistry::from_nbt(
            &Tag::compound([
                ("DataVersion", Tag::Int(DATA_VERSION)),
                (
                    "states",
                    Tag::List(vec![Tag::String("minecraft:air".into())]),
                ),
                ("shapes", Tag::List(vec![])),
                ("cached_shapes", Tag::IntArray(vec![-1])),
            ]),
            &self.registry,
        )
        .expect("the air-only collision table the engine's own tests use");
        BlockCollisionRegistry::new(Arc::new(cached))
            .expect("the air-only collision table the engine's own tests use")
    }
}

fn arena() -> Arena {
    let registry = BlockRegistry::from_nbt(&Tag::compound([
        ("DataVersion", Tag::Int(DATA_VERSION)),
        (
            "states",
            Tag::List(vec![Tag::String("minecraft:air".into())]),
        ),
        ("defaults", Tag::compound([("minecraft:air", Tag::Int(0))])),
    ]))
    .expect("the air-only block registry the engine's own tests use");
    Arena {
        registry,
        world: World::new(BlockStateId(0), 0),
        facts: BlockFacts {
            states: vec![],
            air: BlockStateId(0),
            woods: HashMap::new(),
        },
    }
}

fn engine(camera: [f64; 3]) -> ParticleEngine {
    let rect = Rect {
        u0: 0.0,
        v0: 0.0,
        u1: 1.0,
        v1: 1.0,
        w: 16,
        h: 16,
    };
    let mut engine = ParticleEngine::new(
        Path::new(""),
        &HashMap::from([("block/redstone_block".to_owned(), rect)]),
        &HashSet::new(),
    );
    engine.camera = camera;
    engine
}

/// A fight: sixty waves of two hundred mobs, every one hit, drawn with the mod's
/// default settings, ticked and rendered by the real engine all the way through.
#[test]
fn the_game_engine_draws_every_burst_the_mod_emits() {
    let arena = arena();
    let collision = arena.collision();
    let eye = [0.0, 101.62, 0.0];
    let mut engine = engine(eye);
    m::load(None);
    m::set_on(true);
    let mut drops = 0;
    for wave in 0..120 {
        // A mob hit again within three ticks of its last burst is the same blow
        // to the client, so a wave leaves the window before the next one starts.
        for _ in 0..3 {
            m::begin_tick();
        }
        let now = m::begin_tick();
        let mut health = Vec::new();
        for id in 0..200u32 {
            let feet = [
                f64::from(id % 9) - 4.0,
                100.0 + f64::from(id / 9 % 5),
                f64::from(id / 45) - 2.0,
            ];
            hit(id, feet, eye, now);
            health.push((id, 14.0));
        }
        let later = m::begin_tick();
        let bursts = m::resolve(&health, later);
        assert!(
            !bursts.is_empty(),
            "wave {wave}: two hundred mobs were hit and nothing was queued",
        );
        for burst in &bursts {
            for drop in &burst.drops {
                engine.add(&options(burst.blood, drop), drop.position, drop.velocity);
                drops += 1;
            }
        }
        let level = ParticleLevel {
            world: &arena.world,
            facts: &arena.facts,
            collision: &collision,
            player: None,
        };
        engine.tick(&level);
        assert!(
            engine.len() <= 16_384,
            "the engine's own cap holds: {} particles",
            engine.len()
        );
        // The quads the window would draw, from the engine's own table.
        let mut vertices = Vec::new();
        engine.render(glam::DVec3::from_array(eye), 0.0, 0.0, 1.0, &|_| 15, &mut vertices);
        assert!(
            vertices.iter().flat_map(|v| v.pos).all(f32::is_finite),
            "wave {wave}: a quad the engine drew off the mod's numbers is not a number"
        );
        assert!(
            !vertices.is_empty(),
            "wave {wave}: the mod's drops reached the render data at all"
        );
    }
    assert!(
        drops > 10_000,
        "the fight only drew {drops} drops in 120 waves",
    );
    // Off means off, in the engine's queue too: the switch drops what was
    // queued, and every drop already drawn ages out on its own.
    m::set_on(false);
    let off = m::begin_tick();
    assert!(
        m::resolve(&health_after_the_fight(), off).is_empty(),
        "a mod that is off flushed {} wounds into the engine",
        m::waiting()
    );
    let level = ParticleLevel {
        world: &arena.world,
        facts: &arena.facts,
        collision: &collision,
        player: None,
    };
    for _ in 0..600 {
        engine.tick(&level);
    }
    assert!(
        engine.is_empty(),
        "{} of the mod's particles outlived their lifetime",
        engine.len()
    );
}

fn health_after_the_fight() -> Vec<(u32, f32)> {
    (0..200).map(|id| (id, 14.0)).collect()
}

/// The mod's `reach` and the engine's 32-block limiter: a wound the player is
/// too far from is never asked for, and one the engine will not draw costs it
/// nothing either. Measured, so a future change to either number shows up here.
#[test]
fn a_wound_too_far_away_costs_the_game_nothing() {
    let arena = arena();
    let collision = arena.collision();
    let eye = [0.0, 100.0, 0.0];
    let mut engine = engine(eye);
    m::load(None);
    m::set_on(true);
    for (id, distance) in [1u32, 2, 4].into_iter().zip([30.0, 45.0, 4000.0]) {
        let now = m::begin_tick();
        hit(id, [distance, 100.0, 0.0], eye, now);
        let later = m::begin_tick();
        for burst in &m::resolve(&[(id, 14.0)], later) {
            for drop in &burst.drops {
                engine.add(&options(burst.blood, drop), drop.position, drop.velocity);
            }
        }
    }
    let level = ParticleLevel {
        world: &arena.world,
        facts: &arena.facts,
        collision: &collision,
        player: None,
    };
    engine.tick(&level);
    // The default reach is 40 blocks, so the 30- and 45-block mobs were both
    // seen; the engine then keeps only what is within 32 blocks of the camera.
    assert!(
        engine.len() < 400,
        "the far wounds reached the queue anyway: {} particles",
        engine.len()
    );
    for _ in 0..2 {
        engine.tick(&level);
    }
    let mut vertices = Vec::new();
    engine.render(glam::DVec3::from_array(eye), 0.0, 0.0, 1.0, &|_| 15, &mut vertices);
    assert!(
        vertices.iter().flat_map(|v| v.pos).all(f32::is_finite),
        "a culled burst still put a non-number on the screen"
    );
}

/// The widest settings the mod offers, against a thousand mobs: the engine's
/// own cap and the mod's own cap both hold, and nothing panics on the way.
#[test]
fn the_biggest_settings_the_page_offers_stay_inside_the_engines_cap() {
    let arena = arena();
    let collision = arena.collision();
    let eye = [0.0, 100.0, 0.0];
    let mut engine = engine(eye);
    m::load(Some(
        "droplets=26;mist=12;clouds=4;scaling=full;death=5;spread=1.6;arc=1.2;colours=true;crumbs=true;reach=64;wounds=16;particles=256",
    ));
    m::set_on(true);
    for round in 0..8 {
        let now = m::begin_tick();
        let mut health = Vec::new();
        for id in 0..1000u32 {
            let feet = [
                f64::from(id % 31) - 15.0,
                100.0 + f64::from(id / 31 % 8),
                f64::from(id % 7) - 3.0,
            ];
            hit(id, feet, eye, now);
            health.push((id, if round % 3 == 0 { 0.0 } else { 2.0 }));
        }
        let later = m::begin_tick();
        let drawn = {
            let mut drawn = 0;
            for burst in &m::resolve(&health, later) {
                for drop in &burst.drops {
                    engine.add(&options(burst.blood, drop), drop.position, drop.velocity);
                    drawn += 1;
                }
            }
            drawn
        };
        assert!(
            drawn <= m::config().particles,
            "round {round}: the mod drew {drawn} on a page that allows {}",
            m::config().particles
        );
        let level = ParticleLevel {
            world: &arena.world,
            facts: &arena.facts,
            collision: &collision,
            player: None,
        };
        engine.tick(&level);
        assert!(
            engine.len() <= 16_384,
            "round {round}: {} particles, past what the engine keeps",
            engine.len()
        );
    }
    m::set_on(false);
    let _ = m::begin_tick();
    m::forget_all();
}
