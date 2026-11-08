# Registry page draft (mod-page.md)

Fields for the form at https://playhorizon.dev/mods/submit, and the text the
page at `playhorizon.dev/mods/blood_particles` will show.

- **id**: `blood_particles`
- **name**: `Blood Particles`
- **summary** (>= 50 characters): `Realistic blood from every hit: droplets that arc and spatter, a mist that dries darker, three times as much on the killing blow. Every part of it is yours to set.`
- **categories** (1 to 3): `decoration`, `adventure`
- **tags**: `particles`, `blood`, `combat`, `visual`, `effects`, `config`, `client`
- **license**: `MIT`
- **source**: `https://github.com/<your-login>/blood-particles`
- **icon**: `icons/blood_particles.png` (the 16x16 pixel-art droplet in this
  repository's `icon.png`)
- **version**: `1.2.0`, **game**: `26.3`

## Description

A hurt mob bleeds. Every hit it takes — your sword, an arrow, another mob, a
fall, fire, a creeper's blast — throws **8 droplets** that fly out of the wound,
arc under gravity and spatter on the ground, **6 puffs of mist** that drift and
dry darker, and **2 low clouds** where the spray went.

**How hard the blow was decides how much blood there is:** a three-heart hit
throws exactly what you set, a lighter one less, a heavier one more (from a
third of it to two and a half times). **The killing blow throws three times as
much**, wider and slower, and a dying body is not left bleeding every tick
after.

The spray leaves the **far side of the blow**: a zombie hit from the front
bleeds backwards, and the wound sits in its upper body, not at its feet.

**Each mob has its own blood:**

| Mob | Blood |
| --- | --- |
| everything else | red |
| slime | green |
| blaze, magma cube, strider | fire |
| enderman, endermite, shulker, ender dragon | violet ichor |
| skeletons and their kin | pale bone chips |

**Everything is configurable, twice over.** **Options > Mods** has a page with
one button for each of its 12 settings and a button that puts them all back:
how many droplets, how much mist, how many clouds, whether damage sizes the
burst, the multiplier on the killing blow, the width of the spray, the height of
the arc, each mob's own colour, droplets as block crumbs or mist alone, the
distance it draws within, and the two ceilings of a tick. `/bloodparticles` (or
`/bp`) sets the same 12 from chat — `/bp droplets 12`, `/bp spread splash`,
`/bp reset` — and what either one changes is saved to the mod's own file, so it
is still there next time you start the game.

- **No textures, no models, no resource pack.** The droplets are the game's own
  block-crumb particle (gravity, drag and terrain collision included), so they
  spatter and leave a stain where they land; the mist is its tinted dust.
- **Client side.** It draws on your machine from what the client is already
  told, so it works in single player, on a LAN world and **on any server**,
  sending nothing over the network.
- **Bounded.** A burst stops at 64 droplets, 48 puffs of mist and 12 clouds; a tick
  at the page's own two ceilings — 8 wounds and 112 particles by default, up to
  16 and 256; one hit is drawn once; the game's *Particles* setting still
  applies (on *Minimal* the game draws none of it).
- **Off means off.** The switch in **Options > Mods** stops it on the next tick,
  in the same world, without a restart.
- **Checked, not assumed.** 35 tests, an adversarial soak (mob coordinates that
  are not numbers, a config file of noise, four threads at once, a tick counter
  that wraps) run under Miri for undefined behaviour, and the mod's own output
  pushed through the game's real particle engine. `VERIFICATION.md` in the
  repository lists what was checked and what could not be: there was no live game
  run here, because this build machine has no launcher, no GPU and no Minecraft
  assets.

It does not bleed the player, and it changes no damage, health, drops or
hitboxes: it only draws.

By Bogdan_Tokarev. MIT.

## What the 1.2.0 page needs from you

- The screenshots to upload: `blood-particles-preview.png` (six blows, three
  moments each, and the page), `blood-particles-settings.png` (the page on its
  own) and `blood-particles-animation.gif` (the killing blow, 26 ticks). All
  three are **drawn from the mod's own burst data**, not taken from the game:
  say so in the caption if the form has one.
- The changelog line below is what the release shows; `publish/blood_particles.toml`
  carries the same text with the file's size and hash.
