use thiserror::Error;

#[derive(Debug, Error)]
pub enum SactorError {
    #[error("Actor has been stopped")]
    ActorStopped,
}
