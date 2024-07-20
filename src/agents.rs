use crate::common;
use common::{AgentContent, PatientStatus, Request, Response, ServerContent};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;

pub async fn run(id: String) {
    dbg!("running agent", &id);

    let url = format!("ws://{}/ws?agent_id={}", common::ADDR, id);
    let (ws, _) = connect_async(url).await.expect("Failed to connect");
    let (mut tx, mut rx) = ws.split();

    loop {
        tokio::select! {
            Some(message) = rx.next() => {
                if let Ok(msg) = message {
                    let msg = Request::from_message(msg).unwrap();

                    match msg.message {
                        ServerContent::Close(msg) => {
                            eprintln!("connection closed: {}", msg);
                            return;
                        }
                        ServerContent::GetStatus => {
                            let status = get_status();
                            let msg = Response {
                                message: AgentContent::Status(status),
                                id: msg.id,
                            };
                            tx.send(msg.into_message()).await.unwrap();
                        },
                    }
                }
            },
        }
    }
}

fn get_status() -> PatientStatus {
    PatientStatus::Lying
}
