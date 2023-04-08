use serde::Serialize;
use simple_websockets::{Message as WebSocketMessage, Responder};

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
    pub fn send(&self, responder: &Responder) {
        let message_json = serde_json::to_string(self).expect("should serialize OutboundMessage");
        responder.send(WebSocketMessage::Text(message_json));
    }
}
