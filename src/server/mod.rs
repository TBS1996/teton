use crate::common;
use crate::server::agent::Agent;
use agent::handle_socket;
use axum::{
    extract::ws::WebSocketUpgrade, extract::Extension, extract::Path, extract::Query,
    response::IntoResponse, routing::get, Router,
};
use common::AgentID;
use common::PatientStatus;
use futures_util::StreamExt;
use hyper::Server;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{self, info};

mod agent;

enum StateMessage {
    GetQty(oneshot::Sender<usize>),
    GetStatus((AgentID, oneshot::Sender<Option<PatientStatus>>)),
    Purge(AgentID),
}

struct Inner {
    agents: HashMap<AgentID, Agent>,
    tx: UnboundedSender<StateMessage>,
}

#[derive(Clone)]
struct State(Arc<Mutex<Inner>>);

impl State {
    fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = UnboundedReceiverStream::new(rx);

        let s = Self(Arc::new(Mutex::new(Inner {
            tx,
            agents: Default::default(),
        })));

        s.handle_statemessage(rx);

        s
    }

    fn handle_statemessage(&self, mut rx: UnboundedReceiverStream<StateMessage>) {
        let state = self.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.next().await {
                match msg {
                    StateMessage::Purge(id) => {
                        state.0.lock().await.agents.remove(&id);
                    }
                    StateMessage::GetQty(tx) => {
                        tx.send(state.qty().await).unwrap();
                    }
                    StateMessage::GetStatus((id, tx)) => {
                        let status = match state.get_agent(&id).await {
                            Some(agent) => agent.status().await,
                            None => None,
                        };

                        tx.send(status).unwrap();
                    }
                }
            }
        });
    }

    async fn state_tx(&self) -> UnboundedSender<StateMessage> {
        self.0.lock().await.tx.clone()
    }

    async fn insert_agent(&self, id: AgentID, agent: Agent) {
        if let Some(agent) = self.0.lock().await.agents.insert(id, agent) {
            agent
                .kill("agent with same ID connected to server".to_string())
                .await;
        }
    }

    async fn get_agent(&self, id: &AgentID) -> Option<Agent> {
        self.0.lock().await.agents.get(id).cloned()
    }

    async fn qty(&self) -> usize {
        self.0.lock().await.agents.len()
    }
}

pub async fn run_server() {
    info!("running server");

    let state = State::new();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/status/:agent_id", get(status_handler))
        .route("/kill/:agent_id", get(kill_handler))
        .layer(Extension(state));

    Server::bind(&common::ADDR.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn kill_handler(
    Path(agent_id): Path<String>,
    Extension(state): Extension<State>,
) -> impl IntoResponse {
    info!("{}: receive kill command for: ", &agent_id);

    if let Some(agent) = state.get_agent(&agent_id).await {
        agent.kill("killed through api command".to_string()).await;
        "agent killed"
    } else {
        "agent not found"
    }
}

async fn status_handler(
    Path(agent_id): Path<String>,
    Extension(state): Extension<State>,
) -> impl IntoResponse {
    info!("{}: receive get status request", &agent_id);
    if let Some(s) = state.get_agent(&agent_id).await {
        info!("found agent ");
        let status = &s.status().await;
        return serde_json::to_string(&status).unwrap();
    } else {
        info!("no agent ");
        return "agent not found".to_string();
    };
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<State>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let id = params.get("agent_id").unwrap().to_string();
    let tx = state.state_tx().await;
    let (rx, agent) = Agent::new();
    state.insert_agent(id.clone(), agent.clone()).await;
    ws.on_upgrade(|socket| handle_socket(socket, id, tx, rx, agent))
}
