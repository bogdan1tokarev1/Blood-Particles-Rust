# Blood Particles

A native mod for **Minecraft Horizon** (game 26.3, tested on Nightly `2ebf00a`):
a hurt mob sprays blood — real droplets that fly out of the wound, arc under
gravity and spatter on the ground, and a fine mist that drifts and dries
darker. The blow that kills throws **three times** as much of it, and how much a
hit throws follows **how hard it was**. Every part of that is configurable, from
**Options > Mods** or from chat.

By **Bogdan_Tokarev**. Version **1.2.0**. MIT.

| | |
| --- | --- |
| Mod id | `blood_particles` |
| Kind | native mod (Horizon mixin bundle) |
| Game | 26.3 |
| Tested on Horizon commit | `2ebf00a0281c592f79191e76fce0e7d522c6f425` (Nightly, hook API 411) |
| Files | 1 module + 6 injections, 7 `.rs` + 1 `.toml` |
| Settings | 12, each with its own button in **Options > Mods** |

## What it does

When any mob takes a hit — your sword, an arrow, another mob, a fall, fire, a
creeper's blast — the wound throws, by default:

- **8 droplets**, taken from the game's own block-crumb particle: they have
  gravity, drag and terrain collision, so they arc out of the body and spatter
  on the ground, leaving a stain where they land;
- **6 puffs of mist**, the game's tinted dust particle, fading from wet blood to
  the darker colour it dries to;
- **2 low clouds** of the same mist, wider and fainter, that hang where the
  spray went.

**A hit of three hearts (6 damage) throws exactly what the page says.**
A lighter hit throws less, a heavier one more: the count is scaled by how much
it took, from a third of it to two and a half times, so a punch still spits and
a mace still looks like a spray and not a fountain. `Damage sizes the burst`
turns that off (*off*), halves it (*soft*) or leaves it at full (*full*).

**The killing blow throws three times as much** — 24 droplets, 18 of mist, 6
clouds on the default settings — and throws it wider and slower, because a body
going down does not take a tidy hit. `The killing blow throws` is the multiplier
(x1 to x5). A dying mob is not left bleeding every tick after that.

The spray is aimed **away from the player**: blood leaves the far side of a
blow, so a mob hit from the front bleeds backwards, and the wound itself sits in
the mob's upper body (0.62 of its height), not at its feet.

Each mob keeps its own blood:

| Mob | Blood | Droplets are crumbs of |
| --- | --- | --- |
| everything else | red | `nether_wart_block` |
| slime | green | `slime_block` |
| blaze, magma cube, strider | fire | `magma_block` |
| enderman, endermite, shulker, ender dragon | violet ichor | `purpur_block` |
| skeleton, stray, bogged, wither skeleton, skeleton horse, zombified piglin | bone chips | `bone_block` |

There are no textures, models, blocks or items in the bundle: the mod draws with
particles the game already has, so it changes nothing about how the game looks
when it is off, and it needs no resource pack to look right.

## The settings

The page has **one button for each setting and a last one that puts everything
back**. A click moves that setting to its next value, and the button's label is
redrawn at once, because the game asks the mod for its labels again after every
click and every time the page opens. What the page writes and what the command
writes is the same line, saved in the file the game keeps for the mod, so the
two always agree and a change survives a restart.

| Button | Its values |
| --- | --- |
| `Droplets a hit` | 0, 4, 6, 8, 12, 16, 20, 26 |
| `Mist a hit` | 0, 2, 4, 6, 8, 12 |
| `Clouds a hit` | 0, 1, 2, 3, 4 |
| `Damage sizes the burst` | off, soft, full |
| `The killing blow throws` | x1, x2, x3, x4, x5 |
| `Spread` | jet, normal, splash |
| `Arc` | low, normal, high |
| `Each mob's own colour` | on, off |
| `Droplets are block crumbs` | on, off |
| `Drawn within` | 16, 24, 32, 40, 64 blocks |
| `Wounds a tick` | 2, 4, 8, 12, 16 |
| `Particles a tick` | 32, 64, 112, 160, 256 |
| `Reset to defaults` | everything back to the row above |

## `/bloodparticles`

The same 12 settings, from chat, under either name (`/bp` is the short one):

```
/bloodparticles                      what the settings are now, in three lines
/bloodparticles help                 every setting and the value it takes
/bloodparticles droplets 12          one setting, by name
/bloodparticles spread splash        the words work too
/bloodparticles reset                everything back to its defaults
```

Values snap to the nearest one the page can show, so `/bloodparticles droplets
13` is `12` and the button under it says the same. A name that is not a setting,
or a value that is not one, answers with what is wrong instead of changing
nothing quietly. Both names are completed in the chat box.

## How it works

The client already knows about every hit it can see: the server's damage packet
(and the hurt animation) restart the mob's hurt flash, `LivingEntity.hurtTime`,
at 10, and the client counts it down one a tick — so a flash that went **up**
since the last tick is a hit. The health the blow took is not in the flash, so
the wound waits up to three ticks for the mob's new health to arrive in the
synched data it is sent in; a hit that reports no health change is drawn with
the unscaled count once the wait is over. `mixins/blood_particles/tick.rs` runs
once a tick in the window's game, reads the mob list, and for each fresh flash
asks `runtime.rs` for a burst and hands the drops to the game's particle engine
(`ParticleEngine::add`).

That is why the mod works **everywhere the client does**: single player, a LAN
world you host, and any server you join, without the server knowing about it. It
sends nothing over the network, and the only file it touches is its own settings
line in the directory the game keeps for it.

Bounds the mod keeps, so a busy fight cannot cost a frame:

- the ceilings above: whatever the settings say, a burst stops at 64 droplets,
  48 puffs of mist and 12 clouds, and a tick at the page's `Wounds` and
  `Particles` — the mod trims what it hands the game to those numbers itself, so
  they hold for whoever calls it and not only for the draw loop;
- hits farther than the page's `Drawn within` are not planned at all (the game
  itself drops a particle spawned more than 32 blocks from the camera);
- a mob's flash is drawn once per hit: two reports of the same hit (a packet and
  an animation) make one burst, and a mob's table is capped at 4096 entries;
- a mob the client has no position for is left alone: a coordinate that is not a
  number, or one outside the world the dimension is built in, never reaches the
  particle engine (a `NaN` is a particle that never lands, and a mob id is only
  a number that the next world hands out again);
- the game's own **Particles** setting still applies: on *Minimal* the game
  draws none of these.

## Building it

The bundle builds into the game with
[horizon-mixins](https://github.com/horizon-rust/horizon-mixins) 0.6.0
(`e8cd0aa`), which the Horizon launcher runs for you:

```
horizon-mixins apply --workspace <horizon source> --mods . --out <stage>
python3 native/tools/native.py package blood_particles   # inside horizon-mods
```

The mod's own tests need only Rust (edition 2024, 1.95 or newer):

```
cargo test                                     # 35 tests: 20 of the logic and
                                               # 15 adversarial soaks
cargo clippy --all-targets -- -D warnings      # clean
cargo +nightly miri test                       # no UB, no double borrow, no leak
cargo run --example burst > burst.json         # the geometry, as JSON
python3 tools/preview.py burst.json            # ... drawn
```

Layout:

```
mixins/blood_particles/
  horizon-mixin.toml   what to inject where
  runtime.rs           the mod's logic and its 20 tests (engine-free)
  tick.rs              once a tick: fresh hurt flashes become bursts
  command.rs           /bloodparticles and /bp, and what they change
  commands.rs          their names in the chat box
  settings.rs          the buttons of its page in Options > Mods
  settings_click.rs    a click on one of them, and the file it writes
  list.rs              its row in Options > Mods
examples/burst.rs      the burst's geometry as JSON, for the preview
tools/preview.py       draws that JSON (a simulation, not a screenshot)
```

## What it does not do

- It does not bleed the player: the flash of your own body is a screen effect,
  not a particle burst, and the mod is about the mobs you fight.
- It does not change damage, health, drops or hitboxes. It only draws.
- It opens no network connection, and it writes one line to its own settings
  file when you change a setting — from the page or from chat.

## Licence

MIT, see `LICENSE`. Horizon itself is MIT and not affiliated with this mod.
