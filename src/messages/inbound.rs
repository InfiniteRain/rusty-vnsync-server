use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "init_type")]
pub enum InitMessage {
    #[serde(rename = "host")]
    Host,
    #[serde(rename = "client")]
    Client { room_id: String },
    #[serde(rename = "reconnect")]
    Reconnect { session_id: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "method")]
pub enum MessageBody {
    #[serde(rename = "init")]
    Init(InitMessage),
}

#[derive(Debug, Deserialize)]
pub struct InboundMessage {
    pub id: String,
    pub body: MessageBody,
}
