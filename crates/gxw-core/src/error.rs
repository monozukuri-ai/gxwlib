use thiserror::Error;

#[derive(Debug, Error)]
pub enum GxwError {
    #[error("simulation {code} at instruction {instruction:?}: {message}")]
    Simulation {
        code: String,
        message: String,
        instruction: Option<usize>,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid format in {context}: {message}")]
    Format { context: String, message: String },
    #[error("unsupported format in {context}: {message}")]
    Unsupported { context: String, message: String },
    #[error("resource limit exceeded: {resource} ({actual} > {limit})")]
    ResourceLimit {
        resource: String,
        actual: u64,
        limit: u64,
    },
}

impl GxwError {
    pub(crate) fn format(context: impl Into<String>, error: impl std::fmt::Display) -> Self {
        Self::Format {
            context: context.into(),
            message: error.to_string(),
        }
    }
}
