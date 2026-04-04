# project-metaling-exporter

Reorganizes Ragnarok Online GRF extraction output into a structured layout for use by
[bevy_ro_libs](../bevy_ro_libs). Scans an extracted GRF tree, generates a manifest, and
batch-copies raw SPR/ACT/IMF files into a predictable directory hierarchy. No rendering or
format conversion is performed.

Designed to work on output produced by
[project-metaling-extractor](../grf_extractor/README.md). Point it at
`extracted/data/` as the `data_root`.

## Build

```sh
cargo build --release
```

## Subcommands

### `scan` — Generate a manifest from a GRF data root

Walks the `data_root` directory tree and produces a `manifest.toml` describing
all discovered sprites and maps. Requires two mapping files from `extractor`.

```
exporter scan [OPTIONS] <data_root>

Arguments:
  <data_root>  The data/ directory from an extractor output

Options:
  --slots <PATH>         Path to headgear_slots.toml (required when scanning headgear)
  --weapon-types <PATH>  Path to weapon_types.toml (required when scanning weapons)
  -o, --output <PATH>    Output manifest file [default: manifest.toml]
  --types <TYPES>        Asset types to include, comma-separated.
                         Valid values: body, head, headgear, garment, weapon, shield,
                                       shadow, projectile, map, sound
```

ID-based weapon and shield sprites (numeric item IDs) are warned and skipped during scan.
Only generic type sprites (`sword`, `dagger`, etc.) and named shields (`buckler`, `guard`, `shield`, etc.)
are included in the base manifest.

**Example:**

```sh
asset_importer scan extracted/data/ \
    --slots headgear_slots.toml \
    --weapon-types weapon_types.toml \
    --output manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile,map
```

---

### `batch` — Copy assets from a manifest into the structured layout

Reads a manifest produced by `scan` and copies each file to an organized output directory
tree. Map files (GND, RSW) can have their embedded Korean path references translated to
English using `--translations`.

```
exporter batch [OPTIONS] <manifest>

Arguments:
  <manifest>  Path to the manifest TOML file

Options:
  -o, --output <DIR>        Override the output directory from the manifest
  --types <TYPES>           Asset types to process, comma-separated.
                            Valid values: body, head, headgear, garment, weapon, shield,
                                          shadow, projectile, map, sound
  --translations <PATH>     Path to translations.toml for Korean path translation.
                            When provided, GND texture paths and RSW model paths are
                            translated, and the texture/ and model/ directories are
                            copied with translated names. Has no effect on other types.
```

**Example — sprites only:**

```sh
asset_importer batch manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile
```

**Example — full export with translation:**

```sh
asset_importer batch manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile,map,sound \
    --translations translations.toml \
    --output assets/
```

Output structure:

```
assets/
├── sprite/
│   ├── human_{gender}_{job}/    body.spr/.act/.imf
│   │                            weapon/{type}/weapon.spr/.act, slash.spr/.act
│   │                            shield/{name}.spr/.act
│   │                            garment/{name}/garment.spr/.act
│   ├── human_{gender}_head/     head/{id}.spr/.act/.imf
│   │                            headgear/{name}.spr/.act
│   ├── mercenary/               body/{type}/body.spr/.act
│   │                            body/{type}/weapon/weapon.spr/.act, slash.spr/.act
│   ├── shadow/                  shadow.spr/.act
│   └── projectile/              {name}.spr/.act
├── maps/
│   └── {name}/                  {name}.rsw, {name}.gnd, {name}.gat
├── texture/                     diffuse textures referenced by GND files
├── model/                       3D models referenced by RSW files
└── wav/                         sound files referenced by ACT animation events
    └── effect/                  spell and skill effect sounds
```

Directory names under `sprite/` are bundle boundaries. Everything under
`human_male_swordsman/` belongs in one bundle; `human_male_head/` is the shared
bundle for all heads and headgears of that gender.

When `--translations` is provided:
- GND files are rewritten so texture paths use translated names (e.g.
  `texture/유저인터페이스/map/prontera.bmp` becomes `texture/user_interface/map/prontera.bmp`).
- RSW files are rewritten so model paths use translated names with a `model/` prefix.
- The `texture/` and `model/` directories are copied with translated directory and file names
  to match. The `wav/` directory is always copied verbatim; Korean WAV filenames are ACT
  event keys and must be preserved exactly.

Any sprite pair where the `.spr` or `.act` file is missing is skipped and logged to
`skipped.toml` in the output root.

---

### `dump` — Inspect ACT file contents

Print raw action/frame data from an ACT file for debugging.

```
exporter dump [OPTIONS] <act>

Arguments:
  <act>  Input ACT file

Options:
  --spr <PATH>      SPR file (optional; enables canvas size reporting)
  --actions <LIST>  Action indices to dump, comma-separated. Omit to show all.
  --scan            Summary mode: show only which actions have visible sprites
```

---

## Additional resources

### `headgear_slots.toml` and `weapon_types.toml`

Both files are generated by `extractor` and are required inputs to `scan`.

`headgear_slots.toml` maps each headgear sprite to its equipment slot:

```toml
[[headgear]]
view = 17
slot = "Head_Top"
accname = "ribbon"
items = [2208, 2209]
```

`weapon_types.toml` maps each weapon type to its item IDs:

```toml
[[weapon_type]]
id = 1
name = "dagger"
items = [1201, 1202, ...]
```

See `../extractor` for how to generate these files.

---

## Typical workflow

```sh
# 1. Extract and translate the GRF
grf_extractor data.grf -o extracted/ \
    --translations translations.toml \
    --rathena-db /path/to/rathena/db

# 2. Scan the data root to generate a manifest
asset_importer scan extracted/data/ \
    --slots headgear_slots.toml \
    --weapon-types weapon_types.toml \
    --output manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile,map

# 3. Batch copy into structured layout
asset_importer batch manifest.toml \
    --types body,head,headgear,weapon,shield,shadow,projectile,map,sound \
    --translations translations.toml \
    --output assets/
```
