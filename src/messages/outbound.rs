use crate::{ResponderDelegate, ResponderTrait};
use serde::Serialize;
use simple_websockets::Message as WebSocketMessage;

#[derive(Debug, Serialize)]
#[serde(tag = "init_type")]
pub enum InitType {
    #[serde(rename = "host")]
    Host { session_id: String, room_id: String },
    #[serde(rename = "client")]
    Client { session_id: String },
    #[serde(rename = "reconnect")]
    Reconnect,
}

#[derive(Debug, Serialize)]
#[serde(tag = "reply_to")]
pub enum ReplyData {
    #[serde(rename = "init")]
    Init(InitType),
    #[serde(rename = "get_state_string")]
    GetStateString { string: String },
    #[serde(rename = "set_state_string")]
    SetStateString,
}

#[derive(Debug, Serialize)]
#[serde(tag = "method")]
pub enum OutboundMessage {
    #[serde(rename = "close")]
    Close { reason: String },
    #[serde(rename = "reply")]
    Reply { id: String, data: ReplyData },
}

impl OutboundMessage {
    pub fn send(&self, responder: &Box<dyn ResponderTrait>) {
        let message_json = serde_json::to_string(self).expect("should serialize OutboundMessage");
        responder.send(WebSocketMessage::Text(message_json));
    }
}
