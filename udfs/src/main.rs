use serde_json::Value;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::{stderr, stdin, stdout, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{self, debug};
use udfs::completions::completions;
use udfs::{init_tracing, InvokeError};

async fn process_line(
    udf_name: &str,
    input: String,
    writer: Arc<Mutex<impl AsyncWriteExt + Send + Unpin>>,
) -> Result<(), InvokeError> {
    debug!(
        "Parsing CLI arguments: {:?}",
        std::env::args().collect::<Vec<_>>()
    );

    let last = std::env::args().last().unwrap_or_default().replace("'", "");
    let val = match &udf_name {
        &"completions" => {
            tracing::debug!("Calling completions with input: {}", input);
            completions(input, &last).await
        }
        x => {
            tracing::error!("Unsupported UDF: {}", x);
            Err(InvokeError::Unsupported(x.to_string()))
        }
    }?;

    let values: Vec<Value> = vec![val.into()];
    let mut writer = writer.lock().await;
    write(&mut *writer, values).await
}

async fn execute_udf<R, W>(udf: &str, mut reader: R, writer: W) -> Result<(), InvokeError>
where
    R: tokio::io::AsyncBufRead + std::marker::Unpin,
    W: AsyncWriteExt + std::marker::Unpin + Send + 'static,
{
    let writer = Arc::new(Mutex::new(writer));
    let mut handles = vec![];
    let mut line = String::new();

    while let Ok(n) = reader.read_line(&mut line).await {
        if n == 0 {
            break;
        }

        let line_clone = line.clone();
        let writer_clone = writer.clone();
        let udf = udf.to_string();

        let handle =
            tokio::spawn(async move { process_line(&udf, line_clone, writer_clone).await });

        handles.push(handle);
        line = String::new();

        // If we have too many pending tasks, wait for some to complete
        if handles.len() >= 100 {
            // Wait for any completed task
            let (result, _, remaining) = futures::future::select_all(handles).await;
            handles = remaining;
            result??;
        }
    }

    // Wait for any remaining tasks to complete
    for handle in handles {
        handle.await??;
    }

    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), InvokeError> {
    // Initialize tracing once at program start
    if let Err(e) = init_tracing(Some("debug")) {
        eprintln!("Failed to initialize tracing: {}", e);
    }

    let stdin = stdin();
    let stdout = stdout();
    let mut stderr = stderr();
    let reader = BufReader::new(stdin);
    let args: Vec<String> = std::env::args().collect();
    let udf_str = &args[1].trim().to_string();

    // Wrap stdout in a BufWriter to make it cloneable
    let writer = tokio::io::BufWriter::new(stdout);

    let res = execute_udf(&udf_str, reader, writer).await;
    if let Err(e) = res {
        stderr.write_all(format!("{e}").as_bytes()).await?;
        stderr.flush().await?;
    }
    Ok(())
}

pub async fn write<W>(writer: &mut W, msg: Vec<Value>) -> udfs::Result<()>
where
    W: AsyncWriteExt + std::marker::Unpin,
{
    let msg = serde_json::to_string(&msg)?;
    writer.write_all(msg.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}
