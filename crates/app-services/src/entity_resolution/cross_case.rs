//! Cross-case entity matching — compares resolved entities across multiple
//! case databases to identify the same real-world actor, device, or account
//! appearing in separate investigations.
//!
//! Phase 3 of entity resolution: after intra-case canonicalization/merge
//! (phase 1) and relationship inference (phase 2), this engine reads
//! `resolved_entities` from two or more case databases and matches entities
//! that share a canonical value and entity type.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// How two or more entities from different cases were matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchStrategy {
    /// Same canonical value AND same entity type across cases.
    Exact,
    /// Same entity type and values match after secondary normalization.
    Normalized,
    /// Same entity type only — no value match; flagged for analyst review.
    Fuzzy,
}

/// A cross-case entity match grouping one or more resolved entities from
/// different cases that are likely the same real-world actor, device, or
/// account.
#[derive(Debug, Clone)]
pub struct CrossCaseMatch {
    /// Stable deterministic ID for this match group.
    pub id: String,
    /// (case_id, entity_id, canonical_value) for each matching entity.
    pub entities: Vec<(String, String, String)>,
    /// The entity type shared by all entities in this match (person, account, device, …).
    pub entity_type: String,
    /// Confidence score 0.0–1.0.
    pub confidence: f64,
    /// How the match was produced.
    pub match_strategy: MatchStrategy,
}

/// Cross-case entity matcher that opens case databases, reads their
/// resolved entities, and returns groups of entities that match across
/// cases by canonical value and entity type.
pub struct CrossCaseEntityMatcher;

impl CrossCaseEntityMatcher {
    /// Open two or more case databases, read their `resolved_entities`
    /// tables, and return cross-case entity matches.
    ///
    /// Matching strategy (in order of decreasing confidence):
    ///
    /// | Strategy   | Condition                                     | Confidence |
    /// |------------|-----------------------------------------------|------------|
    /// | Exact      | Same canonical value + same entity type        | 0.95       |
    /// | Normalized | Same entity type + secondary normalization     | 0.85       |
    /// | Fuzzy      | Same entity type only (no value match)         | 0.50       |
    ///
    /// Entities that already matched in a higher-confidence tier are
    /// excluded from lower tiers (each entity node participates in at most
    /// one cross-case match).
    ///
    /// # Errors
    ///
    /// Returns a string error if a database cannot be opened or queried,
    /// or if fewer than 2 database paths are provided.
    pub fn match_entities_across_cases(
        db_paths: &[PathBuf],
    ) -> Result<Vec<CrossCaseMatch>, String> {
        if db_paths.len() < 2 {
            return Err("cross_case: at least 2 database paths are required".into());
        }

        // ── Step 1: Read resolved_entities from every database ──────
        // Each entry: (case_id, entity_id, canonical_value, entity_type, db_index)
        let mut all_entities: Vec<(String, String, String, String, usize)> = Vec::new();

        for (db_idx, path) in db_paths.iter().enumerate() {
            let conn = persistence_sqlite::connection::open_existing(path)
                .map_err(|e| format!("failed to open {}: {e}", path.display()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, case_id, entity_type, canonical_value
                     FROM resolved_entities",
                )
                .map_err(|e| format!("failed to query resolved_entities: {e}"))?;

            let rows: Vec<(String, String, String, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;

            for (entity_id, case_id, entity_type, canonical_value) in rows {
                all_entities.push((case_id, entity_id, canonical_value, entity_type, db_idx));
            }
        }

        if all_entities.is_empty() {
            return Ok(Vec::new());
        }

        // ── Step 2: Exact match on (canonical_value, entity_type) ───
        let mut exact_matches: Vec<CrossCaseMatch> = Vec::new();
        let mut matched_entity_ids: HashSet<(usize, String)> = HashSet::new();
        // Track (db_index, entity_id) of entities that have been claimed.

        // Group by (canonical_value, entity_type) key
        let mut exact_groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (idx, ent) in all_entities.iter().enumerate() {
            let key = (ent.2.clone(), ent.3.clone()); // (canonical_value, entity_type)
            exact_groups.entry(key).or_default().push(idx);
        }

        for ((canonical_value, entity_type), indices) in &exact_groups {
            // Only groups that span 2+ different databases are cross-case matches
            let db_indices: HashSet<usize> = indices.iter().map(|&i| all_entities[i].4).collect();
            if db_indices.len() < 2 {
                continue;
            }

            let entities: Vec<(String, String, String)> = indices
                .iter()
                .map(|&i| {
                    let ent = &all_entities[i];
                    (ent.0.clone(), ent.1.clone(), ent.2.clone())
                })
                .collect();

            let id = Self::cross_case_match_id(entity_type, canonical_value, &MatchStrategy::Exact);

            // Mark all these entities as claimed
            for &i in indices {
                let ent = &all_entities[i];
                matched_entity_ids.insert((ent.4, ent.1.clone()));
            }

            exact_matches.push(CrossCaseMatch {
                id,
                entities,
                entity_type: entity_type.clone(),
                confidence: 0.95,
                match_strategy: MatchStrategy::Exact,
            });
        }

        // ── Step 3: Normalized match (remaining entities) ────────────
        // Secondary normalization: for emails strip domain; for accounts
        // strip domain\ prefix; then compare.
        let mut normalized_matches: Vec<CrossCaseMatch> = Vec::new();

        // Collect remaining entities (not claimed by exact match)
        let remaining: Vec<usize> = (0..all_entities.len())
            .filter(|&i| {
                let ent = &all_entities[i];
                !matched_entity_ids.contains(&(ent.4, ent.1.clone()))
            })
            .collect();

        if !remaining.is_empty() {
            let mut norm_groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
            for &idx in &remaining {
                let ent = &all_entities[idx];
                let normalized = Self::secondary_normalize(&ent.2, &ent.3);
                let key = (normalized, ent.3.clone()); // (normalized_value, entity_type)
                norm_groups.entry(key).or_default().push(idx);
            }

            for ((_normalized_value, entity_type), indices) in &norm_groups {
                let db_indices: HashSet<usize> =
                    indices.iter().map(|&i| all_entities[i].4).collect();
                if db_indices.len() < 2 {
                    continue;
                }

                let entities: Vec<(String, String, String)> = indices
                    .iter()
                    .map(|&i| {
                        let ent = &all_entities[i];
                        (ent.0.clone(), ent.1.clone(), ent.2.clone())
                    })
                    .collect();

                let id = Self::cross_case_match_id(
                    entity_type,
                    &entities[0].2,
                    &MatchStrategy::Normalized,
                );

                // Mark claimed
                for &i in indices {
                    let ent = &all_entities[i];
                    matched_entity_ids.insert((ent.4, ent.1.clone()));
                }

                normalized_matches.push(CrossCaseMatch {
                    id,
                    entities,
                    entity_type: entity_type.clone(),
                    confidence: 0.85,
                    match_strategy: MatchStrategy::Normalized,
                });
            }
        }

        // ── Step 4: Fuzzy match (same entity type only, remaining) ───
        let mut fuzzy_matches: Vec<CrossCaseMatch> = Vec::new();

        let still_remaining: Vec<usize> = (0..all_entities.len())
            .filter(|&i| {
                let ent = &all_entities[i];
                !matched_entity_ids.contains(&(ent.4, ent.1.clone()))
            })
            .collect();

        if !still_remaining.is_empty() {
            let mut type_groups: HashMap<String, Vec<usize>> = HashMap::new();
            for &idx in &still_remaining {
                let ent = &all_entities[idx];
                type_groups.entry(ent.3.clone()).or_default().push(idx);
            }

            for (entity_type, indices) in &type_groups {
                let db_indices: HashSet<usize> =
                    indices.iter().map(|&i| all_entities[i].4).collect();
                if db_indices.len() < 2 {
                    continue;
                }

                let entities: Vec<(String, String, String)> = indices
                    .iter()
                    .map(|&i| {
                        let ent = &all_entities[i];
                        (ent.0.clone(), ent.1.clone(), ent.2.clone())
                    })
                    .collect();

                let id = Self::cross_case_match_id(entity_type, "fuzzy", &MatchStrategy::Fuzzy);

                fuzzy_matches.push(CrossCaseMatch {
                    id,
                    entities,
                    entity_type: entity_type.clone(),
                    confidence: 0.50,
                    match_strategy: MatchStrategy::Fuzzy,
                });
            }
        }

        // ── Assemble final result ───────────────────────────────────
        let mut results: Vec<CrossCaseMatch> = Vec::new();
        results.extend(exact_matches);
        results.extend(normalized_matches);
        results.extend(fuzzy_matches);

        Ok(results)
    }

    // ── Helpers ────────────────────────────────────────────────────

    /// Secondary normalization applied on top of the canonical value for
    /// the Normalized match tier.
    ///
    /// - Email (`person` / `email` types): strip the domain (keep only
    ///   the local part before `@`).
    /// - Account: strip a leading `DOMAIN\` or `domain\` prefix.
    /// - All types: trim and lowercase.
    fn secondary_normalize(value: &str, entity_type: &str) -> String {
        let v = value.trim().to_lowercase();

        match entity_type {
            "person" | "email" => {
                // Keep only the local part of an email address
                if let Some(at_pos) = v.find('@') {
                    v[..at_pos].to_string()
                } else {
                    v
                }
            }
            "account" => {
                // Strip DOMAIN\ prefix
                if let Some(bs_pos) = v.find('\\') {
                    v[bs_pos + 1..].to_string()
                } else {
                    v
                }
            }
            _ => v,
        }
    }

    /// Build a deterministic ID for a cross-case match group.
    fn cross_case_match_id(entity_type: &str, seed: &str, strategy: &MatchStrategy) -> String {
        use std::hash::{Hash, Hasher};
        let strategy_tag = match strategy {
            MatchStrategy::Exact => "exact",
            MatchStrategy::Normalized => "norm",
            MatchStrategy::Fuzzy => "fuzzy",
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        entity_type.hash(&mut hasher);
        seed.hash(&mut hasher);
        strategy_tag.hash(&mut hasher);
        format!("xcase:{}:{:016x}", strategy_tag, hasher.finish())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use persistence_sqlite::connection::open_or_create;
    use persistence_sqlite::runner;
    use std::path::Path;

    /// Create a temporary database file with the full schema and a case
    /// record, then insert resolved entity rows. Returns the path to the
    /// database file.
    fn setup_case_db(
        dir: &Path,
        case_id: &str,
        case_name: &str,
        entities: &[(&str, &str, &str)], // (entity_id, entity_type, canonical_value)
    ) -> PathBuf {
        let db_path = dir.join(format!("{}.db", case_id));
        let conn = open_or_create(&db_path).unwrap();
        runner::run_all(&conn).unwrap();

        // Insert the case record (required by FK constraints)
        conn.execute(
            "INSERT OR REPLACE INTO cases (id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                case_id,
                case_name,
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ],
        )
        .unwrap();

        // Insert resolved entities
        for (entity_id, entity_type, canonical_value) in entities {
            conn.execute(
                "INSERT OR REPLACE INTO resolved_entities
                 (id, case_id, entity_type, canonical_value, source_count, confidence, attributes_json)
                 VALUES (?1, ?2, ?3, ?4, 1, 0.85, '[]')",
                rusqlite::params![entity_id, case_id, entity_type, canonical_value],
            )
            .unwrap();
        }

        drop(conn);
        db_path
    }

    // ── Match tests ─────────────────────────────────────────────────

    #[test]
    fn test_match_same_email_across_two_cases() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(
            dir,
            "case-1",
            "Case One",
            &[
                ("resolved-1", "person", "alice@example.com"),
                ("resolved-2", "person", "bob@test.org"),
            ],
        );
        let db2 = setup_case_db(
            dir,
            "case-2",
            "Case Two",
            &[
                ("resolved-3", "person", "alice@example.com"),
                ("resolved-4", "device", "DESKTOP-XYZ"),
            ],
        );

        let matches = CrossCaseEntityMatcher::match_entities_across_cases(&[db1, db2]).unwrap();

        // Should find an exact match for alice@example.com
        let exact: Vec<_> = matches
            .iter()
            .filter(|m| m.match_strategy == MatchStrategy::Exact)
            .collect();
        assert!(!exact.is_empty(), "expected at least one exact match");

        let alice_match = exact
            .iter()
            .find(|m| m.canonical_value_contains("alice@example.com"))
            .expect("must match on alice@example.com");

        assert_eq!(alice_match.entity_type, "person");
        assert!((alice_match.confidence - 0.95).abs() < f64::EPSILON);
        assert_eq!(alice_match.entities.len(), 2);
        assert!(alice_match.entities.iter().any(|e| e.0 == "case-1"));
        assert!(alice_match.entities.iter().any(|e| e.0 == "case-2"));
    }

    // Helper for the test above: check if any entity in the match has the
    // given canonical value.
    impl CrossCaseMatch {
        fn canonical_value_contains(&self, needle: &str) -> bool {
            self.entities.iter().any(|(_, _, val)| val == needle)
        }
    }

    #[test]
    fn test_no_match_different_entities() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(
            dir,
            "case-1",
            "Case One",
            &[("resolved-1", "person", "alice@example.com")],
        );
        let db2 = setup_case_db(
            dir,
            "case-2",
            "Case Two",
            &[("resolved-2", "person", "bob@test.org")],
        );

        let matches = CrossCaseEntityMatcher::match_entities_across_cases(&[db1, db2]).unwrap();

        // No exact match (different emails), no normalized match
        // (different local parts). Should only get a fuzzy match since
        // both are "person" type — but fuzzy matches are only for
        // entities that share the same type, which they do. However,
        // Alice and Bob should NOT be an exact match.
        let exact: Vec<_> = matches
            .iter()
            .filter(|m| m.match_strategy == MatchStrategy::Exact)
            .collect();
        assert!(
            exact.is_empty(),
            "no exact match expected for different emails"
        );
    }

    #[test]
    fn test_multiple_case_match() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(
            dir,
            "case-1",
            "Case One",
            &[
                ("resolved-1", "person", "charlie@example.com"),
                ("resolved-2", "account", "DOMAIN\\jdoe"),
            ],
        );
        let db2 = setup_case_db(
            dir,
            "case-2",
            "Case Two",
            &[
                ("resolved-3", "person", "charlie@example.com"),
                ("resolved-4", "account", "jdoe"), // same user, no domain prefix
            ],
        );
        let db3 = setup_case_db(
            dir,
            "case-3",
            "Case Three",
            &[("resolved-5", "person", "charlie@example.com")],
        );

        let matches =
            CrossCaseEntityMatcher::match_entities_across_cases(&[db1, db2, db3]).unwrap();

        // exact match on charlie@example.com across 3 cases
        let exact: Vec<_> = matches
            .iter()
            .filter(|m| m.match_strategy == MatchStrategy::Exact)
            .collect();
        assert!(!exact.is_empty());

        let charlie_match = exact
            .iter()
            .find(|m| m.canonical_value_contains("charlie@example.com"))
            .expect("must find charlie exact match");
        assert_eq!(charlie_match.entities.len(), 3);
        assert!((charlie_match.confidence - 0.95).abs() < f64::EPSILON);

        // normalized match on jdoe account (DOMAIN\jdoe vs jdoe)
        let norm: Vec<_> = matches
            .iter()
            .filter(|m| m.match_strategy == MatchStrategy::Normalized)
            .collect();
        assert!(
            !norm.is_empty(),
            "expected normalized match for jdoe account"
        );

        let jdoe_match = norm
            .iter()
            .find(|m| m.entity_type == "account")
            .expect("must find jdoe normalized match");
        assert_eq!(jdoe_match.entities.len(), 2);
        assert!((jdoe_match.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_single_db_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(
            dir,
            "case-1",
            "Case One",
            &[("resolved-1", "person", "alice@example.com")],
        );

        let result = CrossCaseEntityMatcher::match_entities_across_cases(&[db1]);
        assert!(result.is_err(), "single database should be rejected");
    }

    #[test]
    fn test_empty_cases() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(dir, "case-1", "Case One", &[]);
        let db2 = setup_case_db(dir, "case-2", "Case Two", &[]);

        let matches = CrossCaseEntityMatcher::match_entities_across_cases(&[db1, db2]).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_fuzzy_match_across_cases() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();

        let db1 = setup_case_db(
            dir,
            "case-1",
            "Case One",
            &[("resolved-1", "device", "DESKTOP-ABC")],
        );
        let db2 = setup_case_db(
            dir,
            "case-2",
            "Case Two",
            &[("resolved-2", "device", "LAPTOP-XYZ")],
        );

        let matches = CrossCaseEntityMatcher::match_entities_across_cases(&[db1, db2]).unwrap();

        // Same entity_type ("device"), different values: should be a fuzzy match
        let fuzzy: Vec<_> = matches
            .iter()
            .filter(|m| m.match_strategy == MatchStrategy::Fuzzy)
            .collect();
        assert!(!fuzzy.is_empty(), "expected fuzzy match on device type");
        assert_eq!(fuzzy[0].entity_type, "device");
        assert!((fuzzy[0].confidence - 0.50).abs() < f64::EPSILON);
        assert_eq!(fuzzy[0].entities.len(), 2);
    }
}
