use std::process::ExitStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RasterShape {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RasterWindow {
    pub x_offset: usize,
    pub y_offset: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RasterSample {
    pub x: f64,
    pub y: f64,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RasterOutputSize {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug)]
pub(super) struct GdalInfoOutput {
    pub(super) status: ExitStatus,
    pub(super) stdout: String,
    pub(super) stderr: String,
}
