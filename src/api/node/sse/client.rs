use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
    sync::{atomic::AtomicU64, Arc, Mutex},
};

use eventsource_stream::Eventsource;
use futures::stream::TryStreamExt;

use super::{error::SseError, types::EventType, EventInfo};

//TODO: take it from .env file
const DEFAULT_SSE_SERVER: &str = "http://localhost:18101";
const DEFAULT_EVENT_CHANNEL: &str = "/events";

// type BoxedEventStream = BoxStream<'static, Result<Event, EventStreamError<reqwest::Error>>>;

pub struct Client {
    pub url: String,
    // pub event_stream: Option<BoxedEventStream>,
    pub next_event_id: AtomicU64,
    pub event_handlers: Arc<
        Mutex<HashMap<EventType, HashMap<u64, Box<dyn Fn(EventInfo) + Send + Sync + 'static>>>>,
    >,
    sender: Arc<Mutex<Sender<EventInfo>>>,
    receiver: Arc<Mutex<Receiver<EventInfo>>>,
    shutdown: Arc<Mutex<bool>>,
}

impl Default for Client {
    fn default() -> Self {
        let url = format!("{}{}", DEFAULT_SSE_SERVER, DEFAULT_EVENT_CHANNEL);
        let (sender, receiver) = channel::<EventInfo>();
        Self {
            url,
            // event_stream: None,
            next_event_id: AtomicU64::new(0),
            event_handlers: Arc::new(Mutex::new(HashMap::new())),
            sender: Arc::new(Mutex::new(sender)),
            receiver: Arc::new(Mutex::new(receiver)),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }
}

impl Client {
    pub fn new(url: &str) -> Self {
        let (sender, receiver) = channel::<EventInfo>();
        Client {
            url: url.to_string(),
            // event_stream: None,
            next_event_id: AtomicU64::new(0),
            event_handlers: Arc::new(Mutex::new(HashMap::new())),
            sender: Arc::new(Mutex::new(sender)),
            receiver: Arc::new(Mutex::new(receiver)),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn connect(&self) -> Result<(), SseError> {
        // Connect to SSE endpoint.
        let client = reqwest::Client::new();
        let response = client.get(&self.url).send().await?;

        let stream = response.bytes_stream();
        let mut event_stream = stream.eventsource();

        // Handle the handshake with API version.
        let handshake_event = event_stream
            .try_next()
            .await?
            .ok_or(SseError::StreamExhausted)?;
        let handshake_data: EventInfo = serde_json::from_str(&handshake_event.data)?;
        let _api_version = match handshake_data {
            EventInfo::ApiVersion(v) => Ok(v),
            _ => Err(SseError::InvalidHandshake),
        }?;

        // Wrap stream with box and store it.
        let mut boxed_event_stream = Box::pin(event_stream);

        let sender_clone = self.sender.clone();
        // self.event_stream = Some(boxed_event_stream);

        tokio::spawn(async move {
            while let Ok(Some(event)) = boxed_event_stream.try_next().await {
                let data = serde_json::from_str(&event.data).unwrap(); //TODO: add proper error
                sender_clone.lock().unwrap().send(data).unwrap();
            }
        });

        Ok(())
    }

    pub fn on_event<F>(&self, event_type: EventType, handler: F) -> u64
    where
        F: Fn(EventInfo) + 'static + Send + Sync,
    {
        let boxed_handler = Box::new(handler);
        let mut handlers_map = self.event_handlers.lock().unwrap(); //TODO: add error handling
        let handlers = handlers_map.entry(event_type).or_default();
        let event_id = self
            .next_event_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        handlers.insert(event_id, boxed_handler);
        event_id
    }

    pub fn remove_handler(&self, event_type: EventType, id: u64) -> bool {
        match self.event_handlers.lock().unwrap().get_mut(&event_type) {
            Some(handlers_for_type) => handlers_for_type.remove(&id).is_some(),
            None => false,
        }
    }

    pub async fn run(&self) {
        while let Ok(event) = self.receiver.lock().unwrap().recv() {
            // let data: EventInfo = serde_json::from_str(&event.data).unwrap(); // TODO: handle error
            if *self.shutdown.lock().unwrap() {
                break;
            }

            match event {
                EventInfo::ApiVersion(_) => return,
                EventInfo::Shutdown => return,
                _ => {
                    if let Some(handlers) = self
                        .event_handlers
                        .lock()
                        .unwrap()
                        .get_mut(&event.event_type())
                    {
                        for handler in handlers.values() {
                            handler(event.clone());
                        }
                    }
                }
            }
        }
    }

    pub fn shutdown(&self) {
        *self.shutdown.lock().unwrap() = true;
    }
}

// //TODO: do we need to look for any registered handlers in this function? Not sure what is the relation between this and run function.
// pub async fn wait_for_event<F>(
//     &mut self,
//     event_type: EventType,
//     predicate: F,
//     timeout: Duration,
// ) -> Result<Option<EventInfo>, SseError>
// where
//     F: Fn(&EventInfo) -> bool + Send + Sync + 'static,
// {
//     let (tx, rx) = oneshot::channel::<EventInfo>();
//     let tx = Arc::new(Mutex::new(Some(tx)));

//     let handler_id = self.on_event(event_type, {
//         // Clone the Arc to share ownership
//         let tx = Arc::clone(&tx);

//         move |event: EventInfo| {
//             if predicate(&event) {
//                 // Acquire the lock, take the transmitter and try to send
//                 if let Some(tx) = tx.lock().unwrap().take() {
//                     // Lock mutex
//                     let _ = tx.send(event); // Ignore send error if receiver is gone
//                 }
//             }
//         }
//     });

//     let result = timeout_f(timeout, rx).await;

//     self.remove_handler(event_type, handler_id);

//     match result {
//         Ok(Ok(event)) => Ok(Some(event)),
//         Ok(Err(_)) => Err(SseError::NotConnected),
//         Err(_) => Ok(None), // Timeout
//     }
// }
