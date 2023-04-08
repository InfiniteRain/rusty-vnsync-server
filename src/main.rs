use crate::vnsync_server::launch;

mod actors;
mod messages;
mod vnsync_server;

#[tokio::main]
async fn main() {
    launch(8080).await;
}
