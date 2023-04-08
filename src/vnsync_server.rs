use crate::actors::server_actor::{ServerActor, ServerMessage};
use ractor::Actor;
use simple_websockets::Event;

pub async fn launch(port: u16) {
    let event_hub = simple_websockets::launch(port).expect("failed to launch on port 8080");
    let (actor, _) = Actor::spawn(None, ServerActor, ())
        .await
        .expect("failed to start server actor");

    loop {
        actor
            .send_message(match event_hub.poll_async().await {
                Event::Connect(client_id, responder) => {
                    ServerMessage::Connect(client_id, responder)
                }
                Event::Disconnect(client_id) => ServerMessage::Disconnect(client_id),
                Event::Message(client_id, message) => ServerMessage::Message(client_id, message),
            })
            .expect("failed to send a message to server actor");
    }
}
