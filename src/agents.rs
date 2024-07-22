use crate::common;
use common::AgentID;
use common::AgentMessage;
use common::AgentRequest;
use common::AlarmTriggerResult;
use common::RequestID;
use common::ServerMessage;
use common::ServerResponse;
use common::{PatientStatus, ServerRequest};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_tungstenite::connect_async;

fn trigger_alarm() -> AlarmTriggerResult {
    use AlarmTriggerResult as RES;
    match current_unix() % 10 {
        0 => RES::Failure("battery low".to_string()),
        1 => RES::Failure("communication failure".to_string()),
        2 => RES::Failure("alarm disabled".to_string()),
        3 => RES::Failure("alarm turned off".to_string()),
        _ => RES::Success,
    }
}

fn handle_server_request(id: RequestID, content: ServerRequest) -> AgentMessage {
    match content {
        ServerRequest::GetStatus => {
            let status = get_status();
            AgentMessage::new_response(id, status)
        }

        ServerRequest::TriggerAlarm => {
            let res = trigger_alarm();
            AgentMessage::new_response(id, res)
        }
    }
}

async fn sleep(secs: u64) {
    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
}

#[derive(Clone)]
pub struct Agent {
    id: AgentID,
    tx: UnboundedSender<AgentMessage>,
    oneshots: Arc<Mutex<HashMap<RequestID, oneshot::Sender<ServerResponse>>>>,
}

impl Agent {
    pub async fn start(id: AgentID, observer: Vec<AgentID>) {
        tracing::info!("starting agent: {}", &id);

        let (tx, rx) = mpsc::unbounded_channel();
        let rx = UnboundedReceiverStream::new(rx);
        let s = Self {
            id,
            tx,
            oneshots: Default::default(),
        };

        for id in observer {
            s.agent_getstatus(id);
        }

        s.agent_counter();

        s.run(rx).await
    }

    async fn send<T: for<'de> Deserialize<'de>>(&self, req: AgentRequest) -> Result<T, String> {
        let (sender, rx) = oneshot::channel();

        let id = uuid::Uuid::new_v4().simple().to_string();
        self.oneshots.lock().await.insert(id.clone(), sender);
        let res = AgentMessage::Request { id, message: req };

        self.tx.send(res).map_err(|e| e.to_string())?;

        let res = rx.await.map_err(|e| e.to_string())?;

        serde_json::from_slice(&res).map_err(|e| e.to_string())
    }

    async fn handle_response(&self, id: String, data: ServerResponse) {
        let os = match self.oneshots.lock().await.remove(&id) {
            Some(os) => os,
            None => {
                tracing::error!("id not found in oneshots: {}", id);
                return;
            }
        };

        if os.send(data).is_err() {
            tracing::error!("failed to send data to oneshot. id: {}", id);
        }
    }

    fn agent_getstatus(&self, id: AgentID) {
        tracing::info!("getting status of: {}", &id);

        let sender = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(5).await;
                let status: Option<PatientStatus> =
                    match sender.send(AgentRequest::AgentStatus(id.clone())).await {
                        Ok(status) => status,
                        Err(_) => return,
                    };

                match status {
                    Some(status) => tracing::info!("{}-status: {:?}", &id, &status),
                    None => tracing::error!("no agent connected with following id: {}", &id),
                }
            }
        });
    }

    // PoC to show that agents can make requests to server.
    fn agent_counter(&self) {
        let sender = self.clone();
        tokio::spawn(async move {
            loop {
                sleep(5).await;
                let res: usize = match sender.send(AgentRequest::GetQty).await {
                    Ok(res) => res,
                    Err(_) => return,
                };
                tracing::info!("total agents: {}", res);
            }
        });
    }

    pub async fn run(&self, mut rx: UnboundedReceiverStream<AgentMessage>) {
        let (mut socket_tx, mut socket_rx) = {
            let url = format!("ws://{}/ws?agent_id={}", common::ADDR, &self.id);
            let (ws, _) = connect_async(url)
                .await
                .expect("Failed to connect to server");
            ws.split()
        };

        loop {
            tokio::select! {
                Ok(()) = tokio::signal::ctrl_c() => {
                    let _ = socket_tx.send(AgentMessage::Closing("Received sigint".into()).into_message()).await;
                    return;
                },

                Some(msg) = rx.next() => {
                    if let Err(e) = socket_tx.send(msg.into_message()).await {
                        tracing::error!("failed to send message to server: {}", e);
                    }
                },
                Some(Ok(message)) = socket_rx.next() => {
                    match ServerMessage::from_message(message) {
                        ServerMessage::Request{id, message} => {
                            let response = handle_server_request(id, message);
                            if let Err(e) = socket_tx.send(response.into_message()).await {
                                tracing::error!("failed to send message to server: {}", e);
                            }

                        },
                        ServerMessage::Response{ id, data } => {
                            self.handle_response(id, data).await;
                        },

                        ServerMessage::Terminate(reason) => {
                            tracing::warn!("{}: shutting down: {}", &self.id, reason);
                            return;
                        }
                    }
                },
            }
        }
    }
}

fn get_status() -> PatientStatus {
    if current_unix() % 2 == 0 {
        PatientStatus::Lying
    } else {
        PatientStatus::Sitting
    }
}

fn current_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
