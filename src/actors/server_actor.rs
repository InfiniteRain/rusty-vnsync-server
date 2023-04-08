use crate::{
    actors::connection_actor::{ConnectionActor, ConnectionMessage, ConnectionState},
    messages::inbound::InboundMessage,
    messages::outbound::OutboundMessage,
};
use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message, SupervisionEvent};
use simple_websockets::{Message as WebSocketMessage, Responder};
use std::collections::HashMap;

#[derive(Debug)]
pub struct Client {
    pub connection_actor: ActorRef<ConnectionActor>,
}

#[derive(Debug)]
pub struct ServerState {
    clients: HashMap<u64, Client>,
}

#[derive(Debug)]
pub enum ServerMessage {
    Connect(u64, Responder),
    Disconnect(u64),
    Message(u64, WebSocketMessage),
}

impl Message for ServerMessage {}

pub struct ServerActor;

#[async_trait]
impl Actor for ServerActor {
    type Msg = ServerMessage;
    type State = ServerState;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(ServerState {
            clients: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ServerMessage::Connect(client_id, responder) => {
                let (actor, _) =
                    Actor::spawn_linked(None, ConnectionActor, responder, myself.into())
                        .await
                        .expect("failed to start server actor");
                state.clients.insert(
                    client_id,
                    Client {
                        connection_actor: actor,
                    },
                );
            }
            ServerMessage::Disconnect(client_id) => {
                let client = state
                    .clients
                    .get(&client_id)
                    .expect(&format!("no client is associated with id {}", &client_id));

                client.connection_actor.stop(None);
                state.clients.remove(&client_id);
            }
            ServerMessage::Message(client_id, message) => {
                let client = state
                    .clients
                    .get(&client_id)
                    .expect(&format!("no client is associated with id {}", &client_id));

                let WebSocketMessage::Text(message_text) = &message else {
                    client.connection_actor
                        .send_message(ConnectionMessage::MalformedInboundMessageReceived)?;
                    return Ok(());
                };

                let deserialization_result = serde_json::from_str::<InboundMessage>(message_text);

                let Ok(inbound_message) = deserialization_result else {
                    client.connection_actor
                        .send_message(ConnectionMessage::MalformedInboundMessageReceived)?;
                    return Ok(());
                };

                client
                    .connection_actor
                    .send_message(ConnectionMessage::InboundMessageReceived(inbound_message))?;
            }
        }

        Ok(())
    }

    async fn handle_supervisor_evt(
        &self,
        _myself: ActorRef<Self>,
        message: SupervisionEvent,
        _state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        let SupervisionEvent::ActorTerminated(_, actor_state, reason) = message else {
            return Ok(());
        };

        let actor_state = actor_state
            .expect("state should be present")
            .take::<ConnectionState>()
            .expect("state should be ConnectionState");

        OutboundMessage::Close {
            reason: reason.unwrap_or("unknown".into()),
        }
        .send(&actor_state.responder);
        actor_state.responder.close();

        Ok(())
    }
}
