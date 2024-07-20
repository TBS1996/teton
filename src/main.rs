mod common;

#[cfg(not(feature = "server"))]
mod agents;

#[cfg(feature = "server")]
mod server;

#[cfg(feature = "server")]
#[tokio::main]
async fn main() {
    server::run_server().await;
}

#[cfg(not(feature = "server"))]
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("please specify id of agent. e.g. 'cargo run -- agent_id'");
        return;
    }
    let id = args[1].to_string();
    agents::run(id).await;
}
