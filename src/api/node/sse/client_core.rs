use std::collections::HashMap;

use eventsource_stream::{Event, EventStreamError, Eventsource};
use futures::{stream::BoxStream, TryStreamExt};

use super::{client::Callback, error::SseError, types::EventType, EventInfo};

type BoxedEventStream = BoxStream<'static, Result<Event, EventStreamError<reqwest::Error>>>;

pub struct ClientCore {
    url: String,
    event_stream: Option<BoxedEventStream>,
    event_handlers: HashMap<EventType, HashMap<u64, Box<dyn Fn() + Send>>>,
    id_types: HashMap<u64, EventType>,
    next_event_id: u64,
    pub is_connected: bool,
}


impl ClientCore {
    pub async fn new(url: &str) -> Self {
        ClientCore {
            url: url.to_string(),
            event_stream: None,
            event_handlers: HashMap::new(),
            id_types: HashMap::new(),
            next_event_id: 0,
            is_connected: false,
        }
    }

    pub async fn connect(&mut self) -> Result<(), String> {
        // Connect to SSE endpoint.
        let client = reqwest::Client::new();
        let response = client.get(&self.url).send().await.unwrap();

        let stream = response.bytes_stream();
        let mut event_stream = stream.eventsource();

        // Handle the handshake with API version.
        let handshake_event = event_stream
            .try_next()
            .await.unwrap()
            .ok_or(SseError::StreamExhausted).unwrap();
        let handshake_data: EventInfo = serde_json::from_str(&handshake_event.data).unwrap();
        let _api_version = match handshake_data {
            EventInfo::ApiVersion(v) => Ok(v),
            _ => Err(SseError::InvalidHandshake),
        }.unwrap();

        // Wrap stream with box and store it.
        let boxed_event_stream = Box::pin(event_stream);

        self.event_stream = Some(boxed_event_stream);
        self.is_connected = true;
        Ok(())
    }

    pub fn remove_handler(&mut self, id: u64) -> bool {
        let typex = self.id_types.get(&id);
        if let Some(event_type) = typex {
            match self.event_handlers.get_mut(&event_type) {
                Some(handlers_for_type) => handlers_for_type.remove(&id).is_some(),
                None => false,
            };
        }

        true // TODO
    }

    pub async fn run_once(&mut self) -> Option<Event> {
        println!("Runnig!");
        match self.event_stream.take() {
            Some(mut stream) => {
                let y = stream.try_next().await.unwrap();
                self.event_stream = Some(stream);
                y
            },
            None => {
                println!("No stream (not connected)...");
                None
            },
        }
    }

    pub fn handle_event(&mut self, event: &Event) -> Result<(), SseError> {
            let data: EventInfo = serde_json::from_str(&event.data).unwrap();

            match data {
                EventInfo::ApiVersion(_) => return Err(SseError::UnexpectedHandshake), // Should only happen once at connection
                EventInfo::Shutdown => return Err(SseError::NodeShutdown),

                // For each type, find and invoke registered handlers
                event => {
                    if let Some(handlers) = self.event_handlers.get_mut(&event.event_type()) {
                        println!("Foind!");
                        for handler in handlers.values() {
                            handler(); // Invoke each handler for the event
                        }
                    }
                }
            }
            Ok(())
    }
    
    pub fn add_on_event_handler(&mut self, event_type: EventType, callback: Box<Callback>) {
        println!("adding on event...");
    //     pub fn on_event<F>(&mut self, event_type: EventType, handler: F) -> u64
    // where
    //     F: Fn() + 'static + Send + Sync,
    
        // let boxed_handler = Box::new(handler);
        let handlers = self.event_handlers.entry(event_type).or_default();
        let event_id = self.next_event_id;
        handlers.insert(event_id, callback);
        self.next_event_id += 1;
        //event_id
    }
}
