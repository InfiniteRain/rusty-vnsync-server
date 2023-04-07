use crate::connection_handler::{ConnectionEvent, ConnectionFSM};
use async_trait::async_trait;
use ractor::{Actor, ActorProcessingErr, ActorRef, Message};
use simple_websockets::{Message as WebsocketMessage, Responder};
use statig::awaitable::StateMachine;
use statig::prelude::*;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum ConnectionMessage {
    Message(WebsocketMessage),
}

impl Message for ConnectionMessage {}

#[derive(Debug)]
pub struct ConnectionActor;

#[async_trait]
impl Actor for ConnectionActor {
    type Msg = ConnectionMessage;
    type State = Arc<Mutex<StateMachine<ConnectionFSM>>>;
    type Arguments = ();

    async fn pre_start(
        &self,
        _myself: ActorRef<Self>,
        _: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        let fsm_mutex = Arc::new(Mutex::new(
            ConnectionFSM {
                on_timeout: || println!("TIMEOUT"),
            }
            .state_machine(),
        ));
        let fsm_mutex_clone = fsm_mutex.clone();

        tokio::spawn(async move {
            let mut fsm = fsm_mutex_clone.lock().await;
            fsm.handle(&ConnectionEvent::WaitInit).await;
        });

        Ok(fsm_mutex)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self>,
        message: Self::Msg,
        fsm_mutex: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        println!("BBB");
        let mut fsm = fsm_mutex.lock().await;
        println!("CCC");

        fsm.handle(&ConnectionEvent::InitReceived).await;

        Ok(())
    }
}
