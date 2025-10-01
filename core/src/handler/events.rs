use crate::error::GatewayError;
use crate::events::broadcast_channel_manager::BroadcastChannelManager;
use crate::events::ui_broadcaster::EventsUIBroadcaster;
use crate::events::Event;
use crate::types::metadata::project::Project;
use actix_web::{web, HttpResponse, Responder, Result};
use futures::stream::StreamExt;
use tokio::sync::mpsc::{self};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

/// Stream events endpoint handler
pub async fn stream_events(
    project: web::ReqData<Project>,
    broadcaster: web::Data<EventsUIBroadcaster>,
    project_trace_senders: web::Data<BroadcastChannelManager>,
) -> Result<impl Responder> {
    let project_slug = project.slug.clone();

    // Create a unique channel ID for this connection
    let channel_id = format!("{}:{}", project_slug, Uuid::new_v4());

    // Create a channel for sending events
    let (tx, rx) = mpsc::channel::<Event>(10000);

    tracing::debug!("New client connected to events stream: {}", channel_id);

    // Get or create the broadcast channel for this project
    let sender = project_trace_senders
        .get_or_create_channel(&project_slug)
        .map_err(|e| {
            GatewayError::CustomError(format!("Failed to create broadcast channel: {e}"))
        })?;
    let broadcast_receiver = sender.subscribe();

    // Store the sender in our application state
    broadcaster
        .add_sender(&channel_id, tx.clone(), broadcast_receiver)
        .await;

    // Spawn a cleanup task that monitors when this client disconnects
    let project_slug_cleanup = project_slug.clone();
    let project_trace_senders_cleanup = project_trace_senders.clone();
    let tx_cleanup = tx.clone();
    tokio::spawn(async move {
        // Wait for the channel to be closed (client disconnected)
        tx_cleanup.closed().await;

        tracing::info!("Client disconnected from events stream: {}", channel_id);

        // Small delay to allow other disconnections to happen
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Try to clean up the broadcast channel if there are no more receivers
        project_trace_senders_cleanup.try_cleanup_channel(&project_slug_cleanup);
        if let Some(sender) = project_trace_senders_cleanup
            .inner()
            .get(&project_slug_cleanup)
        {
            tracing::info!("Sender receiver count: {}", sender.receiver_count());
        }
    });

    // Create a stream for the events
    let event_stream = ReceiverStream::new(rx).map(move |event| {
        let json = serde_json::to_string(&event).unwrap_or_default();

        // Convert the event to a Server-Sent Event format
        Ok::<_, GatewayError>(web::Bytes::from(format!("data: {json}\n\n")))
    });

    // Return the response
    Ok(HttpResponse::Ok()
        .append_header(("Content-Type", "text/event-stream"))
        .append_header(("Cache-Control", "no-cache"))
        .append_header(("Connection", "keep-alive"))
        .streaming(event_stream))
}
