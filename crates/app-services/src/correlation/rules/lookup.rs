use super::super::helpers::normalized_path_suffix_key;
use super::matching::normalize_path;
use domain::FileEntry;
use segment_tree::{ops::Operation, SegmentPoint};
use std::collections::HashMap;

type Score = (u8, usize, usize);

struct ScoreMin;

impl Operation<Score> for ScoreMin {
    fn combine(&self, left: &Score, right: &Score) -> Score {
        (*left).min(*right)
    }
}

#[derive(Default)]
struct BestMatches([Option<Score>; 3]);

impl BestMatches {
    fn insert(&mut self, position: usize, file: &FileEntry) {
        for (index, slot) in self.0.iter_mut().enumerate() {
            let candidate = score(index, position, file);
            if slot.is_none_or(|current| candidate < current) {
                *slot = Some(candidate);
            }
        }
    }

    fn get(&self, preference: Option<bool>) -> Option<usize> {
        self.0[preference_index(preference)].map(|(_, _, position)| position)
    }
}

struct SuffixIndex {
    keys: Vec<Vec<u8>>,
    trees: Option<[SegmentPoint<Score, ScoreMin>; 3]>,
}

impl SuffixIndex {
    fn build(mut entries: Vec<(Vec<u8>, usize)>, files: &[FileEntry]) -> Self {
        entries.sort_unstable();
        let trees = (!entries.is_empty()).then(|| {
            std::array::from_fn(|preference| {
                let scores = entries
                    .iter()
                    .map(|(_, position)| score(preference, *position, &files[*position]))
                    .collect();
                SegmentPoint::build(scores, ScoreMin)
            })
        });
        Self {
            keys: entries.into_iter().map(|(key, _)| key).collect(),
            trees,
        }
    }

    fn best(&self, suffix: &str, preference: Option<bool>) -> Option<usize> {
        if suffix.is_empty() {
            return None;
        }
        let prefix = suffix.as_bytes().iter().rev().copied().collect::<Vec<_>>();
        let start = self
            .keys
            .partition_point(|key| key.as_slice() < prefix.as_slice());
        let end = start + self.keys[start..].partition_point(|key| key.starts_with(&prefix));
        if start == end {
            return None;
        }
        let trees = self.trees.as_ref()?;
        Some(
            trees[preference_index(preference)]
                .query_noiden(start, end)
                .2,
        )
    }
}

pub(crate) struct FilePathIndex {
    exact: HashMap<String, BestMatches>,
    suffix: SuffixIndex,
    basename: HashMap<String, BestMatches>,
}

impl FilePathIndex {
    fn build(files: &[FileEntry]) -> Self {
        let mut exact = HashMap::<String, BestMatches>::new();
        let mut basename = HashMap::<String, BestMatches>::new();
        let mut suffixes = Vec::new();
        for (position, file) in files.iter().enumerate().filter(|(_, file)| file.is_file()) {
            let normalized = normalize_path(&file.path);
            if !normalized.is_empty() {
                let suffix = normalized_path_suffix_key(&normalized);
                suffixes.push((suffix.as_bytes().iter().rev().copied().collect(), position));
                exact.entry(normalized).or_default().insert(position, file);
            }
            if !file.name.is_empty() {
                basename
                    .entry(file.name.to_ascii_lowercase())
                    .or_default()
                    .insert(position, file);
            }
        }
        Self {
            exact,
            suffix: SuffixIndex::build(suffixes, files),
            basename,
        }
    }

    pub(crate) fn exact_best(&self, path: &str, preference: Option<bool>) -> Option<usize> {
        self.exact.get(path)?.get(preference)
    }

    pub(crate) fn suffix_best(&self, path: &str, preference: Option<bool>) -> Option<usize> {
        self.suffix.best(path, preference)
    }

    pub(crate) fn basename_best(&self, name: &str, preference: Option<bool>) -> Option<usize> {
        self.basename.get(name)?.get(preference)
    }
}

pub(crate) struct FileLookup<'a> {
    files: &'a [FileEntry],
    index: FilePathIndex,
}

impl<'a> FileLookup<'a> {
    pub(crate) fn new(files: &'a [FileEntry]) -> Self {
        Self {
            files,
            index: FilePathIndex::build(files),
        }
    }

    pub(crate) fn file(&self, position: usize) -> &'a FileEntry {
        &self.files[position]
    }

    pub(crate) fn index(&self) -> &FilePathIndex {
        &self.index
    }
}

fn preference_index(preference: Option<bool>) -> usize {
    match preference {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}

fn score(preference: usize, position: usize, file: &FileEntry) -> Score {
    let mismatch = match preference {
        1 => !file.deleted,
        2 => file.deleted,
        _ => false,
    };
    (u8::from(mismatch), file.path.len(), position)
}
