#!/usr/bin/env bash


echo "Cleanup ./target/assets/"
rm -rf target/assets/

echo "Running pipeline..."
./target/release/grf_pipeline ~/Downloads/ragnarok_online_resource_references/data.grf --rathena-db ~/Downloads/ragnarok_online_resource_references/rathena/db -o target/assets --types body,head,headgear,weapon,shield,shadow,projectile,map,sound,effect,lookup,monster

echo "Cleanup ./target/tmp/"
rm -rf target/tmp/