use alloy::transports::http::reqwest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("core error")]
    Core(#[from] contender_core::Error),

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("reqwest error")]
    Reqwest(#[from] reqwest::Error),

    #[error("toml deserialization error")]
    TomlDe(#[from] toml::de::Error),

    #[error("toml serialization error")]
    TomlSer(#[from] toml::ser::Error),

    #[error("campaign validation error: {0}")]
    Campaign(#[from] CampaignError),
}

#[derive(Debug, Error)]
pub enum CampaignError {
    #[error("campaign name must not be empty")]
    NameEmpty,

    #[error("'rate' must be specified for [spam] or [[spam.stage]]")]
    SpamRateMissing,

    #[error("stage {index} ({name}) missing duration and no default spam duration provided")]
    SpamDurationMissing { index: usize, name: String },

    #[error("stage {name} mix shares must sum to a positive number")]
    MixSharesSumInvalid { name: String },

    #[error("stage {index} ({name}) must include at least one mix entry")]
    StageMixEmpty { index: usize, name: String },

    #[error("campaign spam: spam.mix must include at least one entry")]
    SpamMixEmpty,

    #[error("campaign spam: must define either spam.stage or spam.mix + spam.duration")]
    SpamStageOrMixUndefined,

    #[error("campaign spam: shorthand requires spam.duration")]
    ShorthandRequiresSpamDuration,

    #[error("campaign spam: cannot define both spam.stage and spam.mix")]
    ConflictingMixAndStage,

    #[error(
        "stage {}: rate distribution error - assigned {} exceeds total rate {}",
        name,
        assigned_rate,
        total_rate
    )]
    RateDistributionExceedsLimit {
        name: String,
        assigned_rate: u64,
        total_rate: u64,
    },
}
