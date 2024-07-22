use crate::common;
use crate::server::PatientStatus;
use crate::server::StateMessage;
use axum::extract::ws::WebSocket;
use common::AgentID;
use common::AgentRequest;
use common::AgentResponse;
use common::AlarmTriggerResult;
use common::RequestID;
use common::ServerRequest;
use common::{AgentMessage, ServerMessage};
use futures_util::SinkExt;
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{self, info};

type OneShots = HashMap<RequestID, oneshot::Sender<AgentResponse>>;

#[derive(Clone)]
pub struct Agent {
    oneshots: Arc<Mutex<OneShots>>,
    tx: UnboundedSender<ServerMessage>,
}

impl Agent {
    pub fn new() -> (UnboundedReceiverStream<ServerMessage>, Self) {
        let (tx, rx) = mpsc::unbounded_channel::<ServerMessage>();
        let rx = UnboundedReceiverStream::new(rx);
        let agent = Self {
            oneshots: Default::default(),
            tx,
        };

        (rx, agent)
    }

    pub fn kill(&self, reason: String) {
        self.tx.send(ServerMessage::Terminate(reason)).unwrap();
    }

    pub async fn trigger_alarm(&self) -> AlarmTriggerResult {
        self.send_message(ServerRequest::TriggerAlarm)
            .await
            .unwrap()
    }

    pub async fn status(&self) -> Option<PatientStatus> {
        self.send_message(ServerRequest::GetStatus).await
    }

    /// Sends a request to the given agent and returns its response.
    async fn send_message<ResponseType: for<'de> Deserialize<'de>>(
        &self,
        content: ServerRequest,
    ) -> Option<ResponseType> {
        let (sender, rx) = oneshot::channel();

        let request_id = uuid::Uuid::new_v4().simple().to_string();

        self.oneshots
            .lock()
            .await
            .insert(request_id.clone(), sender);

        let req = ServerMessage::new_request(request_id, content);
        self.tx.send(req).unwrap();

        let res = rx.await.ok()?;

        serde_json::from_slice(&res).ok()
    }

    async fn handle_response(&self, id: RequestID, response: AgentResponse) {
        match self.oneshots.lock().await.remove(&id) {
            Some(os) => {
                if os.send(response).is_err() {
                    tracing::error!("failed to send response to oneshot. id: {}", &id);
                }
            }
            None => {
                tracing::error!("id not found among oneshots: {}", &id);
            }
        };
    }

    async fn handle_request(
        &self,
        id: RequestID,
        request: AgentRequest,
        tx: UnboundedSender<StateMessage>,
    ) -> ServerMessage {
        match request {
            AgentRequest::AgentStatus(agent_id) => {
                let (txx, rx) = oneshot::channel();
                tx.send(StateMessage::GetStatus((agent_id, txx))).unwrap();
                let status = rx.await.unwrap();
                ServerMessage::new_response(id, status)
            }
            AgentRequest::GetQty => {
                let (txx, rx) = oneshot::channel();
                tx.send(StateMessage::GetQty(txx)).unwrap();
                let qty = rx.await.unwrap();
                ServerMessage::new_response(id, qty)
            }
        }
    }
}

pub async fn handle_socket(
    socket: WebSocket,
    agent_id: AgentID,
    state_tx: UnboundedSender<StateMessage>,
    mut server_rx: UnboundedReceiverStream<ServerMessage>,
    agent: Agent,
) {
    info!("connecting agent: {}", &agent_id);

    let (mut socket_tx, mut socket_rx) = socket.split();

    info!("starting select loop");
    loop {
        tokio::select! {
            // Messages received from the agent
            Some(Ok(msg)) = socket_rx.next() => {
                match AgentMessage::from_message(msg) {
                    AgentMessage::Closing(reason) => {
                        tracing::warn!("closing agent: {}. {}", &agent_id, reason);
                        state_tx.send(StateMessage::Purge(agent_id.clone())).unwrap();
                    },
                    AgentMessage::Response{id, data} => agent.handle_response(id, data).await,
                    AgentMessage::Request{id, message} => {
                        let msg = agent.handle_request(id, message, state_tx.clone()).await;
                        if let Err(e) = socket_tx.send(msg.into_message()).await {
                            tracing::error!("failed to send message to agent: {}. message: {:?}, err: {}", &agent_id, &msg, e);
                        }
                    },
                };
            },

            // Messages received from [`State`]
            Some(req) = server_rx.next() => {
                let should_shutdown = req.is_terminate();
                if let Err(e) = socket_tx.send(req.into_message()).await  {
                    tracing::error!("failed to send message to agent: {}. message: {:?}, err: {}", &agent_id, &req, e);
                }

                if should_shutdown {
                    state_tx.send(StateMessage::Purge(agent_id.clone())).unwrap();
                    info!("terminating agent: {}", &agent_id);
                    return;
                }
            },
        }
    }
}
