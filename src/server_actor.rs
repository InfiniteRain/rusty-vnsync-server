use crate::connection_actor::{ConnectionActor, ConnectionMessage};
use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};
use simple_websockets::{Message as WebsocketMessage, Responder};
use std::{collections::HashMap, time::Duration};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct Client {
    pub responder: Responder,
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
    Message(u64, WebsocketMessage),
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
        _myself: ActorRef<Self>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ServerMessage::Connect(client_id, responder) => {
                let (actor, handle) = Actor::spawn(None, ConnectionActor, ())
                    .await
                    .expect("failed to start server actor");
                state.clients.insert(
                    client_id,
                    Client {
                        responder,
                        connection_actor: actor,
                    },
                );
            }
            ServerMessage::Disconnect(client_id) => {
                state.clients.remove(&client_id);
            }
            ServerMessage::Message(client_id, message) => {
                let client = state
                    .clients
                    .get(&client_id)
                    .expect(&format!("no client is associated with {}", &client_id));
                println!("AAAAAAAAAAAAA");
                let a = client
                    .connection_actor
                    .send_message(ConnectionMessage::Message(message));
                // let responder = &client.responder;
                // responder.send(message);
                // responder.close();
            }
        }

        Ok(())
    }
}
