use crate::common;
use common::AgentMessage;
use common::AgentRequest;
use common::ServerMessage;
use common::ServerResponse;
use common::{PatientStatus, ServerRequest};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::sync::oneshot;
type RequestID = String;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::connect_async;

fn handle_server_request(id: String, content: ServerRequest) -> AgentMessage {
    match content {
        ServerRequest::Close(msg) => {
            eprintln!("connection closed: {}", msg);
            std::process::exit(0);
        }

        ServerRequest::GetStatus => {
            let status = get_status();
            AgentMessage::new_response(id, status)
        }
    }
}

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
fn agent_counter(sender: LolSender) {
    dbg!("starting agent counter thing");
    tokio::spawn(async move {
        loop {
            sleep(5).await;
            let res = sender.send(AgentRequest::GetQty).await;
            eprintln!("total agents: {:?}", res);
        }
    });
}

#[derive(Clone)]
struct LolSender {
    tx: UnboundedSender<FooBar>,
}

impl LolSender {
    fn new() -> (Self, UnboundedReceiverStream<FooBar>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let rx = UnboundedReceiverStream::new(rx);
        let s = Self { tx };

        (s, rx)
    }

    async fn send(&self, req: AgentRequest) -> ServerResponse {
        let (tx, rx) = oneshot::channel();
        let inside = FooBar::new(req, tx);
        self.tx.send(inside).unwrap();
        rx.await.unwrap()
    }
}

fn agent_getstatus(sender: LolSender, id: String) {
    eprintln!("getting status of: {}", &id);

    tokio::spawn(async move {
        loop {
            sleep(5).await;
            let res = sender.send(AgentRequest::AgentStatus(id.clone())).await;
            let status: Option<PatientStatus> = serde_json::from_slice(&res).unwrap();

            match status {
                Some(status) => println!("{}-status: {:?}", &id, &status),
                None => println!("no agent connected with following id: {}", &id),
            }
        }
    });
}

pub async fn run(id: String, observe: Vec<String>) {
    dbg!("running agent", &id);

    let mut oneshots: HashMap<RequestID, oneshot::Sender<ServerResponse>> = Default::default();
    let (sender, mut agent_rx) = LolSender::new();

    let (mut socket_tx, mut socket_rx) = {
        let url = format!("ws://{}/ws?agent_id={}", common::ADDR, id);
        let (ws, _) = connect_async(url).await.expect("Failed to connect");
        ws.split()
    };

    agent_counter(sender.clone());
    for id in observe {
        agent_getstatus(sender.clone(), id);
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
                        let response = handle_server_request(id, message);
                        socket_tx.send(response.into_message()).await.unwrap();

                    },
                    ServerMessage::Response{ id, data } => {
                        let os = oneshots.remove(&id).unwrap();
                        os.send(data).unwrap();
                    },
                }
            },
        }
    }
}

fn get_status() -> PatientStatus {
    PatientStatus::Lying
}
