#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat <<EOF
Usage: $0 --grf <path-to-data.grf> --rathena <path-to-rathena-repo>

Options:
  --grf       Path to the GRF archive (e.g. data.grf)
  --rathena   Path to a local rAthena repository clone (the script will use its db/ subdirectory)
  -h, --help  Show this help
EOF
}

GRF_PATH=""
RATHENA_PATH=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --grf)
            GRF_PATH="$2"
            shift 2
            ;;
        --rathena)
            RATHENA_PATH="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage
            exit 1
            ;;
    esac
done

if [[ -z "$GRF_PATH" || -z "$RATHENA_PATH" ]]; then
    echo "Error: --grf and --rathena are required." >&2
    usage
    exit 1
fi

if [[ ! -f "$GRF_PATH" ]]; then
    echo "Error: GRF file not found at $GRF_PATH" >&2
    exit 1
fi

RATHENA_DB="$RATHENA_PATH/db"
if [[ ! -d "$RATHENA_DB" ]]; then
    echo "Error: rAthena db directory not found at $RATHENA_DB" >&2
    exit 1
fi

echo "Cleanup ./target/assets/"
rm -rf target/assets/

echo "Running pipeline..."
./target/release/grf_pipeline "$GRF_PATH" --rathena-db "$RATHENA_DB" -o target/assets --types body,head,headgear,weapon,shield,shadow,projectile,map,sound,effect,lookup,monster

echo "Cleanup ./target/tmp/"
rm -rf target/tmp/
