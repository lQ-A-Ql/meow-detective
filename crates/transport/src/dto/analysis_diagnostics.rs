use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisDiagnosticCountsDto {
    pub capability_limits: u64,
    pub corrupt_inputs: u64,
    pub read_failures: u64,
    pub recovered_inputs: u64,
    pub empty_inputs: u64,
}

impl AnalysisDiagnosticCountsDto {
    pub fn merge(&mut self, other: Self) {
        self.capability_limits += other.capability_limits;
        self.corrupt_inputs += other.corrupt_inputs;
        self.read_failures += other.read_failures;
        self.recovered_inputs += other.recovered_inputs;
        self.empty_inputs += other.empty_inputs;
    }

    pub fn into_checkpoint(self) -> [u64; 5] {
        [
            self.capability_limits,
            self.corrupt_inputs,
            self.read_failures,
            self.recovered_inputs,
            self.empty_inputs,
        ]
    }
}

impl From<[u64; 5]> for AnalysisDiagnosticCountsDto {
    fn from(value: [u64; 5]) -> Self {
        Self {
            capability_limits: value[0],
            corrupt_inputs: value[1],
            read_failures: value[2],
            recovered_inputs: value[3],
            empty_inputs: value[4],
        }
    }
}
