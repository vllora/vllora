use serde_json::Value;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::{stderr, stdin, stdout, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tracing::{self, debug};
use udfs::completions::completions;
use udfs::embed::embed;
use udfs::init_tracing;
use udfs::parse_function_config;
use udfs::InvokeError;

use udfs::FunctionConfig;
async fn process_line(
    config: &FunctionConfig,
    values: &mut std::slice::Iter<'_, String>,
    writer: Arc<Mutex<impl AsyncWriteExt + Send + Unpin>>,
    tokens: Arc<AtomicUsize>,
) -> Result<(), InvokeError> {
    debug!(
        "Parsing CLI arguments: {:?}",
        std::env::args().collect::<Vec<_>>()
    );

    let val = match &config {
        FunctionConfig::Completion(config) => completions(values, config).await,

        FunctionConfig::Embedding(config) => embed(values, config).await,
    }?;
    let max_tokens = config.max_tokens();
    if let Some(max_tokens) = max_tokens {
        let val = tokens.fetch_add(val.usage.total_tokens, std::sync::atomic::Ordering::Relaxed);
        if val > max_tokens {
            return Err(InvokeError::CustomError(format!(
                "Total tokens: {} exceeds max tokens: {}",
                val, max_tokens
            )));
        }
    }
    let response = val.response;
    let values: Vec<Value> = vec![response];
    let mut writer = writer.lock().await;
    write(&mut *writer, values).await
}

const PARALLEL: usize = 100;
async fn execute_udf<R, W>(udf: &str, mut reader: R, writer: W) -> Result<(), InvokeError>
where
    R: tokio::io::AsyncBufRead + std::marker::Unpin,
    W: AsyncWriteExt + std::marker::Unpin + Send + 'static,
{
    let writer = Arc::new(Mutex::new(writer));
    let mut line = String::new();

    let tokens = Arc::new(AtomicUsize::new(0));
    // Create a buffer to store futures with their order
    let mut futures = Vec::with_capacity(PARALLEL);
    let mut line_number = 0u64;

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {
                if line.trim().is_empty() {
                    continue;
                }

                let line_clone = line.trim().to_string();
                let writer_clone = writer.clone();

                let tokens_clone = tokens.clone();

                let values: Vec<String> = serde_json::from_str(line_clone.as_str())?;
                let config = values.first().cloned().ok_or_else(|| {
                    InvokeError::CustomError("No configuration provided".to_string())
                })?;
                let config = parse_function_config(udf, &config)?;
                let remaining_values = values[1..].to_vec();

                futures.push((
                    line_number,
                    tokio::spawn(async move {
                        process_line(
                            &config,
                            &mut remaining_values.iter(),
                            writer_clone,
                            tokens_clone,
                        )
                        .await
                    }),
                ));

                line_number += 1;

                // Process results when we hit the parallel limit or on last line
                if futures.len() >= PARALLEL {
                    process_ordered_futures(&mut futures).await?;
                }
            }
            Err(e) => return Err(InvokeError::from(e)),
        }
    }

    // Process any remaining futures
    while !futures.is_empty() {
        process_ordered_futures(&mut futures).await?;
    }

    Ok(())
}

async fn process_ordered_futures(
    futures: &mut Vec<(u64, tokio::task::JoinHandle<Result<(), InvokeError>>)>,
) -> Result<(), InvokeError> {
    // Sort by line number to maintain order
    futures.sort_by_key(|(num, _)| *num);

    // Remove and process the first future
    let (_, future) = futures.remove(0);
    future.await??;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), InvokeError> {
    // Initialize tracing once at program start
    if let Err(e) = init_tracing(Some("info")) {
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

    let res = execute_udf(udf_str, reader, writer).await;
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
