pub enum SpamDuration {
    Seconds(u64),
    Blocks(u64),
}

impl SpamDuration {
    pub fn value(&self) -> u64 {
        match self {
            SpamDuration::Seconds(v) => *v,
            SpamDuration::Blocks(v) => *v,
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            SpamDuration::Seconds(_) => "seconds",
            SpamDuration::Blocks(_) => "blocks",
        }
    }

    pub fn is_seconds(&self) -> bool {
        matches!(self, SpamDuration::Seconds(_))
    }

    pub fn is_blocks(&self) -> bool {
        matches!(self, SpamDuration::Blocks(_))
    }
}

impl std::fmt::Display for SpamDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpamDuration::Seconds(v) => write!(f, "{v} seconds"),
            SpamDuration::Blocks(v) => write!(f, "{v} blocks"),
        }
    }
}

impl From<String> for SpamDuration {
    fn from(value: String) -> Self {
        let value = value.trim();
        if let Some(stripped) = value.strip_suffix(" seconds") {
            if let Ok(seconds) = stripped.trim().parse::<u64>() {
                return SpamDuration::Seconds(seconds);
            }
        } else if let Some(stripped) = value.strip_suffix(" blocks") {
            if let Ok(blocks) = stripped.trim().parse::<u64>() {
                return SpamDuration::Blocks(blocks);
            }
        }
        panic!("Invalid format for SpamDuration: {value}");
    }
}
