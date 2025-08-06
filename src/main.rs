use futures_util::StreamExt;
use futures_util::sink::SinkExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader, stdin};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast};
use tokio_util::codec::{Framed, LinesCodec};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatMessage {
    Username: String,
    Content: String,
    Timestamp: u64,
}

type Clients = Arc<Mutex<HashMap<SocketAddr, String>>>;

#[derive(Debug, Clone)]
struct ChatServer {
    tx: broadcast::Sender<ChatMessage>,
    clients: Clients,
}

impl ChatServer {
    fn new() -> Self {
        let (tx, _rx) = broadcast::channel(1000);
        Self {
            tx,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn run_server(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(addr).await?;
        println!("running on {}", addr);

        loop {
            let (stream, addr) = listener.accept().await?;
            println!("a client connected : {}", addr);

            let tx = self.tx.clone();
            let clients = self.clients.clone();

            tokio::spawn(async move {
                if let Err(e) = Self::client_connection(stream, addr, tx, clients).await {
                    println!("error handling client {} due to {}", addr, e);
                }
            });
        }
    }

    async fn client_connection(
        stream: TcpStream,
        addr: SocketAddr,
        tx: broadcast::Sender<ChatMessage>,
        clients: Clients,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut framed = Framed::new(stream, LinesCodec::new());
        let mut rx = tx.subscribe();
        let line = framed.next().await.ok_or("failed to receive username")??;

        let msg: serde_json::Value =
            serde_json::from_str(&line).map_err(|_| "failed to parse username")?;

        let username = msg
            .get("username")
            .and_then(|u| u.as_str())
            .ok_or("Invalid username format")?
            .to_string();

        {
            let mut clients_guard = clients.lock().await;
            clients_guard.insert(addr, username.clone());
        }

        println!("User : '{}' joined from {}", username, addr);

        let join_msg = ChatMessage {
            Username: "System".to_string(),
            Content: format!("{} joined to chat", username),
            Timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let _ = tx.send(join_msg);

        let (mut sink, mut stream) = framed.split();

        let tx_clone = tx.clone();
        let username_clone = username.clone();
        let clients_clone = clients.clone();
        let addr_clone = addr.clone();

        let read_task = tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(line) => {
                        if let Ok(mut msg) = serde_json::from_str::<ChatMessage>(&line) {
                            msg.Username = username_clone.clone();
                            msg.Timestamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();

                            if let Err(_) = tx_clone.send(msg) {
                                println!("No receivers for broadcast");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        println!("Error reading from client {}: {}", addr_clone, e);
                        break;
                    }
                }
            }

            {
                let mut clients_guard = clients_clone.lock().await;
                clients_guard.remove(&addr_clone);
            }

            let leave_msg = ChatMessage {
                Username: "System".to_string(),
                Content: format!("{} left the chat", username_clone),
                Timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            };
            let _ = tx_clone.send(leave_msg);
        });

        let username_for_write = username.clone();
        let write_task = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {
                if msg.Username != username_for_write {
                    let json_msg = serde_json::to_string(&msg).unwrap();
                    if let Err(e) = sink.send(json_msg).await {
                        println!("Error sending to client: {}", e);
                        break;
                    }
                }
            }
        });

        tokio::select! {
            _ = read_task => {},
            _ = write_task => {},
        }

        println!("Client {} disconnected", addr);
        Ok(())
    }
}

struct ChatClient {
    username: String
}

impl ChatClient {
    fn new(username: String) -> Self {
        Self { username }
    }

    async fn connect(&self, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let stream = TcpStream::connect(addr).await?;
        println!("Connected to server at {}", addr);

        let mut framed = Framed::new(stream, LinesCodec::new());

        let username_msg = serde_json::json!({"username": self.username});
        framed.send(serde_json::to_string(&username_msg)?).await?;

        let stdin = stdin();
        let mut stdin_reader = BufReader::new(stdin).lines();

        let (mut sink, mut stream) = framed.split();

        let username_clone = self.username.clone();
        let input_task = tokio::spawn(async move {
            while let Ok(Some(line)) = stdin_reader.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }

                let msg = ChatMessage {
                    Username: username_clone.clone(),
                    Content: line.trim().to_string(),
                    Timestamp: 0,
                };

                let json_msg = serde_json::to_string(&msg).unwrap();
                if let Err(e) = sink.send(json_msg).await {
                    println!("Error sending message: {}", e);
                    break;
                }
            }
        });

        let receive_task = tokio::spawn(async move {
            while let Some(result) = stream.next().await {
                match result {
                    Ok(line) => {
                        if let Ok(msg) = serde_json::from_str::<ChatMessage>(&line) {
                            let datetime = std::time::UNIX_EPOCH +
                                std::time::Duration::from_secs(msg.Timestamp);
                            let formatted_time = format!("{:?}", datetime);

                            println!("[{}] {}: {}",
                                     formatted_time.split('.').next().unwrap_or(""),
                                     msg.Username,
                                     msg.Content
                            );
                        }
                    }
                    Err(e) => {
                        println!("Error receiving message: {}", e);
                        break;
                    }
                }
            }
        });

        tokio::select! {
            _ = input_task => {},
            _ = receive_task => {},
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!("  Server mode: {} server [port]", args[0]);
        println!("  Client mode: {} client <username> [server_addr]", args[0]);
        return Ok(());
    }

    match args[1].as_str() {
        "server" => {
            let port = args.get(2).unwrap_or(&"8080".to_string()).clone();
            let addr = format!("127.0.0.1:{}", port);

            let server = ChatServer::new();
            server.run_server(&addr).await?;
        }
        "client" => {
            if args.len() < 3 {
                println!("Please provide a username");
                return Ok(());
            }

            let username = args[2].clone();
            let server_addr = args.get(3).unwrap_or(&"127.0.0.1:8080".to_string()).clone();

            let client = ChatClient::new(username);

            println!("Connecting to {}...", server_addr);
            println!("Type messages and press Enter to send. Ctrl+C to quit.");

            client.connect(&server_addr).await?;
        }
        _ => {
            println!("Invalid mode. Use 'server' or 'client'");
        }
    }

    Ok(())
}