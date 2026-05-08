use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum FactProvenance {
    #[default]
    StructuralScanner,
    TreeSitter,
    Scip,
    Lsp,
    Compiler,
    Manual,
}

impl FactProvenance {
    pub fn rank(self) -> u8 {
        match self {
            Self::StructuralScanner => 1,
            Self::TreeSitter => 2,
            Self::Lsp => 3,
            Self::Scip => 4,
            Self::Compiler => 5,
            Self::Manual => 6,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Low,
    #[default]
    Medium,
    High,
    Exact,
}

pub fn prefer_fact(
    existing: (FactProvenance, Confidence),
    candidate: (FactProvenance, Confidence),
) -> bool {
    let existing_score = fact_score(existing.0, existing.1);
    let candidate_score = fact_score(candidate.0, candidate.1);
    candidate_score > existing_score
}

fn fact_score(provenance: FactProvenance, confidence: Confidence) -> u16 {
    let confidence_score = match confidence {
        Confidence::Low => 1,
        Confidence::Medium => 2,
        Confidence::High => 3,
        Confidence::Exact => 4,
    };
    u16::from(provenance.rank()) * 10 + confidence_score
}
