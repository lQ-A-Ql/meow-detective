use super::MountServiceError;
use domain::DataSourceKind;
use evidence_access::mdraid::{MdRaidArrayPlan, MdRaidMemberOrigin};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(super) fn evidence_files(
    path: &Path,
    kind: &DataSourceKind,
    case: &str,
) -> Result<Vec<(PathBuf, u64, SystemTime)>, MountServiceError> {
    let mut paths = Vec::new();
    collect(path, kind, case, 0, &mut paths)?;
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|path| {
            let metadata = std::fs::metadata(&path)?;
            Ok((path, metadata.len(), metadata.modified()?))
        })
        .collect()
}

fn collect(
    path: &Path,
    kind: &DataSourceKind,
    case: &str,
    depth: usize,
    paths: &mut Vec<PathBuf>,
) -> Result<(), MountServiceError> {
    if depth > 4 {
        return Err(MountServiceError::Unsupported(
            "MD RAID source nesting exceeds its bound".into(),
        ));
    }
    match kind {
        DataSourceKind::E01 => paths.extend(
            source_kernel::open_e01_reader_cached(path, case)?
                .open_plan()?
                .segment_paths(),
        ),
        DataSourceKind::MdRaid => {
            paths.push(path.canonicalize()?);
            let plan: MdRaidArrayPlan = serde_json::from_reader(std::fs::File::open(path)?)
                .map_err(|error| MountServiceError::Catalog(error.to_string()))?;
            for member in plan.members {
                match member.origin {
                    MdRaidMemberOrigin::Direct {
                        source_path,
                        source_kind,
                        ..
                    } => collect(
                        Path::new(&source_path),
                        &source_kind,
                        case,
                        depth + 1,
                        paths,
                    )?,
                    MdRaidMemberOrigin::Lvm { identity, .. } => {
                        for pv in identity.pv_sources {
                            let kind = pv.source_kind.ok_or_else(|| {
                                MountServiceError::SourceNotReady(
                                    "LVM PV source kind is missing".into(),
                                )
                            })?;
                            collect(Path::new(&pv.source_path), &kind, case, depth + 1, paths)?;
                        }
                    }
                }
            }
        }
        _ => paths.push(path.canonicalize()?),
    }
    Ok(())
}
