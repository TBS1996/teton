use crate::common;
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    extract::Extension,
    extract::Path,
    extract::Query,
    response::IntoResponse,
    routing::get,
    Router,
};
use common::AgentID;
use common::AgentRequest;
use common::AgentResponse;
use common::PatientStatus;
use common::RequestID;
use common::ServerRequest;
use common::{AgentMessage, ServerMessage};
use futures_util::SinkExt;
use futures_util::StreamExt;
use hyper::Server;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{self, info};

#[derive(Default)]
struct Inner {
    agents: HashMap<AgentID, UnboundedSender<ServerMessage>>,
    oneshots: HashMap<RequestID, oneshot::Sender<AgentResponse>>,
}

#[derive(Clone, Default)]
struct State(Arc<Mutex<Inner>>);

impl State {
    fn new() -> Self {
        Self::default()
    }

    async fn qty(&self) -> usize {
        self.0.lock().await.agents.len()
    }

    async fn get_status(&self, id: AgentID) -> Option<PatientStatus> {
        self.send_message(id, ServerRequest::GetStatus).await
    }

    async fn handle_request(&self, id: RequestID, request: AgentRequest) -> ServerMessage {
        match request {
            AgentRequest::AgentStatus(agent_id) => {
                let status = self.get_status(agent_id).await;
                ServerMessage::new_response(id, status)
            }
            AgentRequest::GetQty => {
                let qty = self.qty().await;
                ServerMessage::new_response(id, qty)
            }
        }
    }

    async fn handle_response(&self, id: RequestID, response: AgentResponse) {
        match self.0.lock().await.oneshots.remove(&id) {
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

    /// Sends a request to the given agent and returns its response.
    async fn send_message<ResponseType: for<'de> Deserialize<'de>>(
        &self,
        id: AgentID,
        content: ServerRequest,
    ) -> Option<ResponseType> {
        let (sender, os) = oneshot::channel();

        let request_id = uuid::Uuid::new_v4().simple().to_string();

        self.0
            .lock()
            .await
            .oneshots
            .insert(request_id.clone(), sender);

        let req = ServerMessage::new_request(request_id, content);

        self.0.lock().await.agents.get(&id)?.send(req).ok()?;

        let res = os.await.ok()?;

        serde_json::from_slice(&res).ok()
    }

    async fn insert_agent(&self, id: AgentID, tx: UnboundedSender<ServerMessage>) {
        if let Some(prev) = self.0.lock().await.agents.insert(id, tx) {
            let msg =
                ServerMessage::Terminate("agent with same ID connected to server".to_string());
            prev.send(msg).ok();
        }
    }
}

pub async fn run_server() {
    info!("running server");

    let state = State::new();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/status/:agent_id", get(status_handler))
        .layer(Extension(state));

    Server::bind(&common::ADDR.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn status_handler(
    Path(agent_id): Path<String>,
    Extension(state): Extension<State>,
) -> impl IntoResponse {
    info!("{}: receive get status request", &agent_id);
    let content = ServerRequest::GetStatus;

    let res: Option<PatientStatus> = state.send_message(agent_id, content).await;

    serde_json::to_string(&res).unwrap()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<State>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let id = params.get("agent_id").unwrap().to_string();
    ws.on_upgrade(|socket| handle_socket(socket, state, id))
}

async fn handle_socket(socket: WebSocket, state: State, agent_id: AgentID) {
    info!("connecting agent: {}", &agent_id);

    let mut server_rx = {
        let (server_tx, server_rx) = mpsc::unbounded_channel::<ServerMessage>();
        state.insert_agent(agent_id.clone(), server_tx).await;
        UnboundedReceiverStream::new(server_rx)
    };

    let (mut socket_tx, mut socket_rx) = socket.split();

    info!("starting select loop");
    loop {
        tokio::select! {
            // Messages received from the agent
            Some(Ok(msg)) = socket_rx.next() => {
                match AgentMessage::from_message(msg) {
                    AgentMessage::Response{id, data} => state.handle_response(id, data).await,
                    AgentMessage::Request{id, message} => {
                        let msg = state.handle_request(id, message).await;
                        if let Err(e) = socket_tx.send(msg.into_message()).await {
                            tracing::error!("failed to send message to agent: {}. message: {:?}, err: {}", &agent_id, &msg, e);
                        }
                    },
                };
            },

            // Messages received from [`State`]
            Some(req) = server_rx.next() => {
                if let Err(e) = socket_tx.send(req.into_message()).await  {
                    tracing::error!("failed to send message to agent: {}. message: {:?}, err: {}", &agent_id, &req, e);
                }
            },
        }
    }
}
