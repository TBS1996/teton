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
use tokio::sync::mpsc::UnboundedReceiver;

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

// PoC to show that agents can make requests to server.
fn agent_counter() -> UnboundedReceiver<(AgentRequest, oneshot::Sender<ServerResponse>)> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            sleep(5).await;
            let (txx, rxx) = oneshot::channel();
            let inside = (AgentRequest::GetQty, txx);
            tx.send(inside).unwrap();
            let wth = rxx.await.unwrap();
            eprintln!("total agents: {:?}", wth);
        }
    });

    rx
}

pub async fn run(id: String) {
    dbg!("running agent", &id);

    let url = format!("ws://{}/ws?agent_id={}", common::ADDR, id);
    let (ws, _) = connect_async(url).await.expect("Failed to connect");
    let (mut tx, mut rx) = ws.split();
    let mut oneshots: HashMap<String, oneshot::Sender<ServerResponse>> = Default::default();

    let mut rec = UnboundedReceiverStream::new(agent_counter());

    loop {
        tokio::select! {
            Some(msg) = rec.next() => {
                let (req, os) = msg;
                let id = uuid::Uuid::new_v4().simple().to_string();
                oneshots.insert(id.clone(), os);
                let res = AgentMessage::Request {id, message: req};
                tx.send(res.into_message()).await.unwrap();
            },
            Some(message) = rx.next() => {
                match ServerMessage::from_message(message.unwrap()) {
                    ServerMessage::Request{id, message} => {
                        let response = AgentMessage::Response {
                            message: handle_server_content(message),
                            id,
                        };
                        tx.send(response.into_message()).await.unwrap();

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
