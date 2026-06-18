use crate::{manifest::TileManifest, publish::RenderedTile, tiles::TileRange};

#[cfg(test)]
use crate::{render::RenderPixel, tiles::TileCoord};

#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub struct SyntheticTileInput {
    pub coord: TileCoord,
    pub pixels: Vec<RenderPixel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGenerationPlan {
    pub ranges: Vec<TileRange>,
    pub tile_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub struct SyntheticTileSet {
    pub plan: TileGenerationPlan,
    pub tiles: Vec<RenderedTile>,
    pub manifest: TileManifest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedTileSet {
    pub plan: TileGenerationPlan,
    pub tiles: Vec<RenderedTile>,
    pub manifest: TileManifest,
}
