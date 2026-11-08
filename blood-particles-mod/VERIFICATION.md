# Blood Particles 1.2.0 — what was checked, and what could not be

Built for **Minecraft Horizon 26.3**, tested on Nightly
`2ebf00a0281c592f79191e76fce0e7d522c6f425` (whose `NATIVE_HOOKS_API` is 411),
horizon-mixins `e8cd0aa38cffef49cb2e088ef716baa107148c02`. Author `Bogdan_Tokarev`. Built and checked with Rust `1.99.1`
(`58cf8f9c3`), edition 2024.

## The one thing that was not done: playing the game

**A real run in the game was not possible where this mod was built**, and this
document does not pretend otherwise. The machine has no Horizon launcher, no
Minecraft 26.3 resources, no GPU and no display, so the game cannot start:
`horizon-client` needs a window, `rend3`/`wgpu` need an adapter, and the world
needs the game's own resource packs. **Nothing that ships with this mod is a
screenshot of the game.** The images are **simulated from the mod's own burst
data** (`examples/burst.rs` → `tools/preview.py`): the counts, positions,
velocities, colours and lifetimes are the runtime's, and the particle engine's
own tick formulas are re-implemented from `particles.rs`, but the drawing — the
shaded droplets, the specular dot, the motion blur, the mist, the stains, the
settings page — is painted here, not rendered by the game. Every image carries
a `SIMULATION` badge for that reason.

What was done instead: the mod was applied to the real Horizon source, and the
game it produces was compiled and tested with all warnings as errors — which is
what verifies that every call the mod makes into the game exists, has the
signature the mod assumes, and is reachable from where the injection lands.

## Verified

| # | Check | Command | Result |
| --- | --- | --- | --- |
| 1 | The mod's own tests | `cargo test` | **35 passed, 0 failed** — 20 of the logic and geometry, 15 adversarial soaks (`tests/fuzz.rs`) |
| 2 | Lints, everything including the example and the soak | `cargo clippy --all-targets -- -D warnings` | **clean** |
| 3 | Formatting | `cargo fmt --check` | **clean** |
| 4 | The bundle applies to the real source | `horizon-mixins apply --workspace <horizon 2ebf00a> --mods mixins --out <stage>` | **`status: ok`, 6 injections, 1 module, 56.7 s** |
| 5 | The same bundle validated against pristine source | `horizon-mixins plan --workspace <horizon> --mods mixins --out plan.json` | **`status: ok`, 6 injections, 1 module, 27.0 s** |
| 6 | The patched game compiles, warnings as errors | `RUSTFLAGS=-D warnings cargo check --locked -p horizon-client -p horizon-client-lib` | **`Finished`, exit 0** (3 m 20 s) |
| 7 | The mod's tests pass inside the patched game | `cargo test --locked -p horizon-client-lib blood_particles` | **20 passed, 0 failed**, 2218 of the game's own tests filtered out, and the game's nine other test targets ran green beside them |
| 8 | The particle engine the mod drives | `cargo test --locked -p horizon-client-render --lib particles` | **33 passed, 0 failed, 1 ignored** — including `terrain_crumb_samples_a_mirrored_quarter` (what the droplets are) and `mod_velocity_adds_the_spawn_velocity_and_jitters_deterministically` |
| 9 | **The mod's numbers through that engine** | `tools/engine-soak.sh <horizon>` | **3 passed, 0 failed**: 120 waves of 200 hit mobs, every drop `add`ed, `tick`ed and `render`ed by the real engine; `len()` never past its 16384; every quad a number; the switch off mid-fight and everything the mod drew aged out (`is_empty()`) |
| 10 | Arithmetic, in a release build with the overflow checks forced on | `RUSTFLAGS="-C overflow-checks=on -C debug-assertions=on" cargo test --release` | **35 passed, 0 failed** |
| 11 | Undefined behaviour, a `RefCell` borrowed twice, a leak, an overflow, a data race — under Miri | `cargo +nightly miri test` (nightly `1.102.0`, `-Zmiri-strict-provenance`) | **35 passed, 0 failed, exit 0**: the 20 logic tests in 57.5 s and the 15 soaks in 15 m 39 s, every one of them interpreted — nothing UB, no table borrowed twice on any path the game can take, nothing leaked at exit, no arithmetic overflow, and the four-thread test saw no race |
| 12 | No panic source outside the tests | reading every shipped file for `unwrap`/`expect`/`panic!`/indexing, and reading the game's own hook dispatch | **none**: every fallible path is `unwrap_or`, `?`, `min`/`max` or a checked slice — and it has to be, because a native mod's helper is an ordinary call in `native_hooks.rs`, not a guarded one (see below) |
| 13 | The package the registry gets | `python3 native/tools/native.py package blood_particles` | **23340 B**, `sha256 8439959e089453a54c7fdcf996d1575e3ca7327a54de26c13975b66bad6e0d06` |
| 14 | The package's contents | `unzip -l` + `unzip -p … horizon-native.json` | 10 files, each byte-identical to the repository's (`diff -r` clean); `game 26.3`, `hooks 411`, `horizon 2ebf00a…`, `mixins e8cd0aa…`, `id blood_particles`, `version 1.2.0`, `schema 1` |
| 15 | The repository archive | `zip -r` of the sources, then `diff -r` against a fresh unzip | **23 files, MATCH** — the archive differs from the working tree by nothing |
| 16 | The burst data is reproducible | `cargo run --example burst` twice, `diff -q` | **identical**: the mod's own generator, seeded from the mob's id, the tick and the flash |

Checks 4 to 9 are the ones that matter. With the mod applied, `runtime.rs`
becomes `horizon_client_lib::blood_particles` — compiled *as part of the game's
own library crate* — and the six helpers are compiled inside
`crates/horizon-client/src/game/native_hooks.rs`. So the compiler proves that
`game.mobs`, `game.particles`, `game.particle_sprite`, `game.pack`,
`game.native_window()`, `game.native_on()`, `game.native_player()`,
`game.native_setting()`, `game.native_set_setting()`, `game.native_chat()`,
`ParticleOptions::dust_transition`, `ParticleOptions::colored`,
`ParticleOptions::block`, `ParticleType::{Block, Dust, DustColorTransition}`,
`registry.default_state` and `ParticleEngine::add` all exist, are visible from
the module the injection lands in, and take exactly what the mod passes.
Check 6 runs the geometry, the settings round-trip and the damage scaling in the
build the player would actually get.

## Where each injection lands

`grep -n blood_particles` in the staged tree, in
`crates/horizon-client/src/game/native_hooks.rs`:

| Line | Hook point | What it does |
| --- | --- | --- |
| 2385 / 2498 | `tick` | once a tick: fresh hurt flashes become bursts |
| 3113 / 3175 | `command` | `/bloodparticles`, `/bp`: show, set, help, reset |
| 3186 / 3193 | `commands` | both names in the chat box's completion |
| 3580 / 3591 | `mods` | its row and switch in Options > Mods |
| 3600 / 3614 | `settings` | the 12 labels + reset of its page |
| 3621 / 3636 | `settings_click` | a click: cycle that setting, write the file |
| `client_lib.rs` 186 | module | `pub mod blood_particles` |

The two settings hooks are the new ones in 1.2.0, and they are the reason the
mod is pinned to a commit rather than only naming hook points: the page's
buttons are collected into a `&mut Vec<String>` and a click comes back as a
`usize`, which is the shape Horizon's `NativePage` has on this Nightly
(`crates/horizon-client/src/game/native_hooks.rs`, `list_native_mods` and
`native_settings_event`, and `options_ui.rs`'s `Act::NativeButton`). The game
also re-asks for the labels after every click and every time the page opens, so
the button text can never go stale — the mod does not push it.

## Why the mod is built the way it is

The client is told about every hit it can see: the server's
`ClientboundDamageEventPacket` (and the hurt animation) restart the mob's hurt
flash — `LivingEntity.hurtTime` — at 10, and the client counts it down one a
tick (`crates/horizon-client/src/game/remote.rs`, `C::DamageEvent` and
`C::HurtAnimation`, both `m.hurt_time = 10`). So a flash that went **up** since
the last tick is a hit, and the mod never needs the server.

That choice is what makes the mod work on **any server**, not only in single
player: no server hook, no packet, no server component.

The flash is read once a tick, and the tick that hears the hit may be the one
that already counted it down, so the test is "went up", not "is exactly 10".

How hard the blow was is not in the flash. The client's mob health *is* synched
to it, so a wound waits up to three ticks (`WOUND_WAIT`) for the mob's new
health to arrive and takes the damage as `health_before − health_after`; when
nothing arrives, the burst is drawn unscaled rather than lost. That is what
`Damage sizes the burst` scales, and what a mob that cannot be seen any more, or
a hit that reports no health change, falls back on.

A death needs no damage at all: `death_time > 0` is synched too, and it is the
one event that multiplies every count by `The killing blow throws` and widens
the spray.

Blood that leaves the far side of the blow uses the attacker's position when the
game gives one; otherwise the spray goes away from the player, which is where a
player's own hit comes from.

## Bounds, and why they are there

- **Ceilings nothing can raise past:** 64 droplets, 48 puffs of mist, 12 clouds
  in one burst (`DROPLET_CAP`, `MIST_CAP`, `CLOUD_CAP`), whatever the page says.
- **Per tick:** the page's own `Wounds a tick` (2 to 16, default 8) and
  `Particles a tick` (32 to 256, default 112). `resolve` trims its own answer to
  both, so the limit is the mod's and not the draw loop's: no caller can ask the
  game for more than the page allows, and check 9 measures it. The engine has its
  own `MAX_PARTICLES` (16384) too; the mod stays under the player's setting, not
  under the engine's.
- **`Drawn within`:** 16 to 64 blocks, default 40. The game's own engine already
  drops a particle spawned beyond 32 blocks of the camera, so this only saves
  the work of planning it.
- **One burst a hit, three ticks apart the least** (`bled_recently`,
  `now − last < 3`): a damage packet and a hurt animation for the same blow make
  one burst. It is a *dedup*, not a cooldown to be tuned: a mob hit twice in one
  tick bleeds once, a mob hit again one or two ticks later is still the same
  blow, and only three ticks after it is a second wound.
- **4096 mobs remembered** (`REMEMBERED_MAX`), **64 wounds waiting**
  (`PENDING_MAX`): neither table can grow with a farm.
- The damage factor is clamped to **0.35 – 2.5** however light or heavy the hit
  was, so no single number in a modpack's damage table can turn a punch into a
  fountain or a mace hit into nothing.
- The mod's random numbers come from **its own generator** (`Rng`, xorshift64\*),
  never from the game's, whose order the rest of the game depends on.

## Why nothing in it may panic

A native mod's helper is compiled *into* the game: the six injections are plain
function calls in `crates/horizon-client/src/game/native_hooks.rs`, and that
dispatch has no `catch_unwind`. The guard the engine does have
(`crates/horizon-client/src/mod_api.rs`) wraps the **WebAssembly** mods' hooks, and
its own test — `a_panic_in_a_hook_names_the_mod_and_the_hook` — is explicit that
even there the panic "goes on to the dispatch's caller": what the game adds is the
name of the mod and the hook in the report, not survival. So a panic in this mod
would be the window's game dying mid-frame, with `crash.rs` writing a
`crash-reports/crash-<time>-client.txt`, and no switch in the options page would
ever be reached again.

That is the reason the mod is written total. Nothing outside `#[cfg(test)]`
unwraps, expects, indexes a computed position, divides by a number it did not
check, or converts a float it did not bound; every fallible step ends in
`unwrap_or`, a `?` into a message, a `min`/`max`, or a slice the compiler knows
the length of. Checks 1, 10, 11 and 12 are the four ways that claim was tested:
by reading, by running the arithmetic where the optimiser cannot hide an overflow,
by running it under an interpreter that rejects undefined behaviour, and by
feeding it every number a server, a resource pack, a hand-edited config file or a
typed command can hand it.

## What the fuzz suite and the soak hold down

`tests/fuzz.rs` drives the same entry points the hooks call every tick with the
numbers a client can actually be handed: mob positions that are `NaN`, `±inf`,
or a coordinate past the build limit; healths that are not numbers; 18 kinds of
mob-name string including the empty one, a `\0` and raw UTF-8; 59 key and value
strings for the settings and the command, including `1e400`, `99999999999999999999`,
`-0.0`, `NaN`, `💧` and `١٢٣`; a config file of pure noise; a tick counter that
wraps past `u32::MAX`; four threads hammering the module at once; and a minute of
the worst fight the settings allow (60 000 cycles of a hostile world). Each one
asserts the same things the game would otherwise have to survive on its own:

- **nothing panics**, and nothing but a finite coordinate inside the world ever
  leaves the mod — a drop's position, velocity and colour are checked against the
  game's limits, not against a picture of what the mod means to do;
- **every burst fits**: at most `Wounds` of them, at most `Particles` of their
  drops in total, never more than 64 / 48 / 12 of a kind, a dust scale inside the
  `0.01..=4.0` the engine clamps to;
- **every setting lands on its ladder** — an absurd number becomes a step of the
  page, never a `usize::MAX`, and the file round-trips what the page shows;
- **the tables stay bounded** (`REMEMBERED_MAX`, `PENDING_MAX`), including when
  a world never ends and when a server never sends the health;
- **the same input paints the same burst**, so a re-applied mod and a reloaded
  world cannot stutter.

Three rules came out of writing it, and are now asserted rather than assumed:

1. A position the world cannot hold is dropped where it is read (`drawable`),
   because the test the mod used to have — `distance > reach` — is false for a
   `NaN` and filtered nothing.
2. **Off means off, including the queue.** A wound that was waiting when the
   player turned the mod off is forgotten instead of being drawn the tick it is
   turned back on: up to 64 wounds of blood for hits taken while the mod was off
   would otherwise arrive at once.
3. **A new world forgets the old one.** `forget_all` clears the dedup table too,
   and the tick helper calls it whenever there is no player. Entity ids start
   again at one, so a zombie in world two would otherwise be judged against the
   flash and the health of whatever mob had that id in world one.

`tools/engine_soak.rs` (check 9, run by `tools/engine-soak.sh`) is the other half:
it takes that same output into the game's real `ParticleEngine` — `add`, `tick`
against an empty but real `World`, and the `render` the window draws with — and
measures that the engine accepts it, culls what is too far, keeps its own count
and forgets all of it. It needs the engine, so it lives outside the shipped
package and is not compiled on its own.

## What a live run would have to confirm

1. A hit on a mob shows the burst at the wound, spraying away from the player.
2. The droplets fall, hit the ground and leave stains; the mist fades darker.
3. A slime, a blaze, an enderman and a skeleton show green, fire, violet and
   bone-chip blood.
4. A light hit throws less than a heavy one, and the killing blow throws three
   times as much; `Damage sizes the burst: off` makes them equal again.
5. **Options > Mods > Blood Particles** shows 13 buttons; clicking one changes
   its value in its own label at once; the last one puts everything back.
6. A setting chosen in the page is still there after quitting and starting the
   game, and `/bloodparticles` reports the same value.
7. `/bloodparticles droplets 12` changes it, `/bloodparticles help` lists all
   twelve, `/bloodparticles reset` clears them, `/bp` does the same as
   `/bloodparticles`, and both names complete in the chat box.
8. **Options > Mods** off stops it on the next tick; on brings it back.
9. On *Minimal* particles the game itself draws none of it.
10. On a server you do not host, blood still appears (the client is told about
    the hit there too) — the case this design was chosen for.
11. No crash over a long fight, and no visible cost: the mod's work per tick is
    one pass over the client's mob list plus, at most, 16 wounds of planning and
    256 particles on the widest settings the page offers — 12 and 112 by default.
    (The engine soak in check 9 measures the numbers behind this; it is not the
    same as having played it.)

## Reproducing all of the above

```sh
cargo test
cargo test --release                        # with RUSTFLAGS="-C overflow-checks=on"
cargo +nightly miri test                    # nightly with the `miri` component
cargo clippy --all-targets -- -D warnings
cargo run --example burst > burst.json      # the geometry, as JSON
python3 tools/preview.py burst.json          # ... drawn (a simulation)

git clone --depth 1 --branch nightly https://github.com/horizon-rust/horizon
cd horizon && rm CLAUDE.md && cp AGENTS.md CLAUDE.md   # `apply` takes no symlinks
horizon-mixins apply --workspace horizon --mods mixins --out stage
cd stage
RUSTFLAGS="-D warnings" cargo check -p horizon-client -p horizon-client-lib
cargo test -p horizon-client-lib --lib blood_particles
cargo test -p horizon-client-render --lib particles
tools/engine-soak.sh horizon                # the mod's numbers through that engine
```

Check 9 was run that way here, on a pristine `--depth 1` clone: 304 crates
compiled, `3 passed; 0 failed` in 1.09 s, exit 0. Two notes for a machine like
this one: the stage is a whole engine checkout (about a gigabyte of sources and
a dozen of build artifacts), so on a sandbox where `/tmp` is a `tmpfs` run it with
`TMPDIR` pointing at the disk, and give the build two jobs (`-j 2`) when the box
has two gigabytes of memory.

To build the package the registry takes, put this repository's `mixins/`,
`compatibility.json` and `LICENSE` in `native/blood_particles/` of a
`horizon-mods` clone and run `python3 native/tools/native.py package
blood_particles`.
