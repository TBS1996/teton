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
use common::AgentRequest;
use common::PatientStatus;
use common::ServerResponse;
use common::{AgentMessage, ServerMessage};
use common::{AgentResponse, ServerRequest};
use futures_util::SinkExt;
use futures_util::StreamExt;
use hyper::Server;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{debug, error, info, trace, warn};

type AgentID = String;

#[derive(Debug, Serialize, Deserialize)]
enum StateMessage {
    GetQty,
}

struct Inner {
    agents: HashMap<String, UnboundedSender<FooBar>>,
}

#[derive(Clone)]
struct State(Arc<Mutex<Inner>>);

impl State {
    fn new() -> Self {
        let inner = Inner {
            agents: HashMap::default(),
        };

        Self(Arc::new(Mutex::new(inner)))
    }

    async fn qty(&self) -> usize {
        self.0.lock().await.agents.len()
    }

    async fn get_status(&self, id: String) -> Option<PatientStatus> {
        let res = self.send_message(id, ServerRequest::GetStatus).await?;
        if let AgentResponse::Status(status) = res {
            return Some(status);
        }

        panic!();
    }

    async fn send_message(&self, id: String, content: ServerRequest) -> Option<AgentResponse> {
        let (os, msg) = FooBar::new(content);
        self.0.lock().await.agents.get(&id)?.send(msg).ok()?;
        os.await.ok()
    }

    async fn insert_agent(&self, id: AgentID, tx: UnboundedSender<FooBar>) {
        if let Some(prev) = self.0.lock().await.agents.insert(id, tx) {
            let (_, msg) = FooBar::new(ServerRequest::Close(
                "agent with same ID connected to server".to_string(),
            ));

            prev.send(msg).ok();
        }
    }
}

pub async fn run_server() {
    tracing_subscriber::fmt::init();
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

    let res = state.send_message(agent_id, content).await;

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

#[derive(Debug)]
struct FooBar {
    msg: ServerRequest,
    os: oneshot::Sender<AgentResponse>,
}

impl FooBar {
    fn new(msg: ServerRequest) -> (oneshot::Receiver<AgentResponse>, Self) {
        let (tx, rx) = oneshot::channel();
        (rx, Self { msg, os: tx })
    }
}

async fn handle_socket(socket: WebSocket, state: State, id: String) {
    info!("connecting agent: {}", &id);

    let (server_tx, mut server_rx) = {
        let (server_tx, server_rx) = mpsc::unbounded_channel::<FooBar>();
        (server_tx, UnboundedReceiverStream::new(server_rx))
    };

    state.insert_agent(id.clone(), server_tx).await;

    let (mut socket_tx, mut socket_rx) = socket.split();
    let mut oneshots: HashMap<String, oneshot::Sender<AgentResponse>> = Default::default();

    info!("starting select loop");
    loop {
        tokio::select! {
            // Messages received from the agent
            res = socket_rx.next() => {
                if let Some(Ok(msg)) = res {
                    match AgentMessage::from_message(msg) {
                        AgentMessage::Response{id, message} => {
                            let os = oneshots.remove(&id).unwrap();
                            os.send(message).unwrap();
                        },
                        AgentMessage::Request{id, message} => {
                            match message {
                                AgentRequest::AgentStatus(agent_id) => {
                                    let status = state.get_status(agent_id).await;
                                    let msg = ServerMessage::Response { id, message: ServerResponse::Status(status) };
                                    socket_tx.send(msg.into_message()).await.unwrap();

                                }
                                AgentRequest::GetQty => {
                                    let qty = state.qty().await;
                                    let msg = ServerMessage::Response { id, message: ServerResponse::Qty(qty) };
                                    socket_tx.send(msg.into_message()).await.unwrap();
                                },
                            }

                        },
                    };
                }
            },

            // Messages received from [`State`]
            res = server_rx.next() => {
                let Some(msg) = res else {
                    return;
                };

                let id = uuid::Uuid::new_v4().simple().to_string();
                oneshots.insert(id.clone(), msg.os);
                let res = ServerMessage::Request {id, message: msg.msg};
                socket_tx.send(res.into_message()).await.unwrap();
            },
        }
    }
}
