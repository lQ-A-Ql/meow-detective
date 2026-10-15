//! Resolve offline unit files exclusively through a caller-owned evidence catalog.
use super::{parse_systemd_unit_with_drop_ins, syntax::error, LinuxServiceUnit, SystemdDropIn};
use crate::LinuxArtifactError;
use std::collections::{BTreeMap, BTreeSet};
mod drop_ins;
pub(super) const ROOTS: &[&str] = &[
    "/etc/systemd/system.control",
    "/run/systemd/system.control",
    "/run/systemd/transient",
    "/run/systemd/generator.early",
    "/etc/systemd/system",
    "/etc/systemd/system.attached",
    "/run/systemd/system",
    "/run/systemd/system.attached",
    "/run/systemd/generator",
    "/usr/local/lib/systemd/system",
    "/usr/lib/systemd/system",
    "/lib/systemd/system",
    "/run/systemd/generator.late",
];
#[derive(Debug, Clone)]
pub struct SystemdUnitFile {
    pub path: String,
    pub is_symlink: bool,
}
pub struct SystemdUnitCatalog {
    files: BTreeMap<String, bool>,
    directories: BTreeMap<String, Vec<String>>,
}
pub struct SystemdUnitLinks {
    by_target: BTreeMap<String, Vec<(String, Vec<String>)>>,
}
impl SystemdUnitCatalog {
    pub fn new(
        files: impl IntoIterator<Item = SystemdUnitFile>,
    ) -> Result<Self, LinuxArtifactError> {
        let mut result = Self {
            files: BTreeMap::new(),
            directories: BTreeMap::new(),
        };
        for file in files {
            if result.files.len() >= 32768 || file.path.len() > 4096 || !file.path.starts_with('/')
            {
                return Err(error("unit catalog exceeds path or entry limits"));
            }
            let path = normalize_path("/", &file.path)?;
            if result.files.insert(path.clone(), file.is_symlink).is_some() {
                return Err(error("duplicate unit catalog path"));
            }
            if let Some((parent, _)) = path.rsplit_once('/') {
                result
                    .directories
                    .entry(parent.into())
                    .or_default()
                    .push(path);
            }
        }
        Ok(result)
    }
    pub fn unit_names(&self) -> BTreeSet<String> {
        self.files
            .keys()
            .filter_map(|path| {
                let (_, relative) = location(path)?;
                let name = relative.rsplit('/').next()?;
                (unit_name(name)
                    && (!relative.contains('/')
                        || relative.contains(".wants/")
                        || relative.contains(".requires/")))
                .then(|| name.into())
            })
            .collect()
    }
    pub fn resolve(
        &self,
        name: &str,
        mut read: impl FnMut(&str, usize) -> Result<Vec<u8>, LinuxArtifactError>,
    ) -> Result<LinuxServiceUnit, LinuxArtifactError> {
        let links = self.prepare_links(&mut read);
        self.resolve_with_links(name, &links, read)
    }
    pub fn prepare_links(
        &self,
        mut read: impl FnMut(&str, usize) -> Result<Vec<u8>, LinuxArtifactError>,
    ) -> SystemdUnitLinks {
        let mut by_target = BTreeMap::<String, Vec<(String, Vec<String>)>>::new();
        for (path, is_link) in &self.files {
            if !is_link || location(path).is_none() {
                continue;
            }
            if let Ok((target, trace)) = self.resolve_path(path, &mut read) {
                by_target
                    .entry(target)
                    .or_default()
                    .push((path.clone(), trace));
            }
        }
        SystemdUnitLinks { by_target }
    }
    pub fn resolve_with_links(
        &self,
        name: &str,
        links: &SystemdUnitLinks,
        mut read: impl FnMut(&str, usize) -> Result<Vec<u8>, LinuxArtifactError>,
    ) -> Result<LinuxServiceUnit, LinuxArtifactError> {
        if !unit_name(name) {
            return Err(error("invalid unit name"));
        }
        let path = self
            .find(name)
            .or_else(|| template(name).and_then(|template| self.find(&template)))
            .ok_or_else(|| error("unit definition is missing"))?;
        let (resolved, mut sources) = self.resolve_path(&path, &mut read)?;
        if resolved == "/dev/null" {
            let mut unit = super::parse_systemd_unit(&path, b"/dev/null")?;
            unit.name = name.into();
            unit.source_paths = sources;
            return Ok(unit);
        }
        let bytes = read(&resolved, super::MAX_UNIT_BYTES + 1)?;
        if bytes.len() > super::MAX_UNIT_BYTES {
            return Err(error("unit file exceeds read limit"));
        }
        if std::str::from_utf8(&bytes).is_ok_and(|text| text.trim() == "/dev/null") {
            return Err(error("mask target has no symlink provenance"));
        }
        let canonical = resolved
            .rsplit('/')
            .next()
            .ok_or_else(|| error("invalid resolved unit path"))?;
        let effective_name = if canonical.contains("@.") && !name.contains("@.") {
            name
        } else {
            canonical
        };
        let (aliases, enablement, proofs) = source_links(links, &resolved, effective_name);
        sources.extend(proofs);
        let mut names = aliases.clone();
        names.push(canonical.into());
        names.push(effective_name.into());
        let drop_paths = drop_ins::select(self, &names)?;
        let mut drop_bytes = Vec::new();
        let mut applied = Vec::new();
        let mut total = bytes.len();
        for drop_path in drop_paths {
            let (target, trace) = self.resolve_path(&drop_path, &mut read)?;
            sources.extend(trace);
            if target == "/dev/null" {
                continue;
            }
            let data = read(&target, super::MAX_UNIT_BYTES + 1)?;
            total = total
                .checked_add(data.len())
                .ok_or_else(|| error("unit size overflow"))?;
            if total > super::MAX_UNIT_BYTES {
                return Err(error("merged unit exceeds 1 MiB limit"));
            }
            applied.push(drop_path);
            drop_bytes.push(data);
        }
        let drops = applied
            .iter()
            .zip(&drop_bytes)
            .map(|(path, bytes)| SystemdDropIn { path, bytes })
            .collect::<Vec<_>>();
        let mut unit = parse_systemd_unit_with_drop_ins(&resolved, &bytes, &drops)?;
        unit.name = effective_name.into();
        unit.enabled = !unit.masked && !enablement.is_empty();
        unit.enablement_symlink = enablement.first().cloned();
        unit.state = if unit.masked {
            "masked"
        } else if unit.enabled {
            "enabled"
        } else if unit.static_unit {
            "static"
        } else {
            "disabled"
        }
        .into();
        unit.aliases.extend(aliases);
        unit.aliases.sort();
        unit.aliases.dedup();
        sources.push(resolved);
        sources.extend(applied);
        sources.sort();
        sources.dedup();
        unit.source_paths = sources;
        Ok(unit)
    }
    fn find(&self, name: &str) -> Option<String> {
        ROOTS
            .iter()
            .map(|root| format!("{root}/{name}"))
            .find(|path| self.files.contains_key(path))
    }
    fn resolve_path(
        &self,
        start: &str,
        read: &mut impl FnMut(&str, usize) -> Result<Vec<u8>, LinuxArtifactError>,
    ) -> Result<(String, Vec<String>), LinuxArtifactError> {
        let mut current = start.to_owned();
        let mut visited = BTreeSet::new();
        let mut trace = Vec::new();
        for _ in 0..32 {
            if !visited.insert(current.clone()) {
                return Err(error("unit symlink cycle"));
            }
            if current == "/dev/null" {
                return Ok((current, trace));
            }
            let components = current
                .split('/')
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            let mut replaced = false;
            for count in 1..=components.len() {
                let prefix = format!("/{}", components[..count].join("/"));
                if self.files.get(&prefix) != Some(&true) {
                    continue;
                }
                let data = read(&prefix, 4097)?;
                if data.len() > 4096 {
                    return Err(error("symlink target exceeds limit"));
                }
                let target = std::str::from_utf8(&data)
                    .map_err(|_| error("symlink target is not UTF-8"))?
                    .trim_end_matches('\0');
                if target.is_empty() || target.contains('\0') {
                    return Err(error("invalid symlink target"));
                }
                let parent = prefix
                    .rsplit_once('/')
                    .map(|(parent, _)| parent)
                    .unwrap_or("/");
                let resolved = normalize_path(parent, target)?;
                let suffix = components[count..].join("/");
                current = if suffix.is_empty() {
                    resolved
                } else {
                    format!("{resolved}/{suffix}")
                };
                trace.push(prefix);
                replaced = true;
                break;
            }
            if replaced {
                continue;
            }
            if !self.files.contains_key(&current) {
                return Err(error("dangling unit symlink"));
            }
            trace.push(current.clone());
            return Ok((current, trace));
        }
        Err(error("unit symlink depth exceeds limit"))
    }
}
fn normalize_path(parent: &str, target: &str) -> Result<String, LinuxArtifactError> {
    let combined = if target.starts_with('/') {
        target.to_owned()
    } else {
        format!("{parent}/{target}")
    };
    let mut parts = Vec::new();
    for part in combined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(error("symlink escapes evidence root"));
                }
            }
            part => parts.push(part),
        }
    }
    let path = format!("/{}", parts.join("/"));
    if path.len() > 4096 {
        return Err(error("resolved path exceeds limit"));
    }
    Ok(path)
}
fn template(name: &str) -> Option<String> {
    let (prefix, rest) = name.split_once('@')?;
    let (_, suffix) = rest.rsplit_once('.')?;
    Some(format!("{prefix}@.{suffix}"))
}
fn instance(name: &str) -> Option<&str> {
    let (_, rest) = name.split_once('@')?;
    rest.rsplit_once('.').map(|(instance, _)| instance)
}
fn unit_name(name: &str) -> bool {
    name.len() <= 255
        && !name.contains('/')
        && name.rsplit_once('.').is_some_and(|(_, kind)| {
            matches!(
                kind,
                "service"
                    | "socket"
                    | "target"
                    | "timer"
                    | "path"
                    | "mount"
                    | "automount"
                    | "swap"
                    | "slice"
                    | "scope"
                    | "device"
            )
        })
}
fn location(path: &str) -> Option<(usize, &str)> {
    ROOTS.iter().enumerate().find_map(|(rank, root)| {
        path.strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
            .map(|rest| (rank, rest))
    })
}

fn source_links(
    links: &SystemdUnitLinks,
    resolved: &str,
    effective_name: &str,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut proofs = Vec::new();
    let mut aliases = Vec::new();
    let mut enablement = Vec::new();
    for (link, trace) in links.by_target.get(resolved).into_iter().flatten() {
        if link.contains(".wants/") || link.contains(".requires/") {
            let enabled_instance = link.rsplit('/').next().and_then(instance);
            if effective_name.contains("@.") || enabled_instance == instance(effective_name) {
                enablement.push(link.clone());
                proofs.extend(trace.iter().cloned());
            }
        } else if let Some((_, relative)) = location(link) {
            if !relative.contains('/') && unit_name(relative) {
                aliases.push(relative.into());
                proofs.extend(trace.iter().cloned());
            }
        }
    }
    (aliases, enablement, proofs)
}
