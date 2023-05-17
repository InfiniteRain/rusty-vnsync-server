use super::connection_actor::SessionState;
use crate::{
    actors::connection_actor::{ConnectionActor, ConnectionMessage},
    messages::inbound::InboundMessage,
    messages::outbound::OutboundMessage,
    ResponderDelegate, ResponderTrait,
};
use async_trait::async_trait;
use mockall::mock;
use ractor::{
    concurrency::JoinHandle, Actor, ActorProcessingErr, ActorRef, Message, MessagingErr,
    RpcReplyPort,
};
use simple_websockets::Message as WebSocketMessage;
use std::{collections::HashMap, time::Duration};

const DANGLING_SESSION_TIMEOUT_MS: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct Client {
    pub connection_actor: ActorRef<ConnectionActor>,
}

#[derive(Debug)]
pub struct DanglingSession {
    pub timer_handle: JoinHandle<Result<(), MessagingErr>>,
    pub session_state: SessionState,
}

#[derive(Debug)]
pub struct ServerState {
    pub clients: HashMap<u64, Client>,
    pub dangling_sessions: HashMap<String, DanglingSession>,
}

#[derive(Debug)]
#[cfg(test)]
pub struct ServerStateSnapshot {
    pub clients: HashMap<u64, Client>,
    pub dangling_sessions: HashMap<String, SessionState>,
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
        responder: Box<dyn ResponderTrait>,
    },
    Disconnect {
        client_id: u64,
    },
    Message {
        client_id: u64,
        message: WebSocketMessage,
    },
    StopConnection {
        connection_actor: ActorRef<ConnectionActor>,
        session_state: Option<SessionState>,
        responder: Box<dyn ResponderTrait>,
        reason: ConnectionStopReason,
    },
    GetDanglingSession {
        session_id: String,
        reply_port: RpcReplyPort<Option<DanglingSession>>,
    },
    RemoveDanglingSession {
        session_id: String,
    },
    #[cfg(test)]
    GetStateSnapshot {
        reply_port: RpcReplyPort<ServerStateSnapshot>,
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
                        .send_message(ConnectionMessage::Stop {
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
            ServerMessage::StopConnection {
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
                        let timer_handle =
                            myself.send_after(DANGLING_SESSION_TIMEOUT_MS, move || {
                                ServerMessage::RemoveDanglingSession {
                                    session_id: session_id.clone(),
                                }
                            });

                        let session_id = session_state.session_id.clone();
                        state.dangling_sessions.insert(
                            session_id.clone(),
                            DanglingSession {
                                timer_handle,
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
                session_id,
                reply_port,
            } => {
                let dangling_session_option = state.dangling_sessions.remove(&session_id);
                reply_port.send(dangling_session_option)?;
            }
            ServerMessage::RemoveDanglingSession { session_id } => {
                state.dangling_sessions.remove(&session_id);
                println!("Session {} removed", &session_id);
            }
            #[cfg(test)]
            ServerMessage::GetStateSnapshot { reply_port } => {
                let result = reply_port.send(ServerStateSnapshot {
                    clients: state.clients.clone(),
                    dangling_sessions: state
                        .dangling_sessions
                        .iter()
                        .map(|(key, value)| (key.clone(), value.session_state.clone()))
                        .collect(),
                });
                println!("result: {:?}", result);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use mockall_double::double;

    #[double]
    use crate::ResponderDelegate;

    use super::*;
    use ractor::call;

    #[tokio::test]
    async fn connect_should_add_client_to_hashmap() {
        let (mock_responder, actor) = start_actor().await;
        let state = actor.get_state_snapshot().await;

        assert_eq!(state.clients.len(), 0);

        actor
            .send_message(ServerMessage::Connect {
                client_id: 0,
                responder: Box::new(mock_responder),
            })
            .unwrap();
        let state = actor.get_state_snapshot().await;

        assert_eq!(state.clients.len(), 1);
    }

    #[tokio::test]
    async fn disconnect_should_remove_client_from_hashmap() {
        let (mock_responder, actor) = start_actor().await;

        actor
            .send_message(ServerMessage::Connect {
                client_id: 0,
                responder: Box::new(mock_responder),
            })
            .unwrap();
        let state = actor.get_state_snapshot().await;

        assert_eq!(state.clients.len(), 1);

        actor
            .send_message(ServerMessage::Disconnect { client_id: 0 })
            .unwrap();
        let state = actor.get_state_snapshot().await;

        assert_eq!(state.clients.len(), 0);
    }

    // #[tokio::test]
    // async fn binary_message_should

    async fn start_actor() -> (ResponderDelegate, ActorRef<ServerActor>) {
        let mock_responder = ResponderDelegate::new();
        let (actor, _) = Actor::spawn(None, ServerActor, ())
            .await
            .expect("failed to start server actor");

        (mock_responder, actor)
    }

    #[async_trait]
    trait Snapshottable {
        async fn get_state_snapshot(&self) -> ServerStateSnapshot;
    }

    #[async_trait]
    impl Snapshottable for ActorRef<ServerActor> {
        async fn get_state_snapshot(&self) -> ServerStateSnapshot {
            call!(self, |reply_port| ServerMessage::GetStateSnapshot {
                reply_port
            })
            .unwrap()
        }
    }
}
