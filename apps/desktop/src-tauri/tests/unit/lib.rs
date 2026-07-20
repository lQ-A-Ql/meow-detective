use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[cfg(windows)]
#[link(name = "resource", kind = "static")]
// SAFETY: The block declares no foreign functions; it only links Tauri's generated
// Windows resource into this unit-test harness.
unsafe extern "C" {}

#[test]
fn every_tauri_command_is_registered_in_invoke_handler() {
    let defined = collect_command_function_names();
    let registered = collect_registered_command_names();
    let missing: Vec<_> = defined.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "#[tauri::command] functions missing from invoke_handler: {:?}",
        missing
    );
}

fn collect_command_function_names() -> HashSet<String> {
    let mut names = HashSet::new();
    read_commands_dir(Path::new("src/commands"), &mut names);
    names
}

fn read_commands_dir(dir: &Path, names: &mut HashSet<String>) {
    for entry in fs::read_dir(dir).expect("commands directory") {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.is_dir() {
            read_commands_dir(&path, names);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let text = fs::read_to_string(&path).expect("command source");
            collect_from_source(&text, names);
        }
    }
}

fn collect_from_source(text: &str, names: &mut HashSet<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();
        if line.starts_with("#[tauri::command") {
            i += 1;
            while i < lines.len() {
                let fn_line = lines[i].trim();
                if fn_line.is_empty() || fn_line.starts_with("//") {
                    i += 1;
                    continue;
                }
                if let Some(rest) = fn_line.strip_prefix("pub async fn ") {
                    let name = rest
                        .split('(')
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap();
                    names.insert(name.to_string());
                    break;
                }
                if let Some(rest) = fn_line.strip_prefix("pub fn ") {
                    let name = rest
                        .split('(')
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .next()
                        .unwrap();
                    names.insert(name.to_string());
                    break;
                }
                i += 1;
            }
        }
        i += 1;
    }
}

fn collect_registered_command_names() -> HashSet<String> {
    let lib = fs::read_to_string("src/lib.rs").expect("lib.rs");
    let marker = "tauri::generate_handler![";
    let start = lib.find(marker).expect("generate_handler macro") + marker.len();
    let end = lib[start..].find(']').expect("closing bracket") + start;
    let block = &lib[start..end];
    let cleaned: String = block
        .lines()
        .map(|line| {
            let mut s = line.to_string();
            if let Some(idx) = s.find("//") {
                s.truncate(idx);
            }
            s
        })
        .collect();
    let mut names = HashSet::new();
    for token in cleaned.split(',') {
        let token = token.trim();
        if !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            names.insert(token.to_string());
        }
    }
    names
}
