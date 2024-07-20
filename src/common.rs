use serde::{Deserialize, Serialize};

pub const ADDR: &str = "0.0.0.0:3000";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PatientStatus {
    Sitting,
    Lying,
}

#[cfg(feature = "server")]
use axum::extract::ws::Message as AxumMessage;

#[cfg(not(feature = "server"))]
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Response {
    pub message: AgentContent,
    pub id: String,
}

/// Message from an agent to the server.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentContent {
    Status(PatientStatus),
    AgentQty,
}

#[cfg(feature = "server")]
impl Response {
    pub fn into_message(self) -> AxumMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        AxumMessage::Binary(writer)
    }

    pub fn from_message(message: AxumMessage) -> Option<Self> {
        let AxumMessage::Binary(b) = message else {
            return None;
        };
        serde_json::from_slice(&b).ok()
    }
}

#[cfg(not(feature = "server"))]
impl Response {
    pub fn into_message(self) -> TungsteniteMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        TungsteniteMessage::Binary(writer)
    }

    pub fn from_message(message: TungsteniteMessage) -> Option<Self> {
        if let TungsteniteMessage::Binary(b) = message {
            serde_json::from_slice(&b).ok()
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub message: ServerContent,
    pub id: String,
}

/// Message from the server to an agent.
#[derive(Debug, Serialize, Deserialize)]
pub enum ServerContent {
    GetStatus,
    Close(String),
}

#[cfg(feature = "server")]
impl Request {
    pub fn into_message(self) -> AxumMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        AxumMessage::Binary(writer)
    }

    pub fn from_message(message: AxumMessage) -> Option<Self> {
        let AxumMessage::Binary(b) = message else {
            return None;
        };
        serde_json::from_slice(&b).ok()
    }
}

#[cfg(not(feature = "server"))]
impl Request {
    pub fn into_message(self) -> TungsteniteMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        TungsteniteMessage::Binary(writer)
    }

    pub fn from_message(message: TungsteniteMessage) -> Option<Self> {
        if let TungsteniteMessage::Binary(b) = message {
            serde_json::from_slice(&b).ok()
        } else {
            None
        }
    }
}
