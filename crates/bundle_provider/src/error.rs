use std::fmt::Display;

#[derive(Debug)]
pub enum BundleProviderError {
    InvalidUrl,
    SendBundleError(Box<dyn std::error::Error>),
}

impl Display for BundleProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            BundleProviderError::InvalidUrl => write!(f, "Invalid builder URL"),
            BundleProviderError::SendBundleError(e) => write!(f, "Failed to send bundle: {e:?}"),
        }
    }
}

impl std::error::Error for BundleProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BundleProviderError::InvalidUrl => None,
            BundleProviderError::SendBundleError(e) => Some(e.as_ref()),
        }
    }
}
