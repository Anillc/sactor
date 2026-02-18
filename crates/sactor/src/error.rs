use std::error::Error;

pub type SactorResult<T> = Result<T, SactorError>;

#[derive(Debug)]
pub enum SactorError {
    ActorStopped,
    Other(anyhow::Error),
}

impl<E> From<E> for SactorError
where
    E: Error + Send + Sync + 'static,
{
    fn from(err: E) -> Self {
        SactorError::Other(err.into())
    }
}
