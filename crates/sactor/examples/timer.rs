use std::time::Duration;

use sactor::sactor;
use tokio::{
    signal::ctrl_c,
    time::{Interval, interval},
};

struct App {
    ticker: Interval,
}

#[sactor]
impl App {
    fn new() -> Self {
        Self {
            ticker: interval(Duration::from_secs(1)),
        }
    }

    #[select]
    fn select(&mut self) -> Vec<Selection<'_>> {
        vec![selection!(self.ticker.tick().await, tick)]
    }

    fn tick(&self) {
        println!("tick");
    }
}

#[tokio::main]
async fn main() {
    let (future, _app) = App::run(|_| App::new());
    tokio::spawn(future);

    ctrl_c().await.unwrap();
}
