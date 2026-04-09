use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use clap::Parser;
use luadec::{DecompileOptions, LuaDecompiler};
use lunify::{Format, Settings};

#[derive(Parser)]
#[command(name = "lub-decompiler")]
#[command(about = "Decompile Ragnarok Online .lub bytecode files to Lua source")]
struct Args {
    /// Only decompile files whose path contains this string
    #[arg(default_value = "effect")]
    filter: String,

    /// Output directory for decompiled .lua files
    #[arg(short, long, default_value = "../../target/lua")]
    output: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let pattern = format!("../../target/tmp/data/**/*{}*.lub", args.filter);
    let output_dir = Path::new(&args.output);
    fs::create_dir_all(output_dir)?;

    let decompiler = LuaDecompiler::with_options(DecompileOptions {
        include_debug_comments: true,
        indent_width: 4,
        include_prototype_numbers: true,
    });
    let format = Format::default();
    let settings = Settings::default();

    let paths: Vec<_> = glob::glob(&pattern)
        .context("invalid glob pattern")?
        .filter_map(|e| e.ok())
        .collect();

    println!("Found {} file(s) matching {}", paths.len(), pattern);

    let mut success = 0usize;
    let mut errors = 0usize;

    for path in &paths {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let out_path = output_dir.join(format!("{}.lua", stem));

        println!("[{}/{}] {}", success + errors + 1, paths.len(), path.display());

        let input = match fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("  SKIP: read error: {}", e);
                errors += 1;
                continue;
            }
        };

        let unified = match lunify::unify(&input, &format, &settings) {
            Ok(bytes) => bytes,
            Err(e) => {
                eprintln!("  SKIP: lunify error: {:?}", e);
                errors += 1;
                continue;
            }
        };

        let mut tmp = match tempfile::NamedTempFile::new() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("  SKIP: temp file error: {}", e);
                errors += 1;
                continue;
            }
        };
        if let Err(e) = tmp.write_all(&unified) {
            eprintln!("  SKIP: temp write error: {}", e);
            errors += 1;
            continue;
        }

        let result = std::panic::catch_unwind(|| decompiler.decompile_file(tmp.path()));
        match result {
            Ok(Ok(source)) => {
                if let Err(e) = fs::write(&out_path, source) {
                    eprintln!("  SKIP: output write error: {}", e);
                    errors += 1;
                } else {
                    success += 1;
                }
            }
            Ok(Err(e)) => {
                eprintln!("  SKIP: decompile error: {}", e);
                errors += 1;
            }
            Err(_) => {
                eprintln!("  SKIP: luadec panicked");
                errors += 1;
            }
        }
    }

    println!("Done: {success} succeeded, {errors} failed.");

    Ok(())
}
