use failure::Error;
use log::info;
use structopt::StructOpt;
use tokio::runtime::Runtime;

mod config;
mod relayer;
mod server;
mod network;

use config::{generate_bridge_config, BridgeConfigGenOpt};
use relayer::{run_relayer, RelayerOpt};
use server::{run_bridge_server, BridgeServerOpt};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "FastPay Bridge",
    about = "A Byzantine fault tolerant cross-chain bridge based on FastPay"
)]
struct Opt {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(Debug, StructOpt)]
enum Command {
    /// Run a bridge authority server
    #[structopt(name = "server")]
    Server(BridgeServerOpt),
    
    /// Run a bridge relayer
    #[structopt(name = "relayer")]
    Relayer(RelayerOpt),
    
    /// Generate bridge configuration
    #[structopt(name = "generate-config")]
    GenerateConfig(BridgeConfigGenOpt),
}

fn main() -> Result<(), Error> {
    env_logger::init();
    let opt = Opt::from_args();
    
    let runtime = Runtime::new().unwrap();
    
    match opt.cmd {
        Command::Server(server_opt) => {
            info!("Starting bridge authority server");
            runtime.block_on(run_bridge_server(server_opt))?;
        }
        Command::Relayer(relayer_opt) => {
            info!("Starting bridge relayer");
            runtime.block_on(run_relayer(relayer_opt))?;
        }
        Command::GenerateConfig(config_opt) => {
            info!("Generating bridge configuration");
            runtime.block_on(generate_bridge_config(config_opt))?;
        }
    }
    
    Ok(())
}