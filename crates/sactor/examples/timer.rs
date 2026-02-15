use std::time::Duration;

use sactor::sactor;
use tokio::{signal::ctrl_c, time::interval};


struct App {
    handle: AppHandle,
    interval: Duration,
}

#[sactor]
impl App {
    fn new(handle: AppHandle) -> Self {
        Self {
            handle,
            interval: Duration::from_secs(1),
        }
    }

    fn init(&self) {
        let handle = self.handle.clone();
        let i = self.interval;
        tokio::spawn(async move {
            let mut timer = interval(i);
            loop {
                tokio::select! {
                    _ = handle.closed() => break,
                    _ = timer.tick() => {
                        let _ = handle.tick().await;
                    },
                }
            }
        });
    }

    async fn tick(&self) {
        println!("tick");
    }
}

#[tokio::main]
async fn main() {
    let (future, app) = App::run(|handle| App::new(handle));
    tokio::spawn(future);
    app.init().await.unwrap();

    ctrl_c().await.unwrap();
}
