use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::broadcast,
};
use std::{error::Error, io , collections::HashMap , sync::Arc};



#[tokio::main]

async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    let mut mssg = String::new();

    io::stdin().read_line(&mut mssg).expect("couldnt read the message");
    

    loop{
        let (mut socket , addr) = listener.accept().await?;
        
        tokio::spawn(async move{
            let mut buff = [0; 1024];
            

            
            loop{
                match socket.read(&mut buff).await {
                    Ok(0)=> return,
                    Ok(n)=> {
                        if socket.write_all(&buff[..n]).await.is_err(){
                            eprintln!("failed to write to socket; addr={}" , addr);
                            return;
                        }
                    }
                    Err(e) => {
                        eprintln!("failed to read from socket; addr={} , err={}", addr , e);
                        return;
                } 
                }

            }
        });
    }

}
