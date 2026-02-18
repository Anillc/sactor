use sactor::{
    error::{SactorError, SactorResult},
    sactor,
};
use thiserror::Error;

struct App {}

#[derive(Debug, Error)]
enum AppError {
    #[error("An example error")]
    ExampleError,
}

#[sactor]
impl App {
    fn test(&self) -> SactorResult<()> {
        Err(AppError::ExampleError)?;
        Ok(())
    }

    #[handle_error]
    fn handle_error(&self, error: &mut SactorError) {
        println!("Error: {:?}", error);
    }
}

#[tokio::main]
async fn main() {
    let (future, app) = App::run(|_| App {});
    tokio::spawn(future);

    app.test().await.unwrap().unwrap_err();
}
