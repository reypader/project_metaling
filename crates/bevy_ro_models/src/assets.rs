use bevy::prelude::*;
use ro_files::RsmFile;

#[derive(Asset, TypePath)]
pub struct RsmAsset {
    pub rsm: RsmFile,
}
