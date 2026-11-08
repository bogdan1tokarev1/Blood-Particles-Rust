// The mod's commands in the chat box: its names are listed, completed and
// coloured as the game's own, and `native_command` runs them.
fn blood_particles_commands(commands: &mut Vec<&'static str>) {
    for name in ["bloodparticles", "bp"] {
        if !commands.contains(&name) {
            commands.push(name);
        }
    }
}
