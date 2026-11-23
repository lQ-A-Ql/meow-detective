/// Resource and evidence thresholds for one analysis request.
#[derive(Debug, Clone)]
pub struct StegLimits {
    pub max_pixels: u64,
    pub max_side: u32,
    pub max_input_bytes: u64,
    pub block_edge: u32,
    pub max_blocks: u32,
    pub max_hotspots: usize,
    pub max_warnings: usize,
    pub max_planes: usize,
    pub min_plane_samples: u64,
    pub min_block_samples: u64,
    pub min_chi_df: u32,
    pub min_rs_groups: u64,
    pub min_spa_pairs: u64,
}

impl Default for StegLimits {
    fn default() -> Self {
        Self {
            max_pixels: 16_777_216,
            max_side: 16_384,
            max_input_bytes: 96 * 1024 * 1024,
            block_edge: 64,
            max_blocks: 4096,
            max_hotspots: 8,
            max_warnings: 16,
            max_planes: 3,
            min_plane_samples: 4096,
            min_block_samples: 1024,
            min_chi_df: 8,
            min_rs_groups: 256,
            min_spa_pairs: 512,
        }
    }
}

impl StegLimits {
    pub(crate) fn check_dimensions(&self, width: u32, height: u32) -> Result<(), &'static str> {
        if width > self.max_side || height > self.max_side {
            return Err("limit.side");
        }
        let pixels = u64::from(width) * u64::from(height);
        if pixels > self.max_pixels {
            return Err("limit.pixels");
        }
        Ok(())
    }
}
