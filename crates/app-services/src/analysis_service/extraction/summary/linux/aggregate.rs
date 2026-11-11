use super::mapping::{LinuxCounts, LinuxEntries};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::AnalysisArtifactRow;
use crate::analysis_service::extraction::attr_mapping::{optional_string_attr, u64_attr};
use chrono::{DateTime, Utc};
use domain::Artifact;
use persistence_source::repositories::artifact_repo::ArtifactRepo;
use rusqlite::Connection;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashSet},
};

const MAX_LINUX_ACCOUNT_ROWS: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LinuxSummaryFamily {
    Journal,
    Login,
    BashCommand,
    AptEvent,
    CronJob,
    SudoEvent,
    SystemConfig,
    WebSite,
    WebAccessLog,
    WebErrorLog,
    WebFinding,
    MysqlConfig,
    MysqlLog,
    MysqlFinding,
}

impl LinuxSummaryFamily {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "LinuxJournal" => Some(Self::Journal),
            "LinuxWtmp" => Some(Self::Login),
            "LinuxBashCommand" => Some(Self::BashCommand),
            "LinuxAptEvent" => Some(Self::AptEvent),
            "LinuxCronJob" => Some(Self::CronJob),
            "LinuxSudoEvent" => Some(Self::SudoEvent),
            "LinuxSystemConfig" => Some(Self::SystemConfig),
            "LinuxWebSite" => Some(Self::WebSite),
            "LinuxWebAccessLog" => Some(Self::WebAccessLog),
            "LinuxWebErrorLog" => Some(Self::WebErrorLog),
            "LinuxWebFinding" => Some(Self::WebFinding),
            "LinuxMysqlConfig" => Some(Self::MysqlConfig),
            "LinuxMysqlLogEntry" => Some(Self::MysqlLog),
            "LinuxMysqlFinding" => Some(Self::MysqlFinding),
            _ => None,
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct PageKey(Reverse<DateTime<Utc>>, String);

struct RankedArtifact {
    key: PageKey,
    artifact: Artifact,
}

impl PartialEq for RankedArtifact {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for RankedArtifact {}

impl PartialOrd for RankedArtifact {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedArtifact {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

struct PageWindow {
    size: usize,
    artifacts: BinaryHeap<RankedArtifact>,
}

impl PageWindow {
    fn new(size: usize) -> Self {
        Self {
            size,
            artifacts: BinaryHeap::new(),
        }
    }

    fn push(&mut self, artifact: Artifact) {
        if self.size == 0 {
            return;
        }
        let candidate = RankedArtifact {
            key: PageKey(Reverse(artifact.created_at), artifact.id.0.clone()),
            artifact,
        };
        if self.artifacts.len() < self.size {
            self.artifacts.push(candidate);
        } else if self
            .artifacts
            .peek()
            .is_some_and(|oldest| candidate.key < oldest.key)
        {
            self.artifacts.pop();
            self.artifacts.push(candidate);
        }
    }

    fn finish(self, offset: usize) -> Vec<AnalysisArtifactRow> {
        self.artifacts
            .into_sorted_vec()
            .into_iter()
            .skip(offset)
            .map(|ranked| artifact_row(ranked.artifact))
            .collect()
    }
}

pub(super) struct LinuxSystemInputs {
    pub(super) os_release: Option<Artifact>,
    pub(super) hostname: Option<(u64, String)>,
    pub(super) accounts: Vec<Artifact>,
    pub(super) account_count: u64,
    pub(super) user_account_count: u64,
    pub(super) locked_account_count: u64,
}

impl LinuxSystemInputs {
    fn observe(&mut self, artifact: &Artifact) -> Result<(), AnalysisServiceError> {
        if artifact.family != "LinuxSystemConfig"
            || artifact
                .attrs
                .get("overlayContext")
                .is_some_and(|value| !value.is_null())
        {
            return Ok(());
        }
        if is_hostname_artifact(artifact) {
            self.observe_hostname(artifact);
        }
        match optional_string_attr(&artifact.attrs, "configKind").as_deref() {
            Some("osRelease") => self.observe_os_release(artifact),
            Some("passwdAccount") => {
                self.account_count = self.account_count.saturating_add(1);
                if artifact_u32_attr(artifact, "uid").is_some_and(|uid| uid >= 1000) {
                    self.user_account_count = self.user_account_count.saturating_add(1);
                }
                self.push_account(artifact)?;
            }
            Some("shadowAccount") => {
                if artifact
                    .attrs
                    .get("locked")
                    .is_some_and(|value| value.as_bool() == Some(true) || value.as_i64() == Some(1))
                {
                    self.locked_account_count = self.locked_account_count.saturating_add(1);
                }
                self.push_account(artifact)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn observe_os_release(&mut self, artifact: &Artifact) {
        let replace = self
            .os_release
            .as_ref()
            .is_none_or(|current| os_release_rank(artifact) < os_release_rank(current));
        if replace {
            self.os_release = Some(artifact.clone());
        }
    }

    fn observe_hostname(&mut self, artifact: &Artifact) {
        let Some(line) = optional_string_attr(&artifact.attrs, "line")
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
        else {
            return;
        };
        let line_number = u64_attr(&artifact.attrs, "lineNumber");
        if self
            .hostname
            .as_ref()
            .is_none_or(|(current_line, _)| line_number < *current_line)
        {
            self.hostname = Some((line_number, line));
        }
    }

    fn push_account(&mut self, artifact: &Artifact) -> Result<(), AnalysisServiceError> {
        if self.accounts.len() >= MAX_LINUX_ACCOUNT_ROWS {
            return Err(AnalysisServiceError::Other(
                "Linux account summary exceeds the bounded row limit".into(),
            ));
        }
        self.accounts.push(artifact.clone());
        Ok(())
    }
}

pub(super) struct LinuxSummaryAggregate {
    pub(super) counts: LinuxCounts,
    pub(super) entries: LinuxEntries,
    pub(super) system_inputs: LinuxSystemInputs,
    pub(super) parsed_source_ids: HashSet<String>,
}

struct LinuxSummaryAccumulator {
    offset: usize,
    page_size: usize,
    counts: LinuxCounts,
    windows: BTreeMap<LinuxSummaryFamily, PageWindow>,
    system_inputs: LinuxSystemInputs,
    parsed_source_ids: HashSet<String>,
}

impl LinuxSummaryAccumulator {
    fn new(offset: u64, limit: u32) -> Result<Self, AnalysisServiceError> {
        let offset = usize::try_from(offset).map_err(|_| {
            AnalysisServiceError::InvalidInput("artifact offset exceeds platform limits".into())
        })?;
        let window_size = if limit == 0 {
            0
        } else {
            offset.checked_add(limit as usize).ok_or_else(|| {
                AnalysisServiceError::InvalidInput(
                    "artifact page range exceeds platform limits".into(),
                )
            })?
        };
        Ok(Self {
            offset,
            page_size: window_size,
            counts: LinuxCounts::default(),
            windows: BTreeMap::new(),
            system_inputs: LinuxSystemInputs {
                os_release: None,
                hostname: None,
                accounts: Vec::new(),
                account_count: 0,
                user_account_count: 0,
                locked_account_count: 0,
            },
            parsed_source_ids: HashSet::new(),
        })
    }

    fn push(&mut self, artifact: Artifact) -> Result<(), AnalysisServiceError> {
        let Some(family) = LinuxSummaryFamily::from_name(&artifact.family) else {
            return Ok(());
        };
        self.counts.record(&artifact);
        if let Some(source_id) = &artifact.source_object_id {
            self.parsed_source_ids.insert(source_id.0.clone());
        }
        self.system_inputs.observe(&artifact)?;
        self.windows
            .entry(family)
            .or_insert_with(|| PageWindow::new(self.page_size))
            .push(artifact);
        Ok(())
    }

    fn finish(self) -> LinuxSummaryAggregate {
        let mut pages = self
            .windows
            .into_iter()
            .map(|(family, window)| (family, window.finish(self.offset)))
            .collect();
        LinuxSummaryAggregate {
            counts: self.counts,
            entries: LinuxEntries::from_pages(&mut pages),
            system_inputs: self.system_inputs,
            parsed_source_ids: self.parsed_source_ids,
        }
    }
}

impl LinuxSummaryAggregate {
    pub(super) fn load(
        conn: &Connection,
        offset: u64,
        limit: u32,
    ) -> Result<Self, AnalysisServiceError> {
        let mut accumulator = LinuxSummaryAccumulator::new(offset, limit)?;
        ArtifactRepo::new(conn).visit_by_family(None, |artifact| {
            accumulator
                .push(artifact)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        })?;
        Ok(accumulator.finish())
    }
}

fn artifact_row(artifact: Artifact) -> AnalysisArtifactRow {
    AnalysisArtifactRow {
        id: artifact.id.0,
        source_object_id: artifact.source_object_id.map(|id| id.0),
        extractor_id: artifact.extractor_id,
        created_at: artifact.created_at.to_rfc3339(),
        attrs: artifact.attrs,
    }
}

fn os_release_rank(artifact: &Artifact) -> (u8, String) {
    let path = optional_string_attr(&artifact.attrs, "sourcePath").unwrap_or_default();
    let normalized = path.trim_start_matches('/');
    let rank = if normalized == "etc/os-release" || normalized.ends_with("/etc/os-release") {
        0
    } else if normalized == "usr/lib/os-release" || normalized.ends_with("/usr/lib/os-release") {
        1
    } else {
        2
    };
    (rank, path)
}

fn is_hostname_artifact(artifact: &Artifact) -> bool {
    let path = optional_string_attr(&artifact.attrs, "sourcePath").unwrap_or_default();
    let normalized = path.trim_start_matches('/');
    normalized == "etc/hostname" || normalized.ends_with("/etc/hostname")
}

fn artifact_u32_attr(artifact: &Artifact, key: &str) -> Option<u32> {
    artifact.attrs.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.parse().ok())
            .and_then(|value| u32::try_from(value).ok())
    })
}
