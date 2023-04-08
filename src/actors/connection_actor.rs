use std::time::Duration;

use crate::messages::{
    inbound::{InboundMessage, MessageBody},
    outbound::{InitType, OutboundMessage, ReplyData},
};
use async_trait::async_trait;
use ractor::{concurrency::JoinHandle, Actor, ActorProcessingErr, ActorRef, Message, MessagingErr};
use simple_websockets::Responder;

#[derive(Debug)]
pub struct ConnectionState {
    pub fsm: FSM,
    pub responder: Responder,
}

#[derive(Debug)]
pub enum FSM {
    WaitingForInit(JoinHandle<Result<(), MessagingErr>>),
    Initialized,
}

#[derive(Debug)]
pub enum ConnectionMessage {
    InitTimeout,
    MalformedInboundMessageReceived,
    InboundMessageReceived(InboundMessage),
}

impl Message for ConnectionMessage {}

#[derive(Debug)]
pub struct ConnectionActor;

#[async_trait]
impl Actor for ConnectionActor {
    type Msg = ConnectionMessage;
    type State = ConnectionState;
    type Arguments = Responder;

    async fn pre_start(
        &self,
        myself: ActorRef<Self>,
        responder: Responder,
    ) -> Result<Self::State, ActorProcessingErr> {
        let handle = myself.send_after(Duration::from_millis(5000), || {
            ConnectionMessage::InitTimeout
        });

        Ok(ConnectionState {
            fsm: FSM::WaitingForInit(handle),
            responder,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match (&state.fsm, &message) {
            // FSM::WaitingForInit
            (
                FSM::WaitingForInit(handle),
                ConnectionMessage::InboundMessageReceived(InboundMessage {
                    id,
                    body: MessageBody::Init(init_message),
                }),
            ) => {
                match init_message {
                    crate::messages::inbound::InitMessage::Host => {
                        OutboundMessage::Reply {
                            id: id.into(),
                            data: ReplyData::Init(InitType::Host {
                                session_id: "session_id".into(),
                                room_id: "room_id".into(),
                            }),
                        }
                        .send(&state.responder);
                    }
                    crate::messages::inbound::InitMessage::Client { room_id: _ } => {
                        OutboundMessage::Reply {
                            id: id.into(),
                            data: ReplyData::Init(InitType::Client {
                                session_id: "session_id".into(),
                            }),
                        }
                        .send(&state.responder);
                    }
                    crate::messages::inbound::InitMessage::Reconnect { session_id: _ } => {
                        OutboundMessage::Reply {
                            id: id.into(),
                            data: ReplyData::Init(InitType::Reconnect),
                        }
                        .send(&state.responder);
                    }
                }

                handle.abort();
                state.fsm = FSM::Initialized;
            }
            (FSM::WaitingForInit(_), ConnectionMessage::InitTimeout) => {
                myself.stop(Some("init_timeout".into()));
            }

            // Any state; MalformedInboundMessageReceived
            (_, ConnectionMessage::MalformedInboundMessageReceived) => {
                myself.stop(Some("malformed_message".into()));
            }

            // Other combinations
            _ => {}
        };

        Ok(())
    }
}
