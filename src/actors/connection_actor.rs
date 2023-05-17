use super::server_actor::{ConnectionStopReason, ServerActor, ServerMessage};
use crate::{
    messages::{
        inbound::{InboundMessage, InitMessage, MessageBody},
        outbound::{InitType, OutboundMessage, ReplyData},
    },
    ResponderDelegate, ResponderTrait,
};
use async_trait::async_trait;
use mockall::mock;
use nanoid::nanoid;
use ractor::{
    call, concurrency::JoinHandle, Actor, ActorProcessingErr, ActorRef, Message, MessagingErr,
};
use simple_websockets::Responder;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: String,
    pub some_random_text: String,
}

#[derive(Debug)]
pub struct ConnectionState {
    pub server_actor: ActorRef<ServerActor>,
    pub fsm: FSM,
    pub responder: Box<dyn ResponderTrait>,
    pub session_state: Option<SessionState>,
}

#[derive(Debug)]
pub enum FSM {
    WaitingForInitialization {
        timer_handle: JoinHandle<Result<(), MessagingErr>>,
    },
    Initialized,
}

#[derive(Debug)]
pub enum ConnectionMessage {
    Stop { reason: ConnectionStopReason },
    InitTimeout,
    MalformedInboundMessageReceived,
    InboundMessageReceived { message: InboundMessage },
}

impl Message for ConnectionMessage {}

#[derive(Debug)]
pub struct ConnectionActor;

#[async_trait]
impl Actor for ConnectionActor {
    type Msg = ConnectionMessage;
    type State = ConnectionState;
    type Arguments = (ActorRef<ServerActor>, Box<dyn ResponderTrait>);

    async fn pre_start(
        &self,
        myself: ActorRef<Self>,
        (server_actor, responder): (ActorRef<ServerActor>, Box<dyn ResponderTrait>),
    ) -> Result<Self::State, ActorProcessingErr> {
        let timer_handle = myself.send_after(Duration::from_millis(5000), || {
            ConnectionMessage::InitTimeout
        });

        Ok(ConnectionState {
            server_actor,
            fsm: FSM::WaitingForInitialization { timer_handle },
            responder,
            session_state: None,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match (&state.fsm, message) {
            // WaitingForInitialization; InboundMessageReceived (Init) (Host)
            (
                FSM::WaitingForInitialization { timer_handle },
                ConnectionMessage::InboundMessageReceived {
                    message:
                        InboundMessage {
                            id,
                            body: MessageBody::Init(InitMessage::Host),
                        },
                },
            ) => {
                timer_handle.abort();

                let session_id = nanoid!();
                let room_id = nanoid!();

                state.session_state = Some(SessionState {
                    session_id: session_id.clone(),
                    some_random_text: "None".into(),
                });

                state.fsm = FSM::Initialized;

                OutboundMessage::Reply {
                    id,
                    data: ReplyData::Init(InitType::Host {
                        session_id,
                        room_id,
                    }),
                }
                .send(&state.responder);
            }

            // WaitingForInitialization; InboundMessageReceived (Init) (Client)
            (
                FSM::WaitingForInitialization { timer_handle },
                ConnectionMessage::InboundMessageReceived {
                    message:
                        InboundMessage {
                            id,
                            body: MessageBody::Init(InitMessage::Client { room_id: _ }),
                        },
                },
            ) => {
                timer_handle.abort();

                let session_id = nanoid!();

                state.session_state = Some(SessionState {
                    session_id: session_id.clone(),
                    some_random_text: "None".into(),
                });

                state.fsm = FSM::Initialized;

                OutboundMessage::Reply {
                    id,
                    data: ReplyData::Init(InitType::Client { session_id }),
                }
                .send(&state.responder);
            }

            // WaitingForInitialization; InboundMessageReceived (Init) (Reconnect)
            (
                FSM::WaitingForInitialization { timer_handle },
                ConnectionMessage::InboundMessageReceived {
                    message:
                        InboundMessage {
                            id,
                            body: MessageBody::Init(InitMessage::Reconnect { session_id }),
                        },
                },
            ) => {
                timer_handle.abort();

                let dangling_session_option = call!(state.server_actor, move |reply_port| {
                    ServerMessage::GetDanglingSession {
                        session_id,
                        reply_port,
                    }
                })?;

                match dangling_session_option {
                    Some(dangling_session) => {
                        dangling_session.timer_handle.abort();
                        println!("stoped dangling session timer for");

                        state.session_state = Some(dangling_session.session_state);
                        state.fsm = FSM::Initialized;

                        OutboundMessage::Reply {
                            id,
                            data: ReplyData::Init(InitType::Reconnect),
                        }
                        .send(&state.responder);
                    }
                    None => {
                        myself.send_message(ConnectionMessage::Stop {
                            reason: ConnectionStopReason::BadSessionIdProvided,
                        })?;
                    }
                };
            }

            // WaitingForInitialization; InitTimeout
            (FSM::WaitingForInitialization { timer_handle: _ }, ConnectionMessage::InitTimeout) => {
                myself.send_message(ConnectionMessage::Stop {
                    reason: ConnectionStopReason::InitTimeout,
                })?;
            }

            // Initialized; InboundMessageReceived (GetStateString)
            (
                FSM::Initialized,
                ConnectionMessage::InboundMessageReceived {
                    message:
                        InboundMessage {
                            id,
                            body: MessageBody::GetStateString,
                        },
                },
            ) => {
                let session_state = state.session_state.as_ref().unwrap();

                OutboundMessage::Reply {
                    id,
                    data: ReplyData::GetStateString {
                        string: session_state.some_random_text.clone(),
                    },
                }
                .send(&state.responder);
            }

            // Initialized; InboundMessageReceived ()
            (
                FSM::Initialized,
                ConnectionMessage::InboundMessageReceived {
                    message:
                        InboundMessage {
                            id,
                            body: MessageBody::SetStateString { string },
                        },
                },
            ) => {
                let session_state = state.session_state.as_mut().unwrap();

                session_state.some_random_text = string;

                OutboundMessage::Reply {
                    id,
                    data: ReplyData::SetStateString,
                }
                .send(&state.responder);
            }

            // Any state; MalformedInboundMessageReceived
            (_, ConnectionMessage::MalformedInboundMessageReceived) => {
                myself.send_message(ConnectionMessage::Stop {
                    reason: ConnectionStopReason::MalformedMessage,
                })?;
            }

            // Any state; Stop
            (_, ConnectionMessage::Stop { reason }) => {
                state
                    .server_actor
                    .send_message(ServerMessage::StopConnection {
                        connection_actor: myself,
                        session_state: state.session_state.clone(),
                        responder: dyn_clone::clone_box(&*state.responder),
                        reason,
                    })?;
            }

            // Other combinations
            _ => {}
        };

        Ok(())
    }
}

#[cfg(test)]
mock! {
    #[derive(Debug)]
    pub ConnectionActor {}

    #[async_trait]
    impl Actor for ConnectionActor {
        type Msg = ConnectionMessage;
        type State = ConnectionState;
        type Arguments = (ActorRef<ServerActor>, Box<dyn ResponderTrait>);

        async fn pre_start(
            &self,
            myself: ActorRef<Self>,
            args: (ActorRef<ServerActor>, Box<dyn ResponderTrait>)
        ) -> Result<ConnectionState, ActorProcessingErr>;

        async fn handle(
            &self,
            myself: ActorRef<Self>,
            message: ConnectionMessage,
            state: &mut ConnectionState,
        ) -> Result<(), ActorProcessingErr>;
    }
}
