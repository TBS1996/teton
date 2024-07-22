# Teton fleet management assignment


## Overview

This is an assignment for creating a fleet management system, where we have one API server which can communicate with agents that are connected to the server. Instead of vehicles, the agents represent cameras in hospital that detects whether a patient is sitting on bed or lying down.

## System design

We use an axum web-server for the api-server. The agents connect using websockets. A custom request-response architecture has been created, since websockets are bidirectional by nature. 

My design allows for agents to be able to proactively send messages to the server and receive responses, instead of just receiving commands from the server. Agents can also send messages to other agents via the server.

## How to run


1. In a terminal, do `cargo run --features server`.
2. Open another terminal, do `cargo run -- foo_agent`

The agent should now be connected to the server.
foo_agent will regularly ask the server how many agents are connected, to prove that agents can also request data from the server.


get patient status by opening a new terminal and enter: "curl localhost:3000/status/foo_agent"

To see that one agent can communicate with another, open a new terminal and enter: "cargo run -- bar_agent --observe foo_agent".
All arguments after --observe will be other agents that bar_agent will check the status of.

also try: "curl localhost:3000/check/foo_agent". 

to kill an agent: "curl localhost:3000/kill/:agent_id". 
