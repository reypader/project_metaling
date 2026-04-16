mod classify;
mod export;
mod headgear_slots;
mod pipeline;
mod rathena;
mod weapon_types;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "grf-pipeline")]
#[command(about = "Extract, classify, and export Ragnarok Online GRF assets in a single pass")]
struct Args {
    /// Path to the .grf file
    grf: PathBuf,

    /// Final output directory
    #[arg(short, long, default_value = "target/assets")]
    output: PathBuf,

    /// Path to translations.toml
    #[arg(short, long, default_value = "config/translations.toml")]
    translations: PathBuf,

    /// Path to rAthena db/ directory for item ID resolution
    #[arg(long, value_name = "PATH")]
    rathena_db: Option<PathBuf>,

    /// Where to write/read the generated headgear slots file (requires --rathena-db to generate)
    #[arg(long, default_value = "config/headgear_slots.toml")]
    headgear_slots: PathBuf,

    /// Where to write/read the generated weapon types file (requires --rathena-db to generate)
    #[arg(long, default_value = "config/weapon_types.toml")]
    weapon_types: PathBuf,

    /// Asset types to process (comma-separated).
    /// Valid: body, head, headgear, garment, weapon, shield, shadow, projectile, map, sound, effect, lookup.
    #[arg(long, value_name = "TYPES")]
    types: Option<String>,

    /// Where to write untranslated segments
    #[arg(long, default_value = "target/miss_log.toml")]
    miss_log: PathBuf,

    /// Translate and classify without writing output files
    #[arg(long)]
    dry_run: bool,

    /// Print each processed file
    #[arg(short, long)]
    verbose: bool,
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
    "monster",
    "map",
    "sound",
    "effect",
    "lookup",
];

fn main() -> Result<()> {
    let args = Args::parse();

    let types = parse_types(args.types.as_deref())?;

    pipeline::run(
        &args.grf,
        &args.output,
        &args.translations,
        args.rathena_db.as_deref(),
        &args.headgear_slots,
        &args.weapon_types,
        types.as_deref(),
        &args.miss_log,
        args.dry_run,
        args.verbose,
    )
}

fn parse_types(types: Option<&str>) -> Result<Option<Vec<String>>> {
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
