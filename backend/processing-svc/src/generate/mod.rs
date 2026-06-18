//! Tile generation pipeline.
//!
//! This module plans tile coordinates, maps those coordinates into HDF raster
//! windows, renders classified pixels, and packages the output with a manifest.

mod error;
mod orchestration;
mod planning;
mod rendering;
#[cfg(test)]
mod synthetic;
mod types;
mod window;

pub use error::GenerateError;
pub(crate) use orchestration::{
    generate_tile_set_for_granule_with_manifest, GranuleTileSetRequest,
};
pub use types::GeneratedTileSet;
#[cfg(test)]
pub use types::TileGenerationPlan;
