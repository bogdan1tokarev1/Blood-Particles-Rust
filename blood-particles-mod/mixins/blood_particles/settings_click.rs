// A click on one of the mod's buttons: its setting moves to its next value (or
// everything goes back to its defaults), and the new configuration is written
// to the file the game keeps for the mod, so it survives a restart.
//
// It is written on every click, not when the screen closes: a player who quits
// the game from Options keeps the settings they just chose.
fn blood_particles_settings_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use horizon_client_lib::blood_particles as m;
    if mod_id != m::ID {
        return;
    }
    if !m::loaded() {
        m::load(game.native_setting(m::ID, "config").as_deref());
    }
    m::cycle(button);
    game.native_set_setting(m::ID, "config", &m::config().encode());
}
