use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

pub const ADDR: &str = "0.0.0.0:3000";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PatientStatus {
    Sitting,
    Lying,
}

pub type ServerResponse = Vec<u8>;
pub type AgentResponse = Vec<u8>;
pub type AgentID = String;

pub struct BarFoo<T: Sized> {
    pub request: T,
    pub sender: oneshot::Sender<ServerResponse>,
}

impl<T> BarFoo<T> {
    pub fn new(request: T) -> (oneshot::Receiver<Vec<u8>>, Self) {
        let (sender, rx) = oneshot::channel();
        (rx, Self { request, sender })
    }
}

// A message from an agent
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentMessage {
    Request { id: String, message: AgentRequest },
    Response { id: String, data: Vec<u8> },
}

#[cfg(feature = "server")]
use axum::extract::ws::Message as AxumMessage;

#[cfg(not(feature = "server"))]
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;

#[cfg(feature = "server")]
impl AgentMessage {
    pub fn from_message(message: AxumMessage) -> Self {
        if let AxumMessage::Binary(b) = message {
            return serde_json::from_slice(&b).unwrap();
        };

        panic!();
    }
}

#[cfg(not(feature = "server"))]
impl AgentMessage {
    pub fn new_response<T: Serialize>(id: String, data: T) -> Self {
        let data = serde_json::to_vec(&data).unwrap();

        Self::Response { id, data }
    }

    pub fn into_message(self) -> TungsteniteMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        TungsteniteMessage::Binary(writer)
    }
}

// A message from the agent
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessage {
    Request { id: String, message: ServerRequest },
    Response { id: String, data: ServerResponse },
}

/// Message from the server to an agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerRequest {
    GetStatus,
    Close(String),
}

#[cfg(feature = "server")]
impl ServerMessage {
    pub fn new_response<T: Serialize>(id: String, data: T) -> Self {
        let data = serde_json::to_vec(&data).unwrap();

        Self::Response { id, data }
    }
    pub fn into_message(self) -> AxumMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        AxumMessage::Binary(writer)
    }
}

#[cfg(not(feature = "server"))]
impl ServerMessage {
    pub fn from_message(message: TungsteniteMessage) -> Self {
        if let TungsteniteMessage::Binary(b) = message {
            return serde_json::from_slice(&b).unwrap();
        };

        panic!();
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentRequest {
    GetQty,
    AgentStatus(String),
}
