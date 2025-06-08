use tokio_tungstenite::WebSocketStream;
use tokio::net::TcpStream;
use tokio::sync::{RwLock, mpsc};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use anyhow::Result;
use uuid::Uuid;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};

use crate::desktop_stream::DesktopStreamer;
use crate::app_stream::AppStreamer;

#[derive(Debug, Clone)]
pub enum StreamMode {
    Desktop { screen_id: u32 },
    Application {
        command: String,
        args: Vec<String>,
        workdir: Option<String>,
        isolate_files: bool,
    },
    Hybrid,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    SetMode {
        mode: String,
        screen_id: Option<u32>,
        command: Option<String>,
        args: Option<Vec<String>>,
        workdir: Option<String>,
        isolate_files: Option<bool>,
    },
    MouseMove { x: i32, y: i32 },
    MouseClick { button: u8, pressed: bool },
    KeyPress { key: String, pressed: bool },
    Ping,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Frame {
        width: u32,
        height: u32,
        data: Vec<u8>,
    },
    ModeSet { success: bool, message: String },
    Pong,
    Error { message: String },
}

pub struct Session {
    pub id: Uuid,
    pub addr: SocketAddr,
    pub mode: StreamMode,
    pub ws_stream: WebSocketStream<TcpStream>,
    pub desktop_streamer: Option<DesktopStreamer>,
    pub app_streamer: Option<AppStreamer>,
}

impl Session {
    pub fn new(
        id: Uuid,
        addr: SocketAddr,
        mode: StreamMode,
        ws_stream: WebSocketStream<TcpStream>,
    ) -> Self {
        Self {
            id,
            addr,
            mode,
            ws_stream,
            desktop_streamer: None,
            app_streamer: None,
        }
    }

    pub async fn initialize_streamers(&mut self) -> Result<()> {
        match &self.mode {
            StreamMode::Desktop { screen_id } => {
                self.desktop_streamer = Some(DesktopStreamer::new(*screen_id, 30)?);
                log::info!("Initialized desktop streamer for session {}", self.id);
            }
            StreamMode::Application { command, args, workdir, isolate_files } => {
                let mut app_streamer = AppStreamer::new(
                    command.clone(),
                    args.clone(),
                    workdir.clone(),
                    *isolate_files,
                    self.id,
                )?;
                app_streamer.start_application()?;
                self.app_streamer = Some(app_streamer);
                log::info!("Initialized app streamer for session {}", self.id);
            }
            StreamMode::Hybrid => {
                // Wait for client to specify mode
                log::info!("Session {} in hybrid mode, waiting for client to specify mode", self.id);
            }
        }
        Ok(())
    }

    pub async fn handle_message(&mut self, message: ClientMessage) -> Result<Option<ServerMessage>> {
        match message {
            ClientMessage::SetMode { mode, screen_id, command, args, workdir, isolate_files } => {
                if !matches!(self.mode, StreamMode::Hybrid) {
                    return Ok(Some(ServerMessage::Error {
                        message: "Mode can only be set in hybrid mode".to_string(),
                    }));
                }

                match mode.as_str() {
                    "desktop" => {
                        let screen_id = screen_id.unwrap_or(0);
                        self.mode = StreamMode::Desktop { screen_id };
                        self.desktop_streamer = Some(DesktopStreamer::new(screen_id, 30)?);
                        Ok(Some(ServerMessage::ModeSet {
                            success: true,
                            message: "Desktop mode set".to_string(),
                        }))
                    }
                    "app" => {
                        if let Some(command) = command {
                            self.mode = StreamMode::Application {
                                command: command.clone(),
                                args: args.unwrap_or_default(),
                                workdir,
                                isolate_files: isolate_files.unwrap_or(false),
                            };
                            
                            let mut app_streamer = AppStreamer::new(
                                command,
                                args.unwrap_or_default(),
                                workdir,
                                isolate_files.unwrap_or(false),
                                self.id,
                            )?;
                            app_streamer.start_application()?;
                            self.app_streamer = Some(app_streamer);
                            
                            Ok(Some(ServerMessage::ModeSet {
                                success: true,
                                message: "Application mode set".to_string(),
                            }))
                        } else {
                            Ok(Some(ServerMessage::Error {
                                message: "Command required for app mode".to_string(),
                            }))
                        }
                    }
                    _ => Ok(Some(ServerMessage::Error {
                        message: "Invalid mode. Use 'desktop' or 'app'".to_string(),
                    })),
                }
            }
            ClientMessage::Ping => Ok(Some(ServerMessage::Pong)),
            ClientMessage::MouseMove { x: _, y: _ } => {
                // TODO: Implement mouse input handling
                Ok(None)
            }
            ClientMessage::MouseClick { button: _, pressed: _ } => {
                // TODO: Implement mouse click handling
                Ok(None)
            }
            ClientMessage::KeyPress { key: _, pressed: _ } => {
                // TODO: Implement keyboard input handling
                Ok(None)
            }
        }
    }

    pub async fn capture_frame(&mut self) -> Result<Option<ServerMessage>> {
        match &mut self.desktop_streamer {
            Some(desktop_streamer) => {
                if let Some(frame_data) = desktop_streamer.capture_frame()? {
                    let (width, height) = desktop_streamer.get_dimensions();
                    return Ok(Some(ServerMessage::Frame {
                        width,
                        height,
                        data: frame_data,
                    }));
                }
            }
            None => {}
        }

        match &mut self.app_streamer {
            Some(app_streamer) => {
                if app_streamer.is_running() {
                    if let Some(frame_data) = app_streamer.capture_window_frame()? {
                        // For now, use fixed dimensions (in real implementation, get from window)
                        return Ok(Some(ServerMessage::Frame {
                            width: 800,
                            height: 600,
                            data: frame_data,
                        }));
                    }
                } else {
                    log::info!("Application process has stopped for session {}", self.id);
                    return Ok(Some(ServerMessage::Error {
                        message: "Application has stopped".to_string(),
                    }));
                }
            }
            None => {}
        }

        Ok(None)
    }
}

pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<Uuid, Session>>>,
    max_connections: usize,
}

impl SessionManager {
    pub fn new(max_connections: usize) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            max_connections,
        }
    }

    pub async fn add_session(&self, mut session: Session) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        
        if sessions.len() >= self.max_connections {
            return Err(anyhow::anyhow!("Maximum connections reached"));
        }

        session.initialize_streamers().await?;
        sessions.insert(session.id, session);
        
        log::info!("Added session, total sessions: {}", sessions.len());
        Ok(())
    }

    pub async fn remove_session(&self, session_id: Uuid) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(&session_id);
        log::info!("Removed session {}, total sessions: {}", session_id, sessions.len());
    }

    pub async fn run_session(&self, session_id: Uuid) -> Result<()> {
        let (tx, mut rx) = mpsc::channel(100);
        
        // Spawn frame capture task
        let sessions_clone = Arc::clone(&self.sessions);
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(33)); // ~30 FPS
            
            loop {
                interval.tick().await;
                
                let sessions = sessions_clone.read().await;
                if let Some(session) = sessions.get(&session_id) {
                    // We can't mutably borrow in a read guard, so we'll need a different approach
                    // For now, we'll handle frame capture in the main loop
                    drop(sessions);
                    
                    let mut sessions = sessions_clone.write().await;
                    if let Some(session) = sessions.get_mut(&session_id) {
                        if let Ok(Some(frame_msg)) = session.capture_frame().await {
                            let _ = tx_clone.send(frame_msg).await;
                        }
                    }
                } else {
                    break;
                }
            }
        });

        // Main message handling loop
        loop {
            let mut sessions = self.sessions.write().await;
            let session = match sessions.get_mut(&session_id) {
                Some(s) => s,
                None => break,
            };

            tokio::select! {
                // Handle incoming WebSocket messages
                ws_msg = session.ws_stream.next() => {
                    match ws_msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                                if let Ok(Some(response)) = session.handle_message(client_msg).await {
                                    let response_text = serde_json::to_string(&response).unwrap();
                                    let _ = session.ws_stream.send(Message::Text(response_text)).await;
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            log::info!("Client disconnected: {}", session_id);
                            break;
                        }
                        Some(Err(e)) => {
                            log::error!("WebSocket error for session {}: {}", session_id, e);
                            break;
                        }
                        _ => {}
                    }
                }
                
                // Handle outgoing frame messages
                frame_msg = rx.recv() => {
                    if let Some(frame) = frame_msg {
                        let frame_text = serde_json::to_string(&frame).unwrap();
                        if session.ws_stream.send(Message::Text(frame_text)).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        self.remove_session(session_id).await;
        Ok(())
    }
}
