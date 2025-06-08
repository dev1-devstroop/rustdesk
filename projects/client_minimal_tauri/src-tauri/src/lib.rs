use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrameData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServerMessage {
    #[serde(rename = "type")]
    msg_type: String,
    frame: Option<FrameData>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClientMessage {
    #[serde(rename = "type")]
    msg_type: String,
    mode: Option<String>,
    app_name: Option<String>,
    input_type: Option<String>,
    x: Option<i32>,
    y: Option<i32>,
    button: Option<String>,
    key: Option<String>,
}

type ConnectionState = Arc<Mutex<Option<mpsc::UnboundedSender<Message>>>>;

#[tauri::command]
async fn connect_to_server(
    app: tauri::AppHandle,
    connection: State<'_, ConnectionState>,
    url: String,
) -> Result<String, String> {
    let (ws_stream, _) = connect_async(&url).await.map_err(|e| e.to_string())?;
    let (write, mut read) = ws_stream.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    
    // Store sender for sending messages
    *connection.lock().unwrap() = Some(tx);
    
    // Spawn task to handle outgoing messages
    let write = Arc::new(Mutex::new(write));
    let write_clone = write.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(mut writer) = write_clone.lock() {
                let _ = futures_util::SinkExt::send(&mut *writer, msg).await;
            }
        }
    });
    
    // Spawn task to handle incoming messages
    tokio::spawn(async move {
        use futures_util::StreamExt;
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(server_msg) = serde_json::from_str::<ServerMessage>(&text) {
                        if let Err(e) = app.emit("frame-update", &server_msg) {
                            log::error!("Failed to emit frame-update: {}", e);
                        }
                    }
                }
                Ok(Message::Binary(data)) => {
                    // Handle binary frame data
                    if data.len() >= 8 {
                        let width = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                        let height = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                        let frame_data = FrameData {
                            width,
                            height,
                            data: data[8..].to_vec(),
                        };
                        let server_msg = ServerMessage {
                            msg_type: "frame".to_string(),
                            frame: Some(frame_data),
                            error: None,
                        };
                        if let Err(e) = app.emit("frame-update", &server_msg) {
                            log::error!("Failed to emit frame-update: {}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });
    
    Ok("Connected successfully".to_string())
}

#[tauri::command]
async fn disconnect_from_server(connection: State<'_, ConnectionState>) -> Result<String, String> {
    *connection.lock().unwrap() = None;
    Ok("Disconnected".to_string())
}

#[tauri::command]
async fn send_message(
    connection: State<'_, ConnectionState>,
    message: ClientMessage,
) -> Result<String, String> {
    if let Some(sender) = connection.lock().unwrap().as_ref() {
        let json = serde_json::to_string(&message).map_err(|e| e.to_string())?;
        sender
            .send(Message::Text(json))
            .map_err(|e| e.to_string())?;
        Ok("Message sent".to_string())
    } else {
        Err("Not connected".to_string())
    }
}

#[tauri::command]
async fn switch_mode(
    connection: State<'_, ConnectionState>,
    mode: String,
    app_name: Option<String>,
) -> Result<String, String> {
    let message = ClientMessage {
        msg_type: "switch_mode".to_string(),
        mode: Some(mode),
        app_name,
        input_type: None,
        x: None,
        y: None,
        button: None,
        key: None,
    };
    send_message(connection, message).await
}

#[tauri::command]
async fn send_input(
    connection: State<'_, ConnectionState>,
    input_type: String,
    x: Option<i32>,
    y: Option<i32>,
    button: Option<String>,
    key: Option<String>,
) -> Result<String, String> {
    let message = ClientMessage {
        msg_type: "input".to_string(),
        mode: None,
        app_name: None,
        input_type: Some(input_type),
        x,
        y,
        button,
        key,
    };
    send_message(connection, message).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ConnectionState::default())
        .invoke_handler(tauri::generate_handler![
            connect_to_server,
            disconnect_from_server,
            send_message,
            switch_mode,
            send_input
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
