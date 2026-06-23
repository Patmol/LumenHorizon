use thiserror::Error;

use crate::{
    hdf_cli::HdfCliError,
    manifest::ManifestError,
    render::RenderError,
    science::ScienceError,
    tiles::{GeographicBounds, TileCoord, TileMathError},
};

#[cfg(test)]
use crate::tiles::TileRange;

#[derive(Debug, Error)]
pub enum GenerateError {
    #[error("source bounds {source_bounds:?} do not overlap configured tile bounds {configured_bounds:?}")]
    ConfiguredBoundsOutsideSource {
        source_bounds: GeographicBounds,
        configured_bounds: GeographicBounds,
    },

    #[error(transparent)]
    HdfCli(#[from] HdfCliError),

    #[cfg(test)]
    #[error("tile window must be square and fit in u16, got {width}x{height}")]
    InvalidTileWindow { width: usize, height: usize },

    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error("failed to render tile {coord:?}")]
    RenderTile {
        coord: TileCoord,
        #[source]
        source: RenderError,
    },

    #[error("sample count mismatch: radiance={radiance}, quality={quality}, cloud={cloud:?}, observation_count={observation_count:?}")]
    SampleCountMismatch {
        radiance: usize,
        quality: usize,
        cloud: Option<usize>,
        observation_count: Option<usize>,
    },

    #[error("no renderable tile evidence in generated coverage {generation_bounds:?}")]
    NoRenderableTiles { generation_bounds: GeographicBounds },

    #[error("resized dataset '{dataset}' returned {actual} sample(s), expected {expected}")]
    ResizedSampleCountMismatch {
        dataset: &'static str,
        expected: usize,
        actual: usize,
    },

    #[error("tile render worker failed: {source}")]
    RenderWorker {
        #[source]
        source: tokio::task::JoinError,
    },

    #[error(transparent)]
    Science(#[from] ScienceError),

    #[error("planned tile count {tile_count} exceeds u32 range")]
    TileCountOverflow { tile_count: u64 },

    #[error(transparent)]
    TileMath(#[from] TileMathError),

    #[cfg(test)]
    #[error("synthetic tile coordinate {coord:?} is outside requested tile range {range:?}")]
    TileOutsideRange { coord: TileCoord, range: TileRange },

    #[error("tile {coord:?} does not intersect source bounds {source_bounds:?}")]
    TileOutsideSourceBounds {
        coord: TileCoord,
        source_bounds: GeographicBounds,
    },
}
