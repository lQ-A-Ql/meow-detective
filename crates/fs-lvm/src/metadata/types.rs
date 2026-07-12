/// Parsed volume group metadata.
#[derive(Debug, Clone)]
pub struct VolumeGroup {
    pub name: String,
    pub id: String,
    /// Extent size in sectors (typically 512-byte sectors).
    pub extent_size: u64,
    /// Monotonic sequence number; highest valid copy wins.
    pub seqno: u64,
    pub physical_volumes: Vec<PvMeta>,
    pub logical_volumes: Vec<LvMeta>,
}

#[derive(Debug, Clone)]
pub struct PvMeta {
    pub name: String,
    pub uuid: String,
    /// Physical extent start in sectors.
    pub pe_start: u64,
    pub pe_count: u64,
}

#[derive(Debug, Clone)]
pub struct LvMeta {
    pub name: String,
    pub uuid: String,
    pub status: Vec<String>,
    pub role: LvRole,
    pub segments: Vec<SegmentMeta>,
    /// Total size in bytes, derived from segments.
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LvRole {
    Public,
    ThinVolume,
    ThinPool,
    ThinData,
    ThinMetadata,
    CacheVolume,
    CachePool,
    CacheData,
    CacheMetadata,
    RaidImage,
    RaidMetadata,
    MirrorImage,
    MirrorLog,
    Snapshot,
    Internal,
}

impl LvRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            LvRole::Public => "public",
            LvRole::ThinVolume => "thin",
            LvRole::ThinPool => "thin-pool",
            LvRole::ThinData => "thin-data",
            LvRole::ThinMetadata => "thin-metadata",
            LvRole::CacheVolume => "cache",
            LvRole::CachePool => "cache-pool",
            LvRole::CacheData => "cache-data",
            LvRole::CacheMetadata => "cache-metadata",
            LvRole::RaidImage => "raid-image",
            LvRole::RaidMetadata => "raid-metadata",
            LvRole::MirrorImage => "mirror-image",
            LvRole::MirrorLog => "mirror-log",
            LvRole::Snapshot => "snapshot",
            LvRole::Internal => "internal",
        }
    }

    pub fn is_internal(&self) -> bool {
        !matches!(
            self,
            LvRole::Public | LvRole::ThinVolume | LvRole::CacheVolume | LvRole::Snapshot
        )
    }
}

impl LvMeta {
    pub fn is_visible(&self) -> bool {
        if self.role.is_internal() {
            return false;
        }
        self.status.is_empty() || self.status.iter().any(|status| status == "VISIBLE")
    }

    pub fn is_public(&self) -> bool {
        self.is_visible() && matches!(self.role, LvRole::Public)
    }

    pub fn is_directly_mappable(&self) -> bool {
        self.is_public()
            && self.segments.iter().all(|segment| {
                matches!(
                    segment.seg_type,
                    SegmentType::Linear | SegmentType::Striped { .. }
                ) && segment.has_only_data_areas()
            })
    }
}

#[derive(Debug, Clone)]
pub struct SegmentMeta {
    pub start_extent: u64,
    pub extent_count: u64,
    pub seg_type: SegmentType,
    /// Backward-compatible list of directly-addressed PV stripes.
    pub stripes: Vec<(String, u64)>,
    /// Complete area model, including component logical-volume references.
    pub areas: Vec<SegmentArea>,
    pub dependencies: SegmentDependencies,
}

impl SegmentMeta {
    pub(crate) fn has_only_data_areas(&self) -> bool {
        !self.areas.is_empty()
            && self.areas.iter().all(|area| {
                matches!(
                    area,
                    SegmentArea::PhysicalVolume { .. } | SegmentArea::LogicalVolume { .. }
                )
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentArea {
    PhysicalVolume { name: String, start_extent: u64 },
    LogicalVolume { name: String, start_extent: u64 },
    Unassigned { start_extent: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SegmentDependencies {
    pub raid_component_source: Option<RaidComponentSource>,
    pub raid_components: Vec<RaidComponent>,
    pub thin_pool: Option<String>,
    pub metadata: Option<String>,
    pub pool: Option<String>,
    pub data: Option<String>,
    pub origin: Option<String>,
    pub external_origin: Option<String>,
    pub cow_store: Option<String>,
    pub merging_store: Option<String>,
    pub cache_pool: Option<String>,
    pub transaction_id: Option<u64>,
    pub device_id: Option<u64>,
    pub chunk_size: Option<u64>,
    pub metadata_format: Option<u64>,
    pub metadata_start: Option<u64>,
    pub metadata_len: Option<u64>,
    pub data_start: Option<u64>,
    pub data_len: Option<u64>,
    pub metadata_id: Option<String>,
    pub data_id: Option<String>,
}

impl SegmentDependencies {
    pub(crate) fn referenced_lvs(&self) -> Vec<&str> {
        let mut refs = [
            self.thin_pool.as_deref(),
            self.metadata.as_deref(),
            self.pool.as_deref(),
            self.data.as_deref(),
            self.origin.as_deref(),
            self.external_origin.as_deref(),
            self.cow_store.as_deref(),
            self.merging_store.as_deref(),
            self.cache_pool.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        refs.extend(
            self.raid_components
                .iter()
                .flat_map(RaidComponent::referenced_lvs),
        );
        refs
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidComponentSource {
    Raid0Lvs,
    Raids,
    Stripes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidComponent {
    pub data_lv: String,
    pub metadata_lv: Option<String>,
}

impl RaidComponent {
    pub(crate) fn referenced_lvs(&self) -> impl Iterator<Item = &str> {
        self.metadata_lv
            .as_deref()
            .into_iter()
            .chain(std::iter::once(self.data_lv.as_str()))
    }
}

#[derive(Debug, Clone)]
pub enum SegmentType {
    Linear,
    Striped {
        stripe_count: u64,
        stripe_size: u64,
    },
    Raid0 {
        stripe_count: u64,
        stripe_size: u64,
    },
    Raid1 {
        mirror_count: u64,
    },
    Raid5 {
        stripe_count: u64,
    },
    Raid6 {
        stripe_count: u64,
    },
    Raid10 {
        stripe_count: u64,
        mirror_count: u64,
    },
    ThinVolume,
    ThinPool,
    Snapshot,
    CacheVolume,
    CachePool,
    Unsupported {
        type_name: String,
    },
}
