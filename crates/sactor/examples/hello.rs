use sactor::sactor;

struct Hello {}

#[sactor]
impl Hello {
    pub(crate) fn greet(&self) -> String {
        "Hello, Sactor!".to_string()
    }
}

#[tokio::main]
async fn main() {
    let hello = Hello {};
    let (future, handle) = hello.run();
    tokio::spawn(future);

    let handle = handle.clone();
    println!("Greeting: {:?}", handle.greet().await);
}
