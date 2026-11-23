use crate::decode::{DecodedHeader, RowSink};
use crate::limits::StegLimits;
use crate::stats::{assess, band_for, Accumulator};
use crate::types::{
    PlaneKind, StegAssessment, StegBlockReport, StegMethod, StegPlaneReport, StegSeverity,
    StegWarning,
};
use crate::warnings::WarningCollector;

const HOTSPOT_MIN_SCORE: f64 = 10.0;

/// Accumulates global statistics and a deterministic subset of 64x64 blocks.
pub(crate) struct ScanSink {
    width: usize,
    height: u32,
    plane_kinds: Vec<PlaneKind>,
    indexed: bool,
    edge: u32,
    limits: StegLimits,
    global: Vec<Accumulator>,
    selected_cols: Vec<u32>,
    selected_row_stride: u32,
    current_band: Vec<Accumulator>,
    current_band_row: Option<u32>,
    blocks_total: u32,
    blocks_scored: u32,
    hotspots: Vec<StegBlockReport>,
    warnings: WarningCollector,
    rows_seen: u32,
}

pub(crate) struct ScanResults {
    pub planes: Vec<StegPlaneReport>,
    pub global: Option<StegAssessment>,
    pub dominant: Option<PlaneKind>,
    pub blocks: Vec<StegBlockReport>,
    pub blocks_scored: u32,
    pub blocks_total: u32,
    pub warnings: Vec<StegWarning>,
}

impl ScanSink {
    pub(crate) fn new(header: DecodedHeader, limits: &StegLimits) -> Self {
        let edge = limits.block_edge.max(1);
        let cols = header.width.div_ceil(edge);
        let rows = header.height.div_ceil(edge);
        let total = cols.saturating_mul(rows);
        let stride = choose_stride(cols, rows, limits.max_blocks);
        let selected_cols: Vec<u32> = if limits.max_blocks == 0 {
            Vec::new()
        } else {
            (0..cols).step_by(stride as usize).collect()
        };
        let plane_kinds = plane_kinds(header);
        let mut warnings = WarningCollector::new(limits.max_warnings);
        if header.channels.alpha_excluded() {
            warnings.push(
                "image.steg.alpha-excluded",
                StegSeverity::Info,
                "Alpha channels are excluded from steganalysis.",
            );
        }
        if header.channels.indexed() {
            warnings.push(
                "image.steg.palette-index-domain",
                StegSeverity::Warning,
                "Palette images are tested in the index domain with the chi-square test only.",
            );
        }
        if total > limits.max_blocks {
            let message = if limits.max_blocks == 0 {
                "Local block scoring is disabled by the configured zero block budget.".to_owned()
            } else {
                format!(
                    "{total} blocks exceed the {} block budget; a deterministic stride of {stride} was applied.",
                    limits.max_blocks
                )
            };
            warnings.push("image.steg.blocks-truncated", StegSeverity::Info, message);
        }
        let band_slots = selected_cols.len() * plane_kinds.len();
        Self {
            width: header.width as usize,
            height: header.height,
            plane_kinds,
            indexed: header.channels.indexed(),
            edge,
            limits: limits.clone(),
            global: (0..header.plane_count())
                .map(|_| Accumulator::new())
                .collect(),
            selected_cols,
            selected_row_stride: stride,
            current_band: (0..band_slots).map(|_| Accumulator::new()).collect(),
            current_band_row: None,
            blocks_total: total,
            blocks_scored: 0,
            hotspots: Vec::new(),
            warnings,
            rows_seen: 0,
        }
    }

    pub(crate) fn rows_seen(&self) -> u32 {
        self.rows_seen
    }

    fn aggregate_band_row(&mut self, planes: &[&[u8]]) {
        let plane_count = self.plane_kinds.len();
        for (plane_index, row) in planes.iter().enumerate() {
            for (slot, &block_x) in self.selected_cols.iter().enumerate() {
                let start = (block_x * self.edge) as usize;
                let end = ((block_x + 1) * self.edge).min(self.width as u32) as usize;
                if start < end && end <= row.len() {
                    self.current_band[slot * plane_count + plane_index]
                        .add_segment(&row[start..end]);
                }
            }
        }
    }

    fn finish_band(&mut self) {
        let Some(block_row) = self.current_band_row else {
            return;
        };
        let plane_count = self.plane_kinds.len();
        let y = block_row * self.edge;
        let block_height = self.edge.min(self.height - y);
        for slot in 0..self.selected_cols.len() {
            let x = self.selected_cols[slot] * self.edge;
            let block_width = self.edge.min(self.width as u32 - x);
            let mut scored = false;
            for plane_index in 0..plane_count {
                let accumulator = &mut self.current_band[slot * plane_count + plane_index];
                if let Some(report) = make_block_report(
                    accumulator,
                    &self.limits,
                    self.indexed,
                    self.plane_kinds[plane_index],
                    BlockRect {
                        x,
                        y,
                        width: block_width,
                        height: block_height,
                    },
                ) {
                    scored = true;
                    if report.score >= HOTSPOT_MIN_SCORE {
                        self.hotspots.push(report);
                    }
                }
                accumulator.clear();
            }
            self.blocks_scored += u32::from(scored);
        }
    }

    pub(crate) fn finish(mut self) -> ScanResults {
        self.finish_band();
        self.current_band_row = None;
        let mut planes = Vec::with_capacity(self.plane_kinds.len());
        let mut best: Option<(f64, usize)> = None;
        for index in 0..self.plane_kinds.len() {
            let plane = self.plane_kinds[index];
            let samples = self.global[index].samples;
            let unique = self.global[index].unique_count();
            let assessment = self.plane_assessment(plane, index);
            if let Some(score) = assessment.as_ref().and_then(|value| value.score) {
                if best.is_none_or(|(current, _)| score > current) {
                    best = Some((score, index));
                }
            }
            planes.push(StegPlaneReport {
                plane,
                samples,
                unique_values: unique,
                assessment,
            });
        }
        let global = if self.indexed {
            planes.first().and_then(|plane| plane.assessment.clone())
        } else {
            best.and_then(|(_, index)| planes[index].assessment.clone())
        };
        let dominant = if self.indexed {
            Some(PlaneKind::Index)
        } else {
            best.map(|(_, index)| planes[index].plane)
        };
        self.hotspots.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
                .then_with(|| left.plane.as_str().cmp(right.plane.as_str()))
        });
        self.hotspots.truncate(self.limits.max_hotspots);
        ScanResults {
            planes,
            global,
            dominant,
            blocks: self.hotspots,
            blocks_scored: self.blocks_scored,
            blocks_total: self.blocks_total,
            warnings: self.warnings.into_vec(),
        }
    }

    fn plane_assessment(&mut self, plane: PlaneKind, index: usize) -> Option<StegAssessment> {
        let accumulator = &self.global[index];
        if accumulator.samples < self.limits.min_plane_samples {
            self.warnings.push(
                "image.steg.small-image",
                StegSeverity::Warning,
                "At least one plane has fewer samples than the configured minimum and was skipped.",
            );
            return None;
        }
        if accumulator.unique_count() < 2 {
            self.warnings.push(
                "image.steg.low-texture",
                StegSeverity::Warning,
                "At least one plane has fewer than two unique values and was skipped.",
            );
            return None;
        }
        let methods = accumulator.method_reports(&self.limits, !self.indexed);
        if methods.is_empty() {
            return None;
        }
        if !self.indexed {
            if !has_method(&methods, StegMethod::Rs) {
                self.warnings.push(
                    "image.steg.rs-degenerate",
                    StegSeverity::Warning,
                    "The RS estimate was unavailable for at least one plane.",
                );
            }
            if !has_method(&methods, StegMethod::Spa) {
                self.warnings.push(
                    "image.steg.spa-degenerate",
                    StegSeverity::Warning,
                    "The SPA estimate was unavailable for at least one plane.",
                );
            }
        }
        let assessment = assess(&methods);
        if assessment.methods_used < 2 {
            self.warnings.push(
                "image.steg.single-method",
                StegSeverity::Warning,
                format!(
                    "Plane {} produced fewer than two methods, so no fused score was computed.",
                    plane.as_str()
                ),
            );
        }
        Some(assessment)
    }
}

impl RowSink for ScanSink {
    fn on_row(&mut self, y: u32, planes: &[&[u8]], width: usize) {
        if width != self.width || planes.len() != self.plane_kinds.len() {
            return;
        }
        self.rows_seen += 1;
        for (index, row) in planes.iter().enumerate() {
            if row.len() == width {
                self.global[index].add_segment(row);
            }
        }
        let block_row = y / self.edge;
        if self.selected_cols.is_empty() || !block_row.is_multiple_of(self.selected_row_stride) {
            return;
        }
        if self.current_band_row != Some(block_row) {
            self.finish_band();
            self.current_band_row = Some(block_row);
        }
        self.aggregate_band_row(planes);
        let block_height = self.edge.min(self.height - block_row * self.edge);
        if y % self.edge + 1 == block_height {
            self.finish_band();
            self.current_band_row = None;
        }
    }
}

#[derive(Clone, Copy)]
struct BlockRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn make_block_report(
    accumulator: &Accumulator,
    limits: &StegLimits,
    indexed: bool,
    plane: PlaneKind,
    rect: BlockRect,
) -> Option<StegBlockReport> {
    if accumulator.samples < limits.min_block_samples || accumulator.unique_count() < 2 {
        return None;
    }
    let methods = accumulator.method_reports(limits, !indexed);
    if indexed {
        let chi = methods
            .iter()
            .find(|method| method.method == StegMethod::ChiSquare)?;
        let score = chi.score?;
        return Some(StegBlockReport {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            plane,
            score,
            band: band_for(Some(score)),
            methods_used: 1,
        });
    }
    let assessment = assess(&methods);
    let score = assessment.score?;
    Some(StegBlockReport {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
        plane,
        score,
        band: assessment.band,
        methods_used: assessment.methods_used,
    })
}

fn has_method(methods: &[crate::types::StegMethodReport], method: StegMethod) -> bool {
    methods.iter().any(|item| item.method == method)
}

fn plane_kinds(header: DecodedHeader) -> Vec<PlaneKind> {
    use crate::decode::Channels;
    match header.channels {
        Channels::Gray | Channels::GrayAlpha => vec![PlaneKind::Gray],
        Channels::Rgb | Channels::Rgba => {
            vec![PlaneKind::Red, PlaneKind::Green, PlaneKind::Blue]
        }
        Channels::Indexed => vec![PlaneKind::Index],
    }
}

fn choose_stride(cols: u32, rows: u32, max_blocks: u32) -> u32 {
    let max_blocks = max_blocks.max(1);
    let mut stride = 1u32;
    while cols.div_ceil(stride).saturating_mul(rows.div_ceil(stride)) > max_blocks {
        stride += 1;
        if stride > cols.max(rows) {
            return stride;
        }
    }
    stride
}
