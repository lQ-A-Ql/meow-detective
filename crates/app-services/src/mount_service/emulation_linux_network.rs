use evidence_core::FileSystemReader;

pub(crate) fn linux_network_pci_slot(fs: &dyn FileSystemReader) -> Option<u16> {
    let mut names = Vec::new();
    for directory in [
        "etc/sysconfig/network-scripts",
        "etc/netplan",
        "etc/systemd/network",
    ] {
        let Ok(children) = fs.list_children(directory) else {
            continue;
        };
        for child in children {
            if child.is_dir {
                continue;
            }
            let lower = child.name.to_ascii_lowercase();
            let candidate = lower
                .strip_prefix("ifcfg-")
                .or_else(|| lower.strip_suffix(".yaml"))
                .or_else(|| lower.strip_suffix(".yml"))
                .or_else(|| lower.strip_suffix(".network"));
            if let Some(candidate) = candidate.filter(|name| !name.is_empty()) {
                names.push(candidate.to_string());
            }
            let path = format!("{directory}/{}", child.name);
            if let Ok(content) = fs.read_file_range(&path, 0, 64 * 1024) {
                names.extend(interface_names_from_config(&content));
            }
        }
    }
    if let Ok(content) = fs.read_file_range("etc/network/interfaces", 0, 64 * 1024) {
        names.extend(
            String::from_utf8_lossy(&content)
                .lines()
                .filter_map(|line| {
                    let mut fields = line.split_whitespace();
                    (fields.next() == Some("iface"))
                        .then(|| fields.next().map(str::to_ascii_lowercase))
                        .flatten()
                }),
        );
    }
    names.into_iter().find_map(predictable_network_pci_slot)
}

pub(crate) fn predictable_network_pci_slot(name: String) -> Option<u16> {
    let slot = name.strip_prefix("ens")?.parse::<u16>().ok()?;
    (slot > 0 && slot <= 255).then_some(slot)
}

fn interface_names_from_config(content: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(content)
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| token.len() > 3)
        .map(str::to_ascii_lowercase)
        .filter(|token| token.starts_with("ens"))
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/mount_service/emulation_linux_network.rs"]
mod tests;
