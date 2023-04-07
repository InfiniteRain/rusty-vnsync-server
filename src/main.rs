use crate::server_actor::ServerActor;
use crate::vnsync_server::launch;
use connection_handler::ConnectionFSM;
use ractor::Actor;

mod connection_actor;
mod connection_handler;
mod server_actor;
mod vnsync_server;

#[tokio::main]
async fn main() {
    launch(8080).await;
}
