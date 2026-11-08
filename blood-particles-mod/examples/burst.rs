// Prints the Blood Particles previews' data as JSON: the drops the mod plans
// for several blows, the physics the game's particle engine gives them, and
// where each one is on each of its first 26 ticks. `tools/preview.py` draws
// this, so the pictures come from the mod's own numbers (runtime.rs), not from
// an artist's idea of a blood spray.
//
// It is a simulation, not a screenshot: the engine's own particle constructor
// draws from the game's random streams, whose state depends on whatever else
// the world is doing, so this tool gives each drop a deterministic stand-in
// for them. Everything else — the counts, the colours, the gravity, the
// friction, the lifetimes, the sizes, the collisions — is the engine's own.
//
//   cargo run --example burst > burst.json
//   python3 tools/preview.py burst.json

use horizon_mod_blood_particles as m;

const GROUND: f64 = 64.0;
const HEIGHT: f64 = 1.95;
const TICKS: usize = 26;
const PLAYER: [f64; 3] = [-2.0, GROUND + 1.62, 0.0];
const FEET: [f64; 3] = [0.0, GROUND, 0.0];
const HEALTH_BEFORE: f32 = 20.0;

/// One blow of the preview sheet.
struct Spec {
    id: u32,
    kind: &'static str,
    title: &'static str,
    caption: &'static str,
    config: m::Config,
    /// The health the server reports after the blow: the damage sizes the burst.
    health: f32,
    death: bool,
}

fn main() {
    let default = m::Config::DEFAULT;
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"version\": {},\n", quote(m::VERSION)));
    out.push_str(&format!("  \"config\": {},\n", quote(&default.encode())));
    out.push_str("  \"settings\": [");
    for (i, label) in default.labels().iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&quote(label));
    }
    out.push_str("],\n  \"keys\": [");
    for (i, key) in m::KEYS.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&quote(key));
    }
    out.push_str("],\n  \"scenes\": [\n");

    let wide = m::Config {
        droplets: 20,
        mist: 12,
        clouds: 4,
        spread: 1.6,
        arc: 1.2,
        ..default
    };
    let no_crumbs = m::Config {
        crumbs: false,
        ..default
    };
    let specs = [
        Spec {
            id: 1,
            kind: "minecraft:zombie",
            title: "A hit of three hearts",
            caption: "The reference blow: the configured 8 droplets, 6 mist, 2 clouds.",
            config: default,
            health: HEALTH_BEFORE - m::REFERENCE_DAMAGE,
            death: false,
        },
        Spec {
            id: 2,
            kind: "minecraft:slime",
            title: "A light hit",
            caption: "Half the damage with full scaling: half as much green spray.",
            config: default,
            health: HEALTH_BEFORE - 3.0,
            death: false,
        },
        Spec {
            id: 3,
            kind: "minecraft:blaze",
            title: "A heavy hit",
            caption: "Twice the damage, twice the fire, until the caps are reached.",
            config: default,
            health: HEALTH_BEFORE - 12.0,
            death: false,
        },
        Spec {
            id: 4,
            kind: "minecraft:enderman",
            title: "The killing blow",
            caption: "Three times as much of everything, thrown wider and slower.",
            config: default,
            health: 0.0,
            death: true,
        },
        Spec {
            id: 5,
            kind: "minecraft:skeleton",
            title: "Block crumbs off",
            caption: "Only mist and clouds: pale chips of bone, no falling droplets.",
            config: no_crumbs,
            health: HEALTH_BEFORE - m::REFERENCE_DAMAGE,
            death: false,
        },
        Spec {
            id: 6,
            kind: "minecraft:zombie",
            title: "A wide, high arc",
            caption: "Droplets 20, mist 12, clouds 4, spray wide, arc high: the page at work.",
            config: wide,
            health: HEALTH_BEFORE - m::REFERENCE_DAMAGE,
            death: false,
        },
    ];
    for (i, spec) in specs.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&scene(spec));
    }
    out.push_str("\n  ]\n}\n");
    print!("{out}");
}

/// Drives the mod's own hook sequence for one blow: two ticks of sightings (a
/// mob standing there, then the hurt flash / death animation), then the health
/// the server sends, then the burst the mod plans.
fn plan(spec: &Spec) -> m::Burst {
    m::forget_all();
    m::set_on(true);
    m::set_config(spec.config);
    let standing = m::Sighting {
        id: spec.id,
        flash: 0,
        death_time: 0,
        health: HEALTH_BEFORE,
        kind: spec.kind,
        feet: FEET,
        height: HEIGHT,
    };
    let first = m::begin_tick();
    m::look(standing, PLAYER, first);
    let hit = m::Sighting {
        flash: m::HURT_FLASH,
        death_time: i32::from(spec.death),
        health: spec.health,
        ..standing
    };
    let now = m::begin_tick();
    m::look(hit, PLAYER, now);
    let mut bursts = m::resolve(&[(spec.id, spec.health)], now);
    bursts
        .pop()
        .unwrap_or_else(|| panic!("the blow of {} made no burst", spec.kind))
}

fn scene(spec: &Spec) -> String {
    let burst = plan(spec);
    let blood = burst.blood;
    let (droplets, mist, clouds) = m::counts(&spec.config, burst.damage, spec.death);
    let mut out = String::from("    {\n");
    out.push_str(&format!("      \"title\": {},\n", quote(spec.title)));
    out.push_str(&format!("      \"kind\": {},\n", quote(spec.kind)));
    out.push_str(&format!("      \"blood\": {},\n", quote(blood.label())));
    out.push_str(&format!("      \"caption\": {},\n", quote(spec.caption)));
    out.push_str(&format!("      \"death\": {},\n", spec.death));
    out.push_str(&format!(
        "      \"damage\": {},\n",
        burst.damage.map_or_else(|| "null".to_string(), number)
    ));
    out.push_str(&format!(
        "      \"counts\": [{droplets}, {mist}, {clouds}],\n"
    ));
    out.push_str(&format!(
        "      \"colours\": {{\"mist\": {}, \"mist_to\": {}, \"cloud\": {}, \"crumb\": {}, \"crumb_bright\": {}}},\n",
        rgb(&blood.mist()),
        rgb(&blood.mist_to()),
        rgb(&blood.cloud()),
        rgb(&blood.crumb(false)),
        rgb(&blood.crumb(true)),
    ));
    out.push_str(&format!(
        "      \"blocks\": [{}, {}],\n",
        quote(blood.block(false)),
        quote(blood.block(true)),
    ));
    out.push_str("      \"drops\": [\n");
    for (i, drop) in burst.drops.iter().enumerate() {
        if i > 0 {
            out.push_str(",\n");
        }
        out.push_str(&drop_json(drop, i, blood));
    }
    out.push_str("\n      ]\n    }");
    out
}

/// One drop, with the engine's constructor and tick physics applied to the
/// mod's own position and velocity.
fn drop_json(drop: &m::Drop, index: usize, blood: m::Blood) -> String {
    let dust = matches!(drop.kind, m::DropKind::Mist | m::DropKind::Cloud);
    let scale = f64::from(if drop.kind == m::DropKind::Cloud {
        m::cloud_scale()
    } else {
        m::mist_scale()
    });
    // A deterministic stand-in for the engine's own random stream: the seed is
    // the drop's place in the burst, so the sheet is the same on every run.
    let mut rng = m::Rng::new(0xB100_D000 ^ (index as u64).wrapping_mul(0x9E37_79B9));
    let velocity = engine_velocity(&mut rng, drop.velocity, dust);
    // `Particle::moving`: size = 0.1 * (r*0.5+0.5) * 2. A block crumb halves
    // it, a dust particle scales it by 0.75 * scale.
    let base = 0.1 * (rng.unit() * 0.5 + 0.5) * 2.0;
    let size = if dust {
        base * 0.75 * scale
    } else {
        base / 2.0
    };
    // A crumb lives 4/(r*0.9+0.1) ticks; a dust particle (8/(r*0.8+0.2))*scale.
    let life = if dust {
        ((8.0 / (rng.unit() * 0.8 + 0.2)) as i32 as f64 * scale).max(1.0)
    } else {
        4.0 / (rng.unit() * 0.9 + 0.1)
    };
    // The engine jitters a dust particle's colour by 0.6..1.3 of the tint and
    // a crumb's by 0.6 of the block's; carry a little of that to the picture.
    let tint = 0.78 + 0.44 * rng.unit();
    let gravity = if dust { 0.0 } else { 1.0 };
    let friction = if dust { 0.96 } else { 0.98 };
    let mut position = drop.position;
    let mut v = velocity;
    let mut on_ground = false;
    let mut path = String::from("[");
    for tick in 0..TICKS {
        if tick > 0 {
            v[1] -= 0.04 * gravity;
            for i in 0..3 {
                position[i] += v[i];
            }
            if position[1] <= GROUND {
                position[1] = GROUND;
                on_ground = true;
                v[1] = 0.0;
            }
            for c in &mut v {
                *c *= friction;
            }
            if on_ground {
                v[0] *= 0.7;
                v[2] *= 0.7;
            }
        }
        if tick > 0 {
            path.push(',');
        }
        path.push_str(&format!(
            "[{},{},{}]",
            number(position[0]),
            number(position[1]),
            number(position[2])
        ));
    }
    path.push(']');
    // The wet colour a droplet starts at, and the darker one it dries to.
    let (from, to) = match drop.kind {
        m::DropKind::Mist => (blood.mist(), blood.mist_to()),
        m::DropKind::Cloud => (blood.cloud(), blood.mist_to()),
        m::DropKind::Droplet => (blood.crumb(false), blood.mist_to()),
        m::DropKind::Highlight => (blood.crumb(true), blood.mist_to()),
    };
    format!(
        "        {{\"kind\": {}, \"life\": {}, \"size\": {}, \"tint\": {}, \"on_ground\": {}, \"from\": {}, \"to\": {}, \"path\": {path}}}",
        quote(kind_name(drop.kind)),
        number(life),
        number(size),
        number(tint),
        on_ground,
        rgb(&from),
        rgb(&to),
    )
}

fn kind_name(kind: m::DropKind) -> &'static str {
    match kind {
        m::DropKind::Droplet => "droplet",
        m::DropKind::Highlight => "highlight",
        m::DropKind::Mist => "mist",
        m::DropKind::Cloud => "cloud",
    }
}

/// `Particle::moving`/`base_moving`: the engine takes the direction of the
/// velocity the mod hands it, adds a little jitter, and gives the particle a
/// speed of its own (then `T::Dust` slows it to a tenth). This is why the
/// droplets arc and the mist drifts rather than flying off in a straight line.
fn engine_velocity(rng: &mut m::Rng, v: [f64; 3], dust: bool) -> [f64; 3] {
    let mut d = [
        v[0] + rng.range(-0.4, 0.4),
        v[1] + rng.range(-0.4, 0.4),
        v[2] + rng.range(-0.4, 0.4),
    ];
    let speed = (rng.unit() + rng.unit() + 1.0) * 0.15;
    let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1.0e-6);
    for c in &mut d {
        *c = *c / length * speed * 0.4;
    }
    d[1] += 0.1;
    if dust {
        for c in &mut d {
            *c *= 0.1;
        }
    }
    d
}

fn number(value: impl std::fmt::Display) -> String {
    // A compact four-decimal number; enough for a 26-pixel-per-block picture.
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn rgb(colour: &[f32; 3]) -> String {
    format!(
        "[{}, {}, {}]",
        number(colour[0]),
        number(colour[1]),
        number(colour[2])
    )
}

fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
