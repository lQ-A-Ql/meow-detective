use domain::{EntryType, FileEntry};
use std::cmp::Ordering as CmpOrdering;
use transport::commands::{FileSortDirectionDto, FileSortKeyDto};

fn entry_status_bucket(entry: &FileEntry) -> u8 {
    let abnormal = entry.hidden || entry.system;
    match (abnormal, entry.deleted) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    }
}

fn entry_type_rank(entry: &FileEntry) -> u8 {
    match entry.entry_type {
        EntryType::Directory => 0,
        EntryType::File => 1,
    }
}

pub(crate) fn natural_cmp(a: &str, b: &str) -> CmpOrdering {
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return CmpOrdering::Equal,
            (None, Some(_)) => return CmpOrdering::Less,
            (Some(_), None) => return CmpOrdering::Greater,
            (Some(ca), Some(cb)) => {
                if ca.is_ascii_digit() && cb.is_ascii_digit() {
                    let na = take_digit_run(&mut ai);
                    let nb = take_digit_run(&mut bi);
                    match compare_digit_runs(&na, &nb) {
                        CmpOrdering::Equal => continue,
                        other => return other,
                    }
                } else {
                    let la = ca.to_ascii_lowercase();
                    let lb = cb.to_ascii_lowercase();
                    match la.cmp(&lb) {
                        CmpOrdering::Equal => match ca.cmp(&cb) {
                            CmpOrdering::Equal => {
                                ai.next();
                                bi.next();
                                continue;
                            }
                            other => return other,
                        },
                        other => return other,
                    }
                }
            }
        }
    }
}

fn take_digit_run(iter: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut run = String::new();
    while let Some(ch) = iter.peek().copied() {
        if ch.is_ascii_digit() {
            run.push(ch);
            iter.next();
        } else {
            break;
        }
    }
    run
}

fn compare_digit_runs(a: &str, b: &str) -> CmpOrdering {
    let a_trim = a.trim_start_matches('0');
    let b_trim = b.trim_start_matches('0');
    match a_trim.len().cmp(&b_trim.len()) {
        CmpOrdering::Equal => match a_trim.cmp(b_trim) {
            CmpOrdering::Equal => a.len().cmp(&b.len()),
            other => other,
        },
        other => other,
    }
}

fn name_cmp(a: &FileEntry, b: &FileEntry) -> CmpOrdering {
    natural_cmp(&a.name, &b.name)
}

fn compare_entries(
    a: &FileEntry,
    b: &FileEntry,
    sort_key: FileSortKeyDto,
    direction: FileSortDirectionDto,
) -> CmpOrdering {
    let type_cmp = entry_type_rank(a).cmp(&entry_type_rank(b));
    if type_cmp != CmpOrdering::Equal {
        return type_cmp;
    }

    let status_cmp = entry_status_bucket(a).cmp(&entry_status_bucket(b));
    if status_cmp != CmpOrdering::Equal {
        return status_cmp;
    }

    let key_cmp = match sort_key {
        FileSortKeyDto::Name => name_cmp(a, b),
        FileSortKeyDto::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
        FileSortKeyDto::ModifiedAt => a.modified_at.cmp(&b.modified_at),
        FileSortKeyDto::Ext => {
            let ea = a.ext.as_deref().unwrap_or("").to_ascii_lowercase();
            let eb = b.ext.as_deref().unwrap_or("").to_ascii_lowercase();
            natural_cmp(&ea, &eb)
        }
    };
    let key_cmp = match direction {
        FileSortDirectionDto::Asc => key_cmp,
        FileSortDirectionDto::Desc => key_cmp.reverse(),
    };
    if key_cmp != CmpOrdering::Equal {
        return key_cmp;
    }

    name_cmp(a, b)
}

pub(crate) fn sort_entries(
    entries: &mut [FileEntry],
    sort_key: FileSortKeyDto,
    direction: FileSortDirectionDto,
) {
    entries.sort_by(|a, b| compare_entries(a, b, sort_key, direction));
}

pub(crate) fn sort_directories_for_tree(entries: &mut [FileEntry]) {
    entries.sort_by(name_cmp);
}
