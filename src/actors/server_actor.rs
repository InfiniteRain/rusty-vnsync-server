use super::connection_actor::SessionState;
use crate::{
    actors::connection_actor::{ConnectionActor, ConnectionMessage},
    messages::inbound::InboundMessage,
    messages::outbound::OutboundMessage,
};
use async_trait::async_trait;
use ractor::{concurrency::JoinHandle, Actor, ActorProcessingErr, ActorRef, Message, MessagingErr};
use simple_websockets::{Message as WebSocketMessage, Responder};
use std::{collections::HashMap, time::Duration};

const DANGLING_SESSION_TIMEOUT_MS: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub struct Client {
    pub connection_actor: ActorRef<ConnectionActor>,
}

#[derive(Debug)]
pub struct DanglingSession {
    timer_handler: JoinHandle<Result<(), MessagingErr>>,
    session_state: SessionState,
}

#[derive(Debug)]
pub struct ServerState {
    clients: HashMap<u64, Client>,
    dangling_sessions: HashMap<String, DanglingSession>,
}

#[derive(Debug)]
pub enum ConnectionStopReason {
    InitTimeout,
    MalformedMessage,
    BadSessionIdProvided,
    ClientDisconnect,
}

#[derive(Debug)]
pub enum ServerMessage {
    Connect {
        client_id: u64,
        responder: Responder,
    },
    Disconnect {
        client_id: u64,
    },
    Message {
        client_id: u64,
        message: WebSocketMessage,
    },
    StopConnectionResult {
        connection_actor: ActorRef<ConnectionActor>,
        session_state: Option<SessionState>,
        responder: Responder,
        reason: ConnectionStopReason,
    },
    GetDanglingSession {
        connection_actor: ActorRef<ConnectionActor>,
        session_id: String,
        message_id: String,
    },
    RemoveDanglingSession {
        session_id: String,
    },
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
            dangling_sessions: HashMap::new(),
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        println!("message received, {:?}", message);
        match message {
            ServerMessage::Connect {
                client_id,
                responder,
            } => {
                let (actor, _) = Actor::spawn(None, ConnectionActor, (myself.clone(), responder))
                    .await
                    .expect("failed to start server actor");

                state.clients.insert(
                    client_id,
                    Client {
                        connection_actor: actor,
                    },
                );
            }
            ServerMessage::Disconnect { client_id } => {
                if let Some(client) = state.clients.get(&client_id) {
                    client
                        .connection_actor
                        .send_message(ConnectionMessage::StopConnection {
                            reason: ConnectionStopReason::ClientDisconnect,
                        })?;
                    state.clients.remove(&client_id);
                };
            }
            ServerMessage::Message { client_id, message } => {
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

                let Ok(parsed_message) = deserialization_result else {
                    client.connection_actor
                        .send_message(ConnectionMessage::MalformedInboundMessageReceived)?;
                    return Ok(());
                };

                client.connection_actor.send_message(
                    ConnectionMessage::InboundMessageReceived {
                        message: parsed_message,
                    },
                )?;
            }
            ServerMessage::StopConnectionResult {
                connection_actor,
                session_state,
                responder,
                reason,
            } => {
                match reason {
                    ConnectionStopReason::InitTimeout
                    | ConnectionStopReason::MalformedMessage
                    | ConnectionStopReason::BadSessionIdProvided => {
                        OutboundMessage::Close {
                            reason: match reason {
                                ConnectionStopReason::InitTimeout => "init_timeout",
                                ConnectionStopReason::MalformedMessage => "malformed_message",
                                _ => "bad_session_id_provided",
                            }
                            .into(),
                        }
                        .send(&responder);

                        // removing the client so that the ServerMessage::StopConnection
                        // doesn't get re-emitted (closing the websocket connection from
                        // our side will emit the WebSocketMessage::Disconnect event)
                        state.clients.remove(&responder.client_id());

                        responder.close();
                    }
                    ConnectionStopReason::ClientDisconnect => {
                        let Some(session_state) = session_state else {
                            return Ok(());
                        };

                        let session_id = session_state.session_id.clone();
                        let timer_handler =
                            myself.send_after(DANGLING_SESSION_TIMEOUT_MS, move || {
                                ServerMessage::RemoveDanglingSession {
                                    session_id: session_id.clone(),
                                }
                            });

                        let session_id = session_state.session_id.clone();
                        state.dangling_sessions.insert(
                            session_id.clone(),
                            DanglingSession {
                                timer_handler,
                                session_state,
                            },
                        );

                        println!("started dangling session timer for {}", &session_id);
                    }
                };

                connection_actor.stop(None);
                println!("stopped connection actor");
            }
            ServerMessage::GetDanglingSession {
                connection_actor,
                session_id,
                message_id,
            } => {
                let session_state_option =
                    state
                        .dangling_sessions
                        .remove(&session_id)
                        .map(|dangling_session| {
                            dangling_session.timer_handler.abort();
                            dangling_session.session_state
                        });
                connection_actor.send_message(ConnectionMessage::GetDanglingSessionResult {
                    session_state_option,
                    message_id,
                })?;
            }
            ServerMessage::RemoveDanglingSession { session_id } => {
                state.dangling_sessions.remove(&session_id);
                println!("Session {} removed", &session_id);
            }
        }

        Ok(())
    }
}
