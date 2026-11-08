// The mod's page in Options > Mods: one button for each of its settings, and
// the last one to put them all back. The game asks for them every time the
// page opens and after every click, so the button always shows what the mod is
// set to now.
//
// The game asks every native mod for its page, so the mod answers only for its
// own id.
fn blood_particles_settings(
    game: &mut crate::game::Game<'_>,
    mod_id: &str,
    buttons: &mut Vec<String>,
) {
    use horizon_client_lib::blood_particles as m;
    if mod_id != m::ID {
        return;
    }
    // Options can be opened before any world is drawn, so the file may not have
    // been read yet.
    if !m::loaded() {
        m::load(game.native_setting(m::ID, "config").as_deref());
    }
    buttons.extend(m::config().labels());
}
