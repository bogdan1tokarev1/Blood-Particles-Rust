// `/bloodparticles` shows and changes the mod's settings from chat:
//
//   /bloodparticles                     what the settings are now
//   /bloodparticles help                the settings and their values
//   /bloodparticles droplets 12         one setting, by name
//   /bloodparticles reset               everything back to its defaults
//
// It writes what it changes to the same file the settings page writes, so the
// two always agree.
fn blood_particles_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use horizon_client_lib::blood_particles as m;
    let Some(args) = ["bloodparticles", "bp"]
        .into_iter()
        .find_map(|name| command.strip_prefix(name))
    else {
        return;
    };
    // `/bloodparticles` and `/bp droplets 12`, never `/bpx`.
    if !(args.is_empty() || args.starts_with(' ')) {
        return;
    }
    *ran = true;
    let say = |game: &mut crate::game::Game<'_>, line: &str| {
        game.native_chat(&format!("{} {}: {}", m::NAME, m::VERSION, line));
    };
    if !game.native_on(m::ID) {
        say(game, "off, in Options > Mods.");
        return;
    }
    if !m::loaded() {
        m::load(game.native_setting(m::ID, "config").as_deref());
    }
    let args = args.trim();
    if args.is_empty() {
        for line in m::config().describe() {
            say(game, &line);
        }
        say(game, "/bloodparticles help lists the settings.");
        return;
    }
    if args == "help" {
        say(game, m::HELP);
        return;
    }
    if args == "reset" {
        m::set_config(m::Config::DEFAULT);
        game.native_set_setting(m::ID, "config", &m::config().encode());
        say(game, "everything back to its defaults.");
        return;
    }
    let mut words = args.split_whitespace();
    let (Some(key), Some(value)) = (words.next(), words.next()) else {
        say(game, "/bloodparticles <setting> <value>, or help.");
        return;
    };
    let mut config = m::config();
    match config.set(key, value) {
        Ok(()) => {
            m::set_config(config);
            game.native_set_setting(m::ID, "config", &config.encode());
            // The page in Options > Mods shows the change when it is next
            // opened; the labels are asked again then.
            let index = m::KEYS.iter().position(|name| *name == key);
            let label = index
                .and_then(|index| m::config().labels().get(index).cloned())
                .unwrap_or_else(|| format!("{key}: {value}"));
            say(game, &label);
        }
        Err(why) => say(game, &format!("{why}. Try /bloodparticles help.")),
    }
}
