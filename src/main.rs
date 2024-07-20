use clap::Parser;
use futures::future::join_all;

mod common;

#[cfg(not(feature = "server"))]
mod agents;

#[cfg(feature = "server")]
mod server;

#[derive(Parser, Debug)]
struct Args {
    /// IDs of agents to run
    #[arg(required = true)]
    agents: Vec<String>,

    /// IDs to observe
    #[arg(long)]
    observe: Vec<String>,
}

#[cfg(feature = "server")]
#[tokio::main]
async fn main() {
    server::run_server().await;
}

#[cfg(not(feature = "server"))]
#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.agents.is_empty() {
        println!("Please specify the ID of at least one agent. e.g., 'cargo run -- agent_id'");
        return;
    }

    let mut agent_futures = vec![];
    for agent in args.agents {
        agent_futures.push(agents::run(agent, args.observe.clone()));
    }

    join_all(agent_futures).await;

    println!("All agents terminated");
}

