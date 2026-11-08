// The mod's row in Options > Mods: its name, its version and the switch.
fn blood_particles_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use horizon_client_lib::blood_particles as m;
    mods.push(crate::game::native_hooks::NativeMod {
        id: m::ID.into(),
        title: format!("{} {}", m::NAME, m::VERSION),
        description: format!("{} By {}.", m::DESCRIPTION, m::AUTHOR),
        keys: Vec::new(),
    });
}
