use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use eventsource_stream::{Event, EventStreamError, Eventsource};
use futures::stream::{BoxStream, TryStreamExt};
use tokio::sync::{mpsc, oneshot};

use super::{client_core::ClientCore, error::SseError, types::EventType, EventInfo};

//TODO: take it from .env file
const DEFAULT_SSE_SERVER: &str = "http://localhost:18101";
const DEFAULT_EVENT_CHANNEL: &str = "/events";

type BoxedEventStream = BoxStream<'static, Result<Event, EventStreamError<reqwest::Error>>>;

pub type Callback = dyn Fn() + 'static + Send + Sync;

pub enum CoreCommand {
    Connect(oneshot::Sender<()>),
    AddOnEventHandler(EventType, Box<Callback>, oneshot::Sender<()>),
    RemoveEventHandler(u64, oneshot::Sender<bool>)
    // NOTE: More commands can be here.
}

pub struct Client {
    command_sender: mpsc::Sender<CoreCommand>,
}

// impl Default for Client {
//     fn default() -> Self {
//         let url = format!("{}{}", DEFAULT_SSE_SERVER, DEFAULT_EVENT_CHANNEL);
//         Self {
//             url,
//         }
//     }
// }

impl Client {
    pub async fn new(url: &str) -> Self {
        let client_core = ClientCore::new(url).await;

        let (tx, rx) = mpsc::channel(32);
        let _handle = tokio::spawn(async move {
            run_client_core(rx, client_core).await;
        });

        Client {
            command_sender: tx,
        }
    }

    pub async fn connect(&self) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.command_sender
            .send(CoreCommand::Connect(tx))
            .await
            .unwrap();
            // .map_err(|e| L1SyncError::BrokenChannel(format!("Unable to send trigger: {}", e)))?;
        rx.await.unwrap();
        // .map_err(|e| {
        //     L1SyncError::BrokenChannel(format!("Unable to receive trigger ack: {}", e))
        // })?;

        Ok(())
    }

    pub async fn on_event<F>(&mut self, event_type: EventType, handler: F) -> u64
    where
        F: Fn() + 'static + Send + Sync,
    {
        let (tx, rx) = oneshot::channel();
        self.command_sender
            .send(CoreCommand::AddOnEventHandler(event_type, Box::new(handler), tx))
            .await
            .unwrap();
            // .map_err(|e| L1SyncError::BrokenChannel(format!("Unable to send trigger: {}", e)))?;
        let result = rx.await.unwrap();   
        // result
    
        0
    }

    pub async fn remove_handler(&mut self, id: u64) -> bool {
        let (tx, rx) = oneshot::channel();
        self.command_sender
            .send(CoreCommand::RemoveEventHandler(id, tx))
            .await
            .unwrap();
            // .map_err(|e| L1SyncError::BrokenChannel(format!("Unable to send trigger: {}", e)))?;
        let result = rx.await.unwrap();   
        result
    }

    // //TODO: do we need to look for any registered handlers in this function? Not sure what is the relation between this and run function.
    // pub async fn wait_for_event<F>(
    //     &mut self,
    //     event_type: EventType,
    //     predicate: F,
    //     timeout: Duration,
    // ) -> Result<Option<EventInfo>, SseError>
    // where
    //     F: Fn(&EventInfo) -> bool + Send + Sync,
    // {
    //     let start_time = Instant::now();
    //     loop {
    //         if Instant::now().duration_since(start_time) > timeout {
    //             return Err(SseError::Timeout);
    //         }

    //         // Await for next event
    //         if let Some(event) = self
    //             .event_stream
    //             .as_mut()
    //             .ok_or(SseError::NotConnected)?
    //             .try_next()
    //             .await?
    //         {
    //             let data: EventInfo = serde_json::from_str(&event.data)?;

    //             if data.event_type() == event_type && predicate(&data) {
    //                 return Ok(Some(data)); //matching event
    //             }
    //         } else {
    //             return Err(SseError::StreamExhausted);
    //         }
    //     }
    // }

    // pub async fn run(&mut self) -> Result<(), SseError> {
    //     // // Ensure the client is connected
    //     // let mut event_stream = self.event_stream.take().ok_or(SseError::NotConnected)?;

    //     // while let Some(event) = event_stream.try_next().await? {
        
    //     // }
    //     // // Stream was exhausted.
    //     // Err(SseError::StreamExhausted)
    //     Ok(())
    // }
}

/// Handles incoming commands and delegates tasks to ClientCore.
async fn run_client_core(mut rx: mpsc::Receiver<CoreCommand>, mut client_core: ClientCore) {
    loop {
        tokio::select! {
            Some(event) = client_core.run_once() => {
                println!("event: {:?}", event);
                client_core.handle_event(&event).unwrap();
            },
            Some(command) = rx.recv() => {
                    let _ = handle_command(command, &mut client_core)
                        .await
                        .unwrap();
                        // .map_err(|e| match e {
                        //     L1SyncError::UnexpectedError(e) => panic!("Unrecoverable error: {}", e),
                        //     _ => tracing::error!("Transient error: {}", e),
                        // });
            },
        }
    }
}

async fn handle_command(
    command: CoreCommand,
    client_core: &mut ClientCore,
) -> Result<(), String> {
    match command {
        CoreCommand::AddOnEventHandler(event_type, callback, completion_ack) => {
            client_core.add_on_event_handler(event_type, callback);
            completion_ack
                .send(())
                .unwrap();
                // .map_err(|_| L1SyncError::BrokenChannel("Sender dropped".to_string()))?;
        }

        CoreCommand::Connect(completion_ack) => {
            println!("Connecting!");
            client_core.connect().await.unwrap();
            completion_ack
                .send(())
                .unwrap();
                // .map_err(|_| L1SyncError::BrokenChannel("Sender dropped".to_string()))?;
        }

        CoreCommand::RemoveEventHandler(id, completion_ack) => {
            client_core.remove_handler(id);
            completion_ack
                .send(true) // TODO
                .unwrap();
                // .map_err(|_| L1SyncError::BrokenChannel("Sender dropped".to_string()))?;
        }
    }

    Ok(())
}