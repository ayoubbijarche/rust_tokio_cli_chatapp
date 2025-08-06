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

        let tx_clone = tx.clone();
        let username_clone = username.clone();
        let clients_clone = clients.clone();
        let addr_clone = addr.clone();

        let read_task = tokio::spawn(async move {
            while let Some(result) = framed.next().await {
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

        let write_task = tokio::spawn(async move {
            while let Ok(msg) = rx.recv().await {

                /*
                if msg.Username != username {
                    let json_msg = serde_json::to_string(&msg).unwrap();
                    if let Err(e) = framed.send(json_msg).await {
                        println!("Error sending to client: {}", e);
                        break;
                    }
                }
                */
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



struct ChatClient{
    username : String
}

impl ChatClient{
    
    fn new(username : String) -> Self {
        Self { username }
    }

    async fn connect

}



#[tokio::main]

async fn main() -> anyhow::Result<()> {
    Ok(())
}
