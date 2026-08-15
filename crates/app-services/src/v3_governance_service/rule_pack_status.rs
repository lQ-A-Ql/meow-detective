use transport::dto::{RulePackInfoDto, RulePackStatusDto};

pub(crate) fn build_rule_pack_status() -> RulePackStatusDto {
    use crate::rule_pack::parser;

    build_rule_pack_status_from(parser::V2_STANDARD_TOML)
}

pub(crate) fn build_rule_pack_status_from(source: &str) -> RulePackStatusDto {
    use crate::rule_pack::parser;

    let mut loaded_packs = Vec::new();
    let mut load_status = "unavailable";

    match parser::parse_rule_pack(source) {
        Ok(pack) => {
            load_status = "loaded";
            let rule_count = pack.rules.len() as u32;
            loaded_packs.push(RulePackInfoDto {
                name: pack.manifest.name.clone(),
                version: pack.manifest.version.clone(),
                author: pack.manifest.author.clone(),
                rule_count,
                scope: pack.manifest.scope.clone(),
            });
        }
        Err(error) => {
            tracing::error!(?error, "built-in rule-pack definition could not be loaded");
        }
    }

    let total_rule_count = loaded_packs.iter().map(|p| p.rule_count).sum::<u32>();

    // The built-in pack is currently a definition-only capability. No persisted
    // per-case rule-pack run record exists, so never present a loaded definition
    // as if it had executed for the active case.
    let execution_status = if total_rule_count > 0 {
        "not_executed"
    } else {
        "unavailable"
    };

    RulePackStatusDto {
        loaded_packs,
        total_rule_count,
        load_status: load_status.to_string(),
        execution_status: execution_status.to_string(),
    }
}
