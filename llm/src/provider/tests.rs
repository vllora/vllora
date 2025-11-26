use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

/// Mock server that responds with text/event-stream
pub struct MockStreamServer {
    port: u16,
    events: Arc<Mutex<Vec<String>>>,
}

impl MockStreamServer {
    /// Start a new mock server on an available port
    pub async fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("0.0.0.0:9999").await?;
        let addr = listener.local_addr()?;
        let port = addr.port();
        let events = Arc::new(Mutex::new(Vec::new()));

        let events_clone = events.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, _)) => {
                        let events = events_clone.clone();
                        tokio::spawn(async move {
                            use tokio::io::AsyncReadExt;

                            // Read the entire request with a timeout
                            let read_future = async {
                                let mut request_data = Vec::new();
                                let mut buffer = [0u8; 8192];

                                // Read until we get headers
                                loop {
                                    let size = stream.read(&mut buffer).await?;
                                    if size == 0 {
                                        break;
                                    }
                                    request_data.extend_from_slice(&buffer[..size]);

                                    // Check if we have complete headers
                                    if let Some(headers_end) =
                                        request_data.windows(4).rposition(|w| w == b"\r\n\r\n")
                                    {
                                        // Parse Content-Length
                                        let headers_str = String::from_utf8_lossy(
                                            &request_data[..=headers_end + 3],
                                        );
                                        let content_length = headers_str.lines().find_map(|line| {
                                            if line.to_lowercase().starts_with("content-length:") {
                                                line.split(':')
                                                    .nth(1)
                                                    .and_then(|s| s.trim().parse::<usize>().ok())
                                            } else {
                                                None
                                            }
                                        });

                                        // Read body if needed
                                        if let Some(body_len) = content_length {
                                            let body_start = headers_end + 4;
                                            let body_received =
                                                request_data.len().saturating_sub(body_start);

                                            if body_received < body_len {
                                                let remaining = body_len - body_received;
                                                let mut body_buffer = vec![0u8; remaining];
                                                let mut total_read = 0;

                                                // Read the remaining body bytes
                                                while total_read < remaining {
                                                    match stream
                                                        .read(&mut body_buffer[total_read..])
                                                        .await
                                                    {
                                                        Ok(0) => break, // Connection closed
                                                        Ok(size) => {
                                                            total_read += size;
                                                        }
                                                        Err(e) => return Err(e),
                                                    }
                                                }
                                                request_data
                                                    .extend_from_slice(&body_buffer[..total_read]);
                                            }
                                        }
                                        break;
                                    }
                                }

                                Ok::<Vec<u8>, std::io::Error>(request_data)
                            };

                            let request_data = match tokio::time::timeout(
                                tokio::time::Duration::from_secs(5),
                                read_future,
                            )
                            .await
                            {
                                Ok(Ok(data)) => data,
                                Ok(Err(e)) => {
                                    eprintln!("Error reading request: {}", e);
                                    return;
                                }
                                Err(_) => {
                                    eprintln!("Timeout reading request");
                                    return;
                                }
                            };

                            let request = String::from_utf8_lossy(&request_data);

                            // Check if it's a POST request (for chat completions)
                            if request.starts_with("POST") {
                                // Send HTTP response headers
                                let headers = "HTTP/1.1 200 OK\r\n\
                                    Content-Type: text/event-stream\r\n\
                                    Cache-Control: no-cache\r\n\
                                    Connection: keep-alive\r\n\
                                    Access-Control-Allow-Origin: *\r\n\r\n";

                                if let Err(e) = stream.write_all(headers.as_bytes()).await {
                                    eprintln!("Error writing headers: {}", e);
                                    return;
                                }

                                // Stream events
                                let events_guard = events.lock().await;
                                for event in events_guard.iter() {
                                    let sse_data = format!("data: {}\n\n", event);
                                    if let Err(e) = stream.write_all(sse_data.as_bytes()).await {
                                        eprintln!("Error writing event: {}", e);
                                        break;
                                    }
                                    // Small delay to simulate streaming
                                    tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                        .await;
                                }

                                // Send [DONE] to signal end of stream
                                if let Err(e) = stream.write_all(b"data: [DONE]\n\n").await {
                                    eprintln!("Error writing [DONE]: {}", e);
                                }
                            } else {
                                // Send 404 for non-POST requests
                                let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                                let _ = stream.write_all(response.as_bytes()).await;
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {}", e);
                    }
                }
            }
        });

        Ok(Self { port, events })
    }

    /// Get the base URL of the mock server
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// Set the events to stream
    pub async fn set_events(&self, events: Vec<String>) {
        let mut guard = self.events.lock().await;
        *guard = events;
    }

    /// Add an event to stream
    pub async fn add_event(&self, event: String) {
        let mut guard = self.events.lock().await;
        guard.push(event);
    }
}
