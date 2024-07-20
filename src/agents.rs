use crate::common;
use common::AgentMessage;
use common::ServerMessage;
use common::ServerResponse;
use common::{AgentResponse, PatientStatus, ServerRequest};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::sync::oneshot;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::connect_async;

use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;

fn handle_server_content(content: ServerRequest) -> AgentResponse {
    match content {
        ServerRequest::Close(msg) => {
            eprintln!("connection closed: {}", msg);
            std::process::exit(0);
        }

        ServerRequest::GetStatus => {
            let status = get_status();
            AgentResponse::Status(status)
        }
    }
}

use common::AgentRequest;

async fn sleep(secs: u64) {
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
}

struct FooBar {
    request: AgentRequest,
    sender: oneshot::Sender<ServerResponse>,
}

impl FooBar {
    fn new(request: AgentRequest, sender: oneshot::Sender<ServerResponse>) -> Self {
        Self { request, sender }
    }
}

// PoC to show that agents can make requests to server.
fn agent_counter(tx: UnboundedSender<FooBar>) {
    dbg!("starting agent counter thing");
    tokio::spawn(async move {
        loop {
            sleep(5).await;
            let (txx, rxx) = oneshot::channel();
            let inside = FooBar::new(AgentRequest::GetQty, txx);
            eprintln!("sending qty request");
            tx.send(inside).unwrap();
            let wth = rxx.await.unwrap();
            eprintln!("total agents: {:?}", wth);
        }
    });
}

fn agent_getstatus(tx: UnboundedSender<FooBar>, id: String) {
    eprintln!("getting status of: {}", &id);

    tokio::spawn(async move {
        loop {
            sleep(5).await;
            let (txx, rxx) = oneshot::channel();
            let inside = FooBar::new(AgentRequest::AgentStatus(id.clone()), txx);
            tx.send(inside).unwrap();
            if let ServerResponse::Status(status) = rxx.await.unwrap() {
                match status {
                    Some(status) => {
                        println!("{}-status: {:?}", &id, &status);
                    }
                    None => {
                        println!("no agent connected with following id: {}", &id);
                    }
                }
            }
        }
    });
}

pub async fn run(id: String, observe: Vec<String>) {
    dbg!("running agent", &id);

    let url = format!("ws://{}/ws?agent_id={}", common::ADDR, id);
    let (ws, _) = connect_async(url).await.expect("Failed to connect");
    let (mut socket_tx, mut socket_rx) = ws.split();
    let mut oneshots: HashMap<String, oneshot::Sender<ServerResponse>> = Default::default();

    let (agent_tx, agent_rx) = mpsc::unbounded_channel();
    let mut agent_rx = UnboundedReceiverStream::new(agent_rx);

    agent_counter(agent_tx.clone());
    for id in observe {
        agent_getstatus(agent_tx.clone(), id);
    }

    dbg!("start select looop");
    loop {
        tokio::select! {
            Some(msg) = agent_rx.next() => {
                let id = uuid::Uuid::new_v4().simple().to_string();
                oneshots.insert(id.clone(), msg.sender);
                let res = AgentMessage::Request {id, message: msg.request};
                socket_tx.send(res.into_message()).await.unwrap();
            },
            Some(message) = socket_rx.next() => {
                match ServerMessage::from_message(message.unwrap()) {
                    ServerMessage::Request{id, message} => {
                        let response = AgentMessage::Response {
                            message: handle_server_content(message),
                            id,
                        };
                        socket_tx.send(response.into_message()).await.unwrap();

                    },
                    ServerMessage::Response{ id, message } => {
                        let os = oneshots.remove(&id).unwrap();
                        os.send(message).unwrap();
                    },
                }
            },
        }
    }
}

fn get_status() -> PatientStatus {
    PatientStatus::Lying
}
