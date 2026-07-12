use super::{parse_transaction_log, RegistryTransaction};

pub(crate) fn parse_and_merge_txlogs(
    log1: Option<&[u8]>,
    log2: Option<&[u8]>,
) -> (Vec<RegistryTransaction>, Vec<String>) {
    let mut transactions = Vec::new();
    let mut warnings = Vec::new();
    append_log(log1, "LOG1", &mut transactions, &mut warnings);
    append_log(log2, "LOG2", &mut transactions, &mut warnings);
    transactions.sort_by_key(|transaction| transaction.sequence_number);
    (transactions, warnings)
}

fn append_log(
    data: Option<&[u8]>,
    label: &str,
    transactions: &mut Vec<RegistryTransaction>,
    warnings: &mut Vec<String>,
) {
    let Some(data) = data else {
        return;
    };
    match parse_transaction_log(data) {
        Ok(result) => {
            warnings.extend(result.warnings);
            transactions.extend(result.transactions);
        }
        Err(error) => warnings.push(format!("SAM/SECURITY {label} parse failed: {error}")),
    }
}
