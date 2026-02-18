use std::{
    error::Error,
    fmt::{Display, Formatter},
};

pub type SactorResult<T> = Result<T, SactorError>;

#[derive(Debug)]
pub enum SactorError {
    ActorStopped,
    Other(anyhow::Error),
}

impl Display for SactorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use SactorError::*;
        match self {
            ActorStopped => write!(f, "Actor has been stopped"),
            Other(err) => write!(f, "{}", err),
        }
    }
}

impl<E> From<E> for SactorError
where
    E: Error + Send + Sync + 'static,
{
    fn from(err: E) -> Self {
        SactorError::Other(err.into())
    }
}
