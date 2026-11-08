#!/usr/bin/env bash
# Run the mod's numbers through the game's own particle engine.
#
#   tools/engine-soak.sh /path/to/horizon
#
# Needs a Horizon checkout at the game version the mod declares (the shallow one
# the packaging does too: `git clone --depth 1`), the `horizon-mixins` tool and
# the Rust toolchain. It stages the mod into a throwaway copy of that checkout,
# adds this repository's tools/engine_soak.rs to it as one extra test target, and
# runs it there — the patched tree is never touched, and nothing here is
# shipped in the .hnmod.
set -euo pipefail

horizon=${1:?usage: tools/engine-soak.sh /path/to/horizon}
here=$(cd "$(dirname "$0")/.." && pwd)
stage=$(mktemp -d "${TMPDIR:-/tmp}/blood-soak.XXXXXX")
rmdir "$stage"

horizon-mixins apply --workspace "$horizon" --mods "$here/mixins" --out "$stage"
cp "$here/tools/engine_soak.rs" "$stage/crates/horizon-client/tests/blood_particles_soak.rs"
cat >>"$stage/crates/horizon-client-lib/Cargo.toml" <<'TOML'

[[test]]
name = "blood_particles_soak"
path = "../horizon-client/tests/blood_particles_soak.rs"
TOML

cd "$stage"
cargo test --locked -p horizon-client-lib --test blood_particles_soak
