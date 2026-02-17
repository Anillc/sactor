use thiserror::Error;

pub type SactorResult<T> = Result<T, SactorError>;

#[derive(Debug, Error)]
pub enum SactorError {
    #[error("Actor has stopped")]
    ActorStopped,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
