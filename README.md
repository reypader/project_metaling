# Project Metaling


Single-player variant of the Ragnarok Online client using Rust and Bevy. Casual project mainly intended to learn Rust and Bevy.

## Credits

This project leans heavily on the file-format research, tooling, and prior implementations published by the Ragnarok Online community. Huge thanks to:

* [BrowEdit3](https://github.com/Borf/BrowEdit3) by Borf, for the canonical reference on map (RSW/GND/GAT) and model (RSM) handling.
* [RagnarokFileFormats](https://github.com/rdw-archive/RagnarokFileFormats) by rdw, for the format documentation that made parsing GRF, ACT, SPR, etc possible.
* [Ragnarok Research Lab](https://github.com/RagnarokResearchLab/ragnarokresearchlab.github.io), for deep dives into rendering quirks and engine behavior.
* [Ragnarok Rebuild](https://github.com/Doddler/RagnarokRebuild) by Doddler, for a reference reimplementation that informed many gameplay and asset-pipeline decisions.

## Prerequisites

* Rust toolchain. Install via [rustup](https://rustup.rs/).
* A copy of the original Ragnarok Online `data.grf` archive.
* A local clone of the [rAthena](https://github.com/rathena/rathena) repository. The `db/` subdirectory is read by the asset pipeline for item, monster, and job metadata.
* (Optional) BGM `.mp3` files copied manually into `target/assets/bgm/` after the asset pipeline runs. They are not part of the GRF.

## Setup

1. Clone this repository.
2. Build the asset pipeline in release mode (the setup script invokes the release binary):

   ```shell
   cargo build --release -p grf_pipeline
   ```

3. Run the asset setup script, pointing it at your GRF and rAthena clone:

   ```shell
   ./util/setup_assets.sh --grf /path/to/data.grf --rathena /path/to/rathena
   ```

   The script wipes `target/assets/`, runs `grf_pipeline` with the standard set of asset types (body, head, headgear, weapon, shield, shadow, projectile, map, sound, effect, lookup, monster), and removes the temporary extraction directory. Re-run it whenever the source GRF or rAthena db changes.

4. (Optional) Copy BGM tracks into `target/assets/bgm/`. The filenames must match the entries listed in `misc/mp3nametable.txt` from the GRF.

## Running

Launch the test harness game crate:

```shell
RUST_BACKTRACE=1 cargo run -p game
```

This is the visual verification harness for the `bevy_ro_*` plugins (sprites, models, maps, sounds, vfx).

## Repository layout

* `crates/ro_files/` : parsers for GRF, ACT, SPR, GND, RSW, RSM, IMF, GAT, STR.
* `crates/grf_pipeline/` : single-pass extract + classify + export pipeline driven by `setup_assets.sh`.
* `crates/bevy_ro_sprites/`, `bevy_ro_models/`, `bevy_ro_maps/`, `bevy_ro_sounds/`, `bevy_ro_vfx/` : Bevy plugins for rendering.
* `crates/game/` : test harness binary.
* `crates/lub_decompiler/` : CLI for decompiling `.lub` Lua bytecode.
* `config/` : TOML configuration consumed by the asset pipeline.
* `util/` : helper scripts including `setup_assets.sh`.

Each `bevy_ro_*` crate has its own `README.md` covering plugin configuration and usage.

## AI usage disclosure

Claude has been largely used to get the rendering working. Gameplay would follow (when I get to it) as hand-written but AI-assisted implementation (i.e., code review, note keeping, un-stucking).
