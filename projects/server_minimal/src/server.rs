use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, WebSocketStream};
use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use uuid::Uuid;

use crate::session::{Session, SessionManager, StreamMode};

pub async fn start_desktop_server(
    bind_addr: SocketAddr,
    max_connections: usize,
    screen_id: u32,
) -> Result<()> {
    let session_manager = Arc::new(SessionManager::new(max_connections));
    let listener = TcpListener::bind(bind_addr).await?;
    
    log::info!("Desktop server listening on {}", bind_addr);

    while let Ok((stream, addr)) = listener.accept().await {
        let session_manager = Arc::clone(&session_manager);
        
        tokio::spawn(async move {
            if let Err(e) = handle_desktop_connection(stream, addr, session_manager, screen_id).await {
                log::error!("Desktop connection error from {}: {}", addr, e);
            }
        });
    }

    Ok(())
}

pub async fn start_app_server(
    bind_addr: SocketAddr,
    max_connections: usize,
    command: String,
    args: Vec<String>,
    workdir: Option<String>,
    isolate_files: bool,
) -> Result<()> {
    let session_manager = Arc::new(SessionManager::new(max_connections));
    let listener = TcpListener::bind(bind_addr).await?;
    
    log::info!("App server listening on {}", bind_addr);

    while let Ok((stream, addr)) = listener.accept().await {
        let session_manager = Arc::clone(&session_manager);
        let command = command.clone();
        let args = args.clone();
        let workdir = workdir.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_app_connection(
                stream, 
                addr, 
                session_manager, 
                command, 
                args, 
                workdir, 
                isolate_files
            ).await {
                log::error!("App connection error from {}: {}", addr, e);
            }
        });
    }

    Ok(())
}

pub async fn start_hybrid_server(
    bind_addr: SocketAddr,
    max_connections: usize,
) -> Result<()> {
    let session_manager = Arc::new(SessionManager::new(max_connections));
    let listener = TcpListener::bind(bind_addr).await?;
    
    log::info!("Hybrid server listening on {}", bind_addr);

    while let Ok((stream, addr)) = listener.accept().await {
        let session_manager = Arc::clone(&session_manager);
        
        tokio::spawn(async move {
            if let Err(e) = handle_hybrid_connection(stream, addr, session_manager).await {
                log::error!("Hybrid connection error from {}: {}", addr, e);
            }
        });
    }

    Ok(())
}

async fn handle_desktop_connection(
    stream: TcpStream,
    addr: SocketAddr,
    session_manager: Arc<SessionManager>,
    screen_id: u32,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let session_id = Uuid::new_v4();
    
    log::info!("New desktop session {} from {}", session_id, addr);
    
    let session = Session::new(
        session_id,
        addr,
        StreamMode::Desktop { screen_id },
        ws_stream,
    );
    
    session_manager.add_session(session).await?;
    session_manager.run_session(session_id).await?;
    
    Ok(())
}

async fn handle_app_connection(
    stream: TcpStream,
    addr: SocketAddr,
    session_manager: Arc<SessionManager>,
    command: String,
    args: Vec<String>,
    workdir: Option<String>,
    isolate_files: bool,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let session_id = Uuid::new_v4();
    
    log::info!("New app session {} from {} for command: {}", session_id, addr, command);
    
    let session = Session::new(
        session_id,
        addr,
        StreamMode::Application {
            command,
            args,
            workdir,
            isolate_files,
        },
        ws_stream,
    );
    
    session_manager.add_session(session).await?;
    session_manager.run_session(session_id).await?;
    
    Ok(())
}

async fn handle_hybrid_connection(
    stream: TcpStream,
    addr: SocketAddr,
    session_manager: Arc<SessionManager>,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let session_id = Uuid::new_v4();
    
    log::info!("New hybrid session {} from {}", session_id, addr);
    
    // Wait for client to specify mode via initial message
    let session = Session::new(
        session_id,
        addr,
        StreamMode::Hybrid,
        ws_stream,
    );
    
    session_manager.add_session(session).await?;
    session_manager.run_session(session_id).await?;
    
    Ok(())
}
