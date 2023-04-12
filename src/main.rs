use vnsync_server::launch;

#[tokio::main]
async fn main() {
    launch(8080).await;
}
