#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::{Manager, State};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use futures_util::{SinkExt, StreamExt};

#[derive(Debug, Serialize, Deserialize)]
struct ConnectionConfig {
    host: String,
    port: u16,
    mode: String, // "desktop" or "app"
    command: Option<String>,
    args: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ServerMessage {
    Frame { width: u32, height: u32, data: Vec<u8> },
    ModeSet { success: bool, message: String },
    Error { message: String },
    Pong,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
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

type ConnectionState = Arc<Mutex<Option<WebSocketStream<MaybeTlsStream<TcpStream>>>>>;

#[tauri::command]
async fn connect_to_server(
    host: String,
    port: u16,
    connection: State<'_, ConnectionState>,
    app_handle: tauri::AppHandle,
) -> Result<String, String> {
    let url = format!("ws://{}:{}", host, port);
    
    match connect_async(&url).await {
        Ok((ws_stream, _)) => {
            *connection.lock().unwrap() = Some(ws_stream);
            
            // Start listening for messages
            let connection_clone = connection.inner().clone();
            let app_handle_clone = app_handle.clone();
            
            tokio::spawn(async move {
                listen_for_messages(connection_clone, app_handle_clone).await;
            });
            
            Ok("Connected successfully".to_string())
        }
        Err(e) => Err(format!("Connection failed: {}", e)),
    }
}

#[tauri::command]
async fn set_mode(
    mode: String,
    screen_id: Option<u32>,
    command: Option<String>,
    args: Option<Vec<String>>,
    connection: State<'_, ConnectionState>,
) -> Result<String, String> {
    let msg = ClientMessage::SetMode {
        mode,
        screen_id,
        command,
        args,
        workdir: None,
        isolate_files: Some(true),
    };
    
    send_message(connection, msg).await
}

#[tauri::command]
async fn send_mouse_move(
    x: i32,
    y: i32,
    connection: State<'_, ConnectionState>,
) -> Result<(), String> {
    let msg = ClientMessage::MouseMove { x, y };
    send_message(connection, msg).await.map(|_| ())
}

#[tauri::command]
async fn send_mouse_click(
    button: u8,
    pressed: bool,
    connection: State<'_, ConnectionState>,
) -> Result<(), String> {
    let msg = ClientMessage::MouseClick { button, pressed };
    send_message(connection, msg).await.map(|_| ())
}

#[tauri::command]
async fn send_key_press(
    key: String,
    pressed: bool,
    connection: State<'_, ConnectionState>,
) -> Result<(), String> {
    let msg = ClientMessage::KeyPress { key, pressed };
    send_message(connection, msg).await.map(|_| ())
}

async fn send_message(
    connection: State<'_, ConnectionState>,
    message: ClientMessage,
) -> Result<String, String> {
    let mut conn_guard = connection.lock().unwrap();
    
    if let Some(ws) = conn_guard.as_mut() {
        let json = serde_json::to_string(&message).map_err(|e| e.to_string())?;
        ws.send(tokio_tungstenite::tungstenite::Message::Text(json))
            .await
            .map_err(|e| e.to_string())?;
        Ok("Message sent".to_string())
    } else {
        Err("Not connected".to_string())
    }
}

async fn listen_for_messages(
    connection: ConnectionState,
    app_handle: tauri::AppHandle,
) {
    loop {
        let message = {
            let mut conn_guard = connection.lock().unwrap();
            if let Some(ws) = conn_guard.as_mut() {
                ws.next().await
            } else {
                break;
            }
        };

        match message {
            Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                    match server_msg {
                        ServerMessage::Frame { width, height, data } => {
                            // Emit frame data to frontend
                            app_handle.emit_all("frame", FrameData { width, height, data }).ok();
                        }
                        ServerMessage::ModeSet { success, message } => {
                            app_handle.emit_all("mode_set", ModeSetData { success, message }).ok();
                        }
                        ServerMessage::Error { message } => {
                            app_handle.emit_all("error", ErrorData { message }).ok();
                        }
                        ServerMessage::Pong => {
                            // Handle pong if needed
                        }
                    }
                }
            }
            Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => {
                app_handle.emit_all("disconnected", ()).ok();
                break;
            }
            Some(Err(e)) => {
                app_handle.emit_all("error", ErrorData { message: e.to_string() }).ok();
                break;
            }
            _ => {}
        }
    }
    
    // Clear connection
    *connection.lock().unwrap() = None;
}

#[derive(Serialize)]
struct FrameData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

#[derive(Serialize)]
struct ModeSetData {
    success: bool,
    message: String,
}

#[derive(Serialize)]
struct ErrorData {
    message: String,
}

fn main() {
    tauri::Builder::default()
        .manage(ConnectionState::default())
        .invoke_handler(tauri::generate_handler![
            connect_to_server,
            set_mode,
            send_mouse_move,
            send_mouse_click,
            send_key_press
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
