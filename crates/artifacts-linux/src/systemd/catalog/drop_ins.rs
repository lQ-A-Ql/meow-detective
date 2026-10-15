use super::{template, SystemdUnitCatalog, ROOTS};
use crate::LinuxArtifactError;
use std::collections::{BTreeMap, BTreeSet};
pub(super) fn select(
    catalog: &SystemdUnitCatalog,
    names: &[String],
) -> Result<Vec<String>, LinuxArtifactError> {
    let mut directories = BTreeMap::<String, usize>::new();
    for name in names {
        let Some((stem, kind)) = name.rsplit_once('.') else {
            continue;
        };
        directories.insert(format!("{kind}.d"), 0);
        for (index, _) in stem.match_indices('-') {
            directories.insert(format!("{}-.{kind}.d", &stem[..index]), index + 1);
        }
        if let Some(template) = template(name) {
            directories.insert(format!("{template}.d"), 1024);
        }
        directories.insert(format!("{name}.d"), 2048);
    }
    let mut chosen = BTreeMap::<String, (usize, usize, String)>::new();
    for (directory, specificity) in directories {
        for (rank, root) in ROOTS.iter().enumerate() {
            let Some(paths) = catalog.directories.get(&format!("{root}/{directory}")) else {
                continue;
            };
            for path in paths {
                let name = path.rsplit('/').next().unwrap_or_default();
                if !name.ends_with(".conf") {
                    continue;
                }
                let replace = chosen
                    .get(name)
                    .is_none_or(|(old_specificity, old_rank, _)| {
                        // Name-specific directories always override type.d;
                        // within that class the lookup directory precedes
                        // progressively less specific name prefixes.
                        (specificity > 0 && *old_specificity == 0)
                            || ((specificity == 0) == (*old_specificity == 0)
                                && (rank < *old_rank
                                    || (rank == *old_rank && specificity > *old_specificity)))
                    });
                if replace {
                    chosen.insert(name.into(), (specificity, rank, path.clone()));
                }
            }
        }
    }
    let paths = chosen
        .into_values()
        .map(|(_, _, path)| path)
        .collect::<Vec<_>>();
    if paths.len() > 128 || names.iter().collect::<BTreeSet<_>>().len() > 256 {
        return Err(super::error("drop-in or alias count exceeds limit"));
    }
    Ok(paths)
}
