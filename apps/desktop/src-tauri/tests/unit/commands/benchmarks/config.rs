use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Catalog {
    pub schema_version: u32,
    pub harness_version: String,
    pub scenario_names: Vec<String>,
    pub smoke: Vec<String>,
    pub levels: BTreeMap<String, Scale>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Scale {
    pub core_files: usize,
    pub clone_records: usize,
    pub timeline_records: usize,
    pub facet_records: usize,
    pub artifact_records: usize,
    pub correlation_files: usize,
    pub header_files: usize,
    pub search_sources: usize,
    pub search_files_per_source: usize,
}

pub(super) struct RunConfig {
    pub selected: Vec<String>,
    pub level: String,
    pub scale: Scale,
    pub runs: u32,
    pub warmups: u32,
}

impl Catalog {
    pub fn load() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/baselines/performance-benchmark-scenarios.json");
        let catalog: Self = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(catalog.schema_version, 1);
        assert_eq!(catalog.scenario_names.len(), 16);
        catalog
    }

    pub fn select(&self, requested: &str) -> Vec<String> {
        match requested {
            "all" => self.scenario_names.clone(),
            "smoke" => self.smoke.clone(),
            name if self.scenario_names.iter().any(|known| known == name) => vec![name.to_owned()],
            _ => panic!("unknown benchmark scenario: {requested}"),
        }
    }
}

impl RunConfig {
    pub fn from_env() -> Self {
        let catalog = Catalog::load();
        let requested = std::env::var("FORENSICS_BENCH_SCENARIO").unwrap_or_else(|_| "all".into());
        let selected = catalog.select(&requested);
        let level =
            std::env::var("FORENSICS_BENCH_FIXTURE_LEVEL").unwrap_or_else(|_| "small".into());
        let scale = catalog
            .levels
            .get(&level)
            .expect("unknown fixture level")
            .clone();
        Self {
            selected,
            level,
            scale,
            runs: integer_env("FORENSICS_BENCH_RUNS", 5, 1, 100),
            warmups: integer_env("FORENSICS_BENCH_WARMUP_RUNS", 2, 0, 10),
        }
    }
}

fn integer_env(name: &str, default: u32, min: u32, max: u32) -> u32 {
    let raw = match std::env::var(name) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return default,
        Err(error) => panic!("{name}: {error}"),
    };
    let value: u32 = raw
        .parse()
        .unwrap_or_else(|_| panic!("{name} must be an integer"));
    assert!(
        (min..=max).contains(&value),
        "{name} must be in {min}..={max}"
    );
    value
}

#[test]
fn benchmark_scenario_catalog_is_complete_and_unique() {
    let catalog = Catalog::load();
    let unique = catalog
        .scenario_names
        .iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), catalog.scenario_names.len());
    assert_eq!(catalog.select("all"), catalog.scenario_names);
    assert_eq!(catalog.select("smoke").len(), 6);
    for scenario in &catalog.scenario_names {
        assert_eq!(catalog.select(scenario), vec![scenario.clone()]);
    }
    for scale in catalog.levels.values() {
        assert!(scale.timeline_records >= 1000);
        assert!(scale.search_sources >= 8);
        assert!(scale.correlation_files >= 2000);
    }
}
