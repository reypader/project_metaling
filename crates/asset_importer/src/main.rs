mod batch;
mod dump;
mod manifest;
mod scan;

use anyhow::Result;
use clap::{Parser, Subcommand};
use ro_files::act::ActFile;
use ro_files::SprFile;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "idavoll-resource-asset_importer")]
#[command(about = "Reorganize Ragnarok Online GRF extraction output into a structured layout")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Batch copy sprites from a manifest TOML into the structured output layout
    Batch {
        /// Path to the manifest TOML file
        #[arg(short, long, default_value = "target/manifest.toml")]
        manifest: PathBuf,

        /// Override the output directory from the manifest
        #[arg(short, long, default_value = "target/assets")]
        output: Option<PathBuf>,

        /// Sprite types to process (comma-separated).
        /// Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile, map, sound.
        /// Example: --types body,head,headgear,weapon,shield,shadow,map
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,

        /// Path to translations.toml for Korean path segment translation (map type only).
        /// When provided, GND texture paths and RSW model paths are translated and the
        /// texture/ and model/ directories are copied with translated names.
        #[arg(long, value_name = "PATH", default_value = "config/translations.toml")]
        translations: Option<PathBuf>,
    },

    /// Scan a GRF extraction and generate a manifest TOML file
    Scan {
        /// GRF data root directory (the "data" folder inside the GRF extraction)
        #[arg(long, value_name = "DATA_ROOT", default_value = "target/tmp/data")]
        data_root: PathBuf,

        /// Path to headgear_slots.toml (required when scanning headgear)
        #[arg(
            long,
            value_name = "PATH",
            default_value = "config/headgear_slots.toml"
        )]
        slots: Option<PathBuf>,

        /// Path to weapon_types.toml (required when scanning weapons)
        #[arg(long, value_name = "PATH", default_value = "config/weapon_types.toml")]
        weapon_types: Option<PathBuf>,

        /// Output manifest file path
        #[arg(short, long, default_value = "target/manifest.toml")]
        output: PathBuf,

        /// Sprite types to include (comma-separated).
        /// Valid values: body, head, headgear, garment, weapon, shield, shadow, projectile, map, sound.
        /// Example: --types body,head,headgear,weapon,shield,shadow,map
        #[arg(long, value_name = "TYPES")]
        types: Option<String>,
    },

    /// Dump ACT frame/layer data for inspection
    Dump {
        /// Input ACT file
        act: PathBuf,

        /// Input SPR file (optional; enables canvas size reporting)
        #[arg(long, value_name = "PATH")]
        spr: Option<PathBuf>,

        /// Action indices to dump (comma-separated). Omit to show all visible actions.
        #[arg(long)]
        actions: Option<String>,

        /// Show only which actions have visible sprites (summary mode)
        #[arg(long)]
        scan: bool,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Batch {
            manifest,
            output,
            types,
            translations,
        } => {
            let types = parse_types(types.as_deref())?;
            batch::batch(
                &manifest,
                output.as_deref(),
                types.as_deref(),
                translations.as_deref(),
            )?;
        }

        Command::Scan {
            data_root,
            slots,
            weapon_types,
            output,
            types,
        } => {
            let types = parse_types(types.as_deref())?;
            scan::scan(
                &data_root,
                slots.as_deref(),
                weapon_types.as_deref(),
                &output,
                types.as_deref(),
            )?;
        }

        Command::Dump {
            act,
            spr,
            actions,
            scan,
        } => {
            let act_data = std::fs::read(&act)?;
            let act = ActFile::parse(&act_data)?;

            let spr_file: Option<SprFile> = match spr {
                Some(path) => {
                    let data = std::fs::read(&path)?;
                    Some(SprFile::parse(&data)?)
                }
                None => None,
            };

            let action_filter: Option<Vec<usize>> = actions
                .as_deref()
                .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect());

            if scan {
                dump::scan(&act);
            } else {
                dump::dump(&act, spr_file.as_ref(), action_filter.as_deref());
            }
        }
    }

    Ok(())
}

const VALID_TYPES: &[&str] = &[
    "body",
    "head",
    "headgear",
    "garment",
    "weapon",
    "shield",
    "shadow",
    "projectile",
    "map",
    "sound",
];

fn parse_types(types: Option<&str>) -> anyhow::Result<Option<Vec<String>>> {
    let Some(s) = types else { return Ok(None) };
    let parsed: Vec<String> = s.split(',').map(|t| t.trim().to_string()).collect();
    for t in &parsed {
        if !VALID_TYPES.contains(&t.as_str()) {
            anyhow::bail!(
                "unknown type '{t}'; valid types: {}",
                VALID_TYPES.join(", ")
            );
        }
    }
    Ok(Some(parsed))
}
