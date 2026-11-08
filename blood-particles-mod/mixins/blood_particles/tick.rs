// The window's game, once a tick: every mob whose hurt flash has just restarted
// has been hit, every mob whose death animation has just begun has been killed,
// and both bleed. The burst is drawn a tick or two later, once the mob's new
// health has arrived, so its size follows the damage the blow did.
//
// Why the client draws it, and not the server: a client-side mod has to work on
// any server, and the client is told both things — the hurt flash and the
// health (`LivingEntity.DATA_HEALTH_ID` is synched data) — whatever machine the
// world runs on. In single player, on a LAN world and on a server, the same
// code draws the same blood.
fn blood_particles_tick(game: &mut crate::game::Game<'_>) {
    use horizon_client_lib::blood_particles as m;
    // The switch is in Options > Mods, and this runs on the thread that draws
    // the world, so turning the mod off stops the blood on the next tick.
    m::set_on(game.native_on(m::ID));
    // A server's own Game has no window and no camera; only the window's Game
    // draws particles.
    if !game.native_window() {
        return;
    }
    // The player's settings, read from the file the game keeps for the mod the
    // first time a world is drawn.
    if !m::loaded() {
        m::load(game.native_setting(m::ID, "config").as_deref());
    }
    if !m::on() {
        return;
    }
    let now = m::begin_tick();
    let Some(player) = game.native_player() else {
        // No player is no world: the menu, a dimension change, a server that
        // dropped us. Everything remembered is forgotten, because the next world
        // starts its entity ids again from one and a mob id would otherwise be
        // judged against the flash and health of the mob that had it last time.
        m::forget_all();
        return;
    };
    let eye = [player.x, player.y, player.z];
    // What the client knows of each mob: the hit it heard, the death it saw,
    // and the health the server last sent.
    for mob in &game.mobs {
        let sighting = m::Sighting {
            id: mob.id,
            flash: mob.hurt_time,
            death_time: mob.death_time,
            health: mob.health,
            kind: &mob.kind.id,
            feet: [mob.position.x, mob.position.y, mob.position.z],
            height: mob.kind.height,
        };
        m::look(sighting, eye, now);
    }
    let mut health = Vec::new();
    for mob in &game.mobs {
        health.push((mob.id, mob.health));
    }
    let bursts = m::resolve(&health, now);
    if bursts.is_empty() {
        return;
    }
    // The droplets' texture is a block's own particle sprite, resolved the way
    // the game resolves the crumbs of a block it breaks. A resource pack
    // without that block leaves the mist and the cloud alone instead of
    // failing.
    let mut crumbs: Vec<((m::Blood, bool), horizon_client::particles::ParticleOptions)> =
        Vec::new();
    for burst in &bursts {
        for bright in [false, true] {
            if crumbs
                .iter()
                .any(|((blood, br), _)| *blood == burst.blood && *br == bright)
            {
                continue;
            }
            let at = burst
                .drops
                .first()
                .map(|drop| drop.position.map(|v| v.floor() as i32))
                .unwrap_or([0; 3]);
            let Some(sprite) = game
                .pack
                .assets
                .registry
                .default_state(burst.blood.block(bright))
                .and_then(|state| game.particle_sprite(state, at))
            else {
                continue;
            };
            crumbs.push((
                (burst.blood, bright),
                horizon_client::particles::ParticleOptions::block(
                    horizon_client::particles::ParticleType::Block,
                    sprite.0,
                    sprite.1,
                ),
            ));
        }
    }
    let mut drawn = 0;
    let cap = m::config().particles;
    for burst in &bursts {
        let mist = horizon_client::particles::ParticleOptions::dust_transition(
            burst.blood.mist(),
            burst.blood.mist_to(),
            m::mist_scale(),
        );
        let colour = burst.blood.cloud();
        let mut cloud = horizon_client::particles::ParticleOptions::colored(
            horizon_client::particles::ParticleType::Dust,
            [colour[0], colour[1], colour[2], 1.0],
        );
        cloud.scale = m::cloud_scale();
        for drop in &burst.drops {
            if drawn >= cap {
                return;
            }
            let options = match drop.kind {
                m::DropKind::Mist => Some(&mist),
                m::DropKind::Cloud => Some(&cloud),
                m::DropKind::Droplet | m::DropKind::Highlight => {
                    let bright = drop.kind == m::DropKind::Highlight;
                    crumbs
                        .iter()
                        .find(|((blood, br), _)| *blood == burst.blood && *br == bright)
                        .map(|(_, options)| options)
                }
            };
            let Some(options) = options else {
                // No sprite for this kind of blood in this resource pack.
                continue;
            };
            // `add` drops anything past 32 blocks of the camera by itself, so
            // a burst seen from far away costs nothing to draw.
            game.particles.add(options, drop.position, drop.velocity);
            drawn += 1;
        }
    }
}
