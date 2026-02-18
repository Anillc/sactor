use sactor::sactor;

struct Hello {}

#[sactor(pub(crate))]
impl Hello {
    pub(crate) fn greet(&self) -> String {
        "Hello, Sactor!".to_string()
    }
}

#[tokio::main]
async fn main() {
    let (future, handle) = Hello::run(|_| Hello {});
    tokio::spawn(future);

    let handle = handle.clone();
    println!("Greeting: {:?}", handle.greet().await.unwrap());
}
