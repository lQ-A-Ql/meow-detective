use super::*;

#[test]
fn tier_advances_catalog_to_extract() {
    let mut sm = TierStateMachine::new();
    assert_eq!(sm.current_tier, None);

    let tier = advance_tier(&mut sm);
    assert_eq!(tier, Some(Tier::Catalog));
    assert_eq!(sm.current_tier, Some(Tier::Catalog));
    // Catalog is not yet completed — it's the active tier.
    assert!(!sm
        .tier_results
        .get(&Tier::Catalog)
        .is_some_and(|r| r.completed));

    // After MFT enumeration, advance again.
    let tier = advance_tier(&mut sm);
    assert_eq!(tier, Some(Tier::ExtractArtifacts));
    assert_eq!(sm.current_tier, Some(Tier::ExtractArtifacts));
    // Catalog should now be marked completed.
    assert!(sm
        .tier_results
        .get(&Tier::Catalog)
        .is_some_and(|r| r.completed));
}

#[test]
fn partial_results_accessible_at_catalog_tier() {
    let mut sm = TierStateMachine::new();
    // Start Catalog.
    advance_tier(&mut sm);
    // Add some partial stats before the tier completes.
    sm.tier_results.entry(Tier::Catalog).or_default().stats = "files_queued=42".to_string();

    // Complete Catalog, move to Extract.
    advance_tier(&mut sm);

    let catalog_result = sm.tier_results.get(&Tier::Catalog).unwrap();
    assert!(catalog_result.completed);
    assert_eq!(catalog_result.stats, "files_queued=42");
    // Extract tier is not yet complete.
    assert!(!sm
        .tier_results
        .get(&Tier::ExtractArtifacts)
        .is_some_and(|r| r.completed));
}

#[test]
fn all_tiers_complete() {
    let mut sm = TierStateMachine::new();

    // Advance through all three tiers.
    assert_eq!(advance_tier(&mut sm), Some(Tier::Catalog));
    assert_eq!(advance_tier(&mut sm), Some(Tier::ExtractArtifacts));
    assert_eq!(advance_tier(&mut sm), Some(Tier::CorrelateAndIndex));

    // All tiers should be active / completed.
    assert!(sm
        .tier_results
        .get(&Tier::Catalog)
        .is_some_and(|r| r.completed));
    assert!(sm
        .tier_results
        .get(&Tier::ExtractArtifacts)
        .is_some_and(|r| r.completed));
    assert_eq!(sm.current_tier, Some(Tier::CorrelateAndIndex));

    // Final advance — all done.
    assert_eq!(advance_tier(&mut sm), None);
    assert!(sm
        .tier_results
        .get(&Tier::CorrelateAndIndex)
        .is_some_and(|r| r.completed));
    assert_eq!(sm.current_tier, None);

    // Another call stays done.
    assert_eq!(advance_tier(&mut sm), None);
}
