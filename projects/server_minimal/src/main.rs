use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use anyhow::Result;

mod server;
mod desktop_stream;
mod app_stream;
mod session;
mod isolation;

#[derive(Parser)]
#[command(name = "rustdesk-server-minimal")]
#[command(about = "A minimal RustDesk server for remote desktop and application streaming")]
struct Cli {
    /// Server bind address
    #[arg(short, long, default_value = "0.0.0.0:8080")]
    bind: SocketAddr,

    /// Maximum concurrent connections
    #[arg(short, long, default_value = "10")]
    max_connections: usize,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start server in desktop mode (full screen capture)
    Desktop {
        /// Screen to capture (0 for primary)
        #[arg(short, long, default_value = "0")]
        screen: u32,
    },
    /// Start server in app mode (specific application)
    App {
        /// Application command to execute
        #[arg(short, long)]
        command: String,

        /// Arguments for the application
        #[arg(short, long)]
        args: Vec<String>,

        /// Working directory for the application
        #[arg(short, long)]
        workdir: Option<String>,

        /// Enable file isolation for each client
        #[arg(long)]
        isolate_files: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Desktop { screen }) => {
            log::info!("Starting desktop streaming server on {} for screen {}", cli.bind, screen);
            server::start_desktop_server(cli.bind, cli.max_connections, *screen).await?;
        }
        Some(Commands::App { command, args, workdir, isolate_files }) => {
            log::info!("Starting app streaming server on {} for command: {}", cli.bind, command);
            server::start_app_server(
                cli.bind,
                cli.max_connections,
                command.clone(),
                args.clone(),
                workdir.clone(),
                *isolate_files,
            ).await?;
        }
        None => {
            log::info!("Starting hybrid server on {} (supports both desktop and app modes)", cli.bind);
            server::start_hybrid_server(cli.bind, cli.max_connections).await?;
        }
    }

    Ok(())
}
