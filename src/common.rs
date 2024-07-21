use serde::{Deserialize, Serialize};

pub const ADDR: &str = "0.0.0.0:3000";

impl ResponseType for AgentRequest {
    fn response_type(&self) -> &'static str {
        match self {
            Self::AgentStatus(_) => "PatientStatus",
            Self::GetQty => "usize",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum PatientStatus {
    Sitting,
    Lying,
}

// A message from the agent
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentMessage {
    Request { id: String, message: AgentRequest },
    Response { id: String, message: AgentResponse },
}

#[cfg(feature = "server")]
use axum::extract::ws::Message as AxumMessage;

#[cfg(not(feature = "server"))]
use tokio_tungstenite::tungstenite::protocol::Message as TungsteniteMessage;

/// Message from an agent to the server.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AgentResponse {
    Status(PatientStatus),
    AgentQty,
}

#[cfg(feature = "server")]
impl AgentMessage {
    pub fn into_message(self) -> AxumMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        AxumMessage::Binary(writer)
    }

    pub fn from_message(message: AxumMessage) -> Self {
        if let AxumMessage::Binary(b) = message {
            return serde_json::from_slice(&b).unwrap();
        };

        panic!();
    }
}

#[cfg(not(feature = "server"))]
impl AgentMessage {
    pub fn into_message(self) -> TungsteniteMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        TungsteniteMessage::Binary(writer)
    }

    pub fn from_message(message: TungsteniteMessage) -> Self {
        if let TungsteniteMessage::Binary(b) = message {
            return serde_json::from_slice(&b).unwrap();
        };

        panic!()
    }
}

// A message from the agent
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessage {
    Request { id: String, message: ServerRequest },
    Response { id: String, message: ServerResponse },
}

/// Message from the server to an agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerResponse {
    Qty(usize),
    Status(Option<PatientStatus>),
}

/// Message from the server to an agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ServerRequest {
    GetStatus,
    Close(String),
}

#[cfg(feature = "server")]
impl ServerMessage {
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
impl ServerMessage {
    pub fn into_message(self) -> TungsteniteMessage {
        let mut writer: Vec<u8> = vec![];
        serde_json::to_writer(&mut writer, &self).unwrap();
        TungsteniteMessage::Binary(writer)
    }

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
