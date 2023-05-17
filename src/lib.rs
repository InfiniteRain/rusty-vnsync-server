use crate::actors::server_actor::{ServerActor, ServerMessage};
use dyn_clone::DynClone;
use ractor::Actor;
use simple_websockets::{Event, Message as WebSocketMessage, Responder};
use std::fmt::Debug;

#[cfg(test)]
use mockall::mock;

mod actors;
mod messages;

pub trait ResponderTrait: Send + Debug + DynClone {
    fn send(&self, message: WebSocketMessage) -> bool;
    fn close(&self);
    fn client_id(&self) -> u64;
}

#[derive(Debug, Clone)]
pub struct ResponderDelegate {
    responder: Responder,
}

impl ResponderTrait for ResponderDelegate {
    fn send(&self, message: WebSocketMessage) -> bool {
        self.responder.send(message)
    }

    fn close(&self) {
        self.responder.close()
    }

    fn client_id(&self) -> u64 {
        self.responder.client_id()
    }
}

#[cfg(test)]
mock! {
    #[derive(Debug)]
    pub ResponderDelegate {}
    impl Clone for ResponderDelegate {
        fn clone(&self) -> Self;
    }
    impl ResponderTrait for ResponderDelegate {
        fn send(&self, message: WebSocketMessage) -> bool;
        fn close(&self);
        fn client_id(&self) -> u64;
    }
}

pub async fn launch(port: u16) {
    let event_hub =
        simple_websockets::launch(port).expect(&format!("failed to launch on port {}", port));
    let (actor, _) = Actor::spawn(None, ServerActor, ())
        .await
        .expect("failed to start server actor");

    loop {
        actor
            .send_message(match event_hub.poll_async().await {
                Event::Connect(client_id, responder) => ServerMessage::Connect {
                    client_id,
                    responder: Box::new(ResponderDelegate { responder }),
                },
                Event::Disconnect(client_id) => ServerMessage::Disconnect { client_id },
                Event::Message(client_id, message) => ServerMessage::Message { client_id, message },
            })
            .expect("failed to send a message to server actor");
    }
}
