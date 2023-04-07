use statig::prelude::*;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub enum InitMessageType {
    Host,
    Client,
    Reconnect(String),
}

pub enum ConnectionEvent {
    WaitInit,
    InitReceived,
}

pub struct ConnectionFSM {
    pub on_timeout: fn(),
}

#[state_machine(initial = "State::initializing(None)")]
impl ConnectionFSM {
    #[state(entry_action = "enter_initializing")]
    async fn initializing(
        &mut self,
        timeout_token: &mut Option<CancellationToken>,
        event: &ConnectionEvent,
    ) -> Response<State> {
        let Some(token) = timeout_token else {
            panic!("timeout_token should be set on initializing");
        };

        match event {
            ConnectionEvent::WaitInit => {
                tokio::select! {
                    _ = token.cancelled() => Transition(State::initialized()),
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {
                        (self.on_timeout)();
                        Transition(State::terminal())
                    },
                }
            }
            ConnectionEvent::InitReceived => {
                token.cancel();
                println!("YEEEEEEEEEEEE");
                Super
            }
        }
    }

    #[action]
    fn enter_initializing(&mut self, timeout_token: &mut Option<CancellationToken>) {
        if let Some(_) = timeout_token {
            panic!("timeout_token should not be set on enter_initializing")
        }

        *timeout_token = Some(CancellationToken::new());
    }

    #[state]
    fn initialized(event: &ConnectionEvent) -> Response<State> {
        Super
    }

    #[state]
    fn terminal(event: &ConnectionEvent) -> Response<State> {
        Super
    }
}
