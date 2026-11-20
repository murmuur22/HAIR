// Local patch hair.1: all decoder work is synchronous, for every width and
// feature selection. Historical multithreaded.rs/rayon.rs remain provenance
// only: they are deliberately not declared as modules in this fork.
mod immediate;

use crate::decoder::{choose_color_convert_func, ColorTransform};
use crate::error::Result;
use crate::parser::{Component, Dimensions};
use crate::upsampler::Upsampler;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;

pub struct RowData {
    pub index: usize,
    pub component: Component,
    pub quantization_table: Arc<[u16; 64]>,
}

pub trait Worker {
    fn start(&mut self, row_data: RowData) -> Result<()>;
    fn append_row(&mut self, row: (usize, Vec<i16>)) -> Result<()>;
    fn get_result(&mut self, index: usize) -> Result<Vec<u8>>;
    fn append_rows(&mut self, row: &mut dyn Iterator<Item = (usize, Vec<i16>)>) -> Result<()> {
        for item in row {
            self.append_row(item)?;
        }
        Ok(())
    }
}

#[allow(dead_code)]
pub enum PreferWorkerKind {
    Immediate,
    Multithreaded,
}

#[derive(Default)]
pub struct WorkerScope {
    inner: RefCell<Option<immediate::ImmediateWorker>>,
}

impl WorkerScope {
    pub fn with<T>(with: impl FnOnce(&Self) -> T) -> T {
        with(&Self::default())
    }

    pub fn get_or_init_worker<T>(
        &self,
        _prefer: PreferWorkerKind,
        f: impl FnOnce(&mut dyn Worker) -> T,
    ) -> T {
        let mut inner = self.inner.borrow_mut();
        f(inner.get_or_insert_with(immediate::ImmediateWorker::default))
    }
}

// Keep the historical internal name to avoid unrelated call-site churn.
// This includes the final color/upsample pass: no Rayon dispatch exists.
pub fn compute_image_parallel(
    components: &[Component],
    data: Vec<Vec<u8>>,
    output_size: Dimensions,
    color_transform: ColorTransform,
) -> Result<Vec<u8>> {
    let color_convert_func = choose_color_convert_func(components.len(), color_transform)?;
    let upsampler = Upsampler::new(components, output_size.width, output_size.height)?;
    let line_size = output_size.width as usize * components.len();
    let mut image = vec![0u8; line_size * output_size.height as usize];
    for (row, line) in image.chunks_mut(line_size).enumerate() {
        upsampler.upsample_and_interleave_row(
            &data,
            row,
            output_size.width as usize,
            line,
            color_convert_func,
        );
    }
    Ok(image)
}
