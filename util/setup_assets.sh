#!/usr/bin/env bash


echo "Cleanup ./target/assets/"
rm -rf target/assets/

echo "Extracting GRF..."
cargo run -p grf_extractor -- --rathena-db ~/Downloads/ragnarok_online_resource_references/rathena/db ~/Downloads/ragnarok_online_resource_references/data.grf
echo "Scanning files..."
cargo run -p asset_importer -- scan --types body,head,headgear,weapon,shield,shadow,projectile,map,sound,effect,lookup
echo "Importing assets..."
cargo run -p asset_importer -- batch --types body,head,weapon,headgear,shield,shadow,projectile,map,sound,effect,lookup
echo "Decompiling LUB files"
cargo run -p lub_decompiler

echo "Cleanup ./target/tmp/"
rm -rf target/tmp/