#[cfg(test)]
mod tests {
    use casper_sdk_rs::api::node::sse::{
        client, error::SseError, types::EventType, Client, EventInfo,
    };
    use core::{panic, task};
    use mockito::ServerGuard;
    use std::{
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Mutex,
        },
        thread,
        time::Duration,
    };

    const EVENT_STREAM: &str = concat!(
        "data: {\"ApiVersion\": \"2.0.0\"}\n\n",
        "data: {\"BlockAdded\": {\"height\":10}}\n\n",
        "data: {\"BlockAdded\": {\"height\":11}}\n\n",
        "data: {\"BlockAdded\": {\"height\":12}}\n\n",
        "data: {\"TransactionProcessed\": \"test\"}\n\n",
        "data: {\"BlockAdded\": {\"height\":13}}\n\n",
        "data: {\"BlockAdded\": {\"height\":14}}\n\n",
    );

    async fn create_mock_server(event_stream: Option<&str>) -> ServerGuard {
        let event_stream = match event_stream {
            Some(e_stream) => e_stream,
            None => EVENT_STREAM,
        };
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/")
            .with_header("content-type", "text/event-stream")
            .with_body(event_stream)
            .create_async()
            .await;
        server
    }

    #[tokio::test]
    async fn test_client_connection() {
        let server = create_mock_server(None).await;
        let mut client = Client::new(&server.url());

        client
            .connect()
            .await
            .expect("Failed to connect to SSE endpoint");
    }

    #[tokio::test]
    async fn test_run_invokes_handlers() {
        let server = create_mock_server(None).await;
        let client = Arc::new(Client::new(&server.url()));
        client.connect().await.unwrap();

        let block_added_count = Arc::new(Mutex::new(0));
        let tx_processed_count = Arc::new(Mutex::new(0));

        let block_added_handler = {
            let block_added_count = Arc::clone(&block_added_count);
            move |event_info: EventInfo| {
                let mut count = block_added_count.lock().unwrap();
                *count += 1;
                println!("Block added handler, event: {:?}", event_info.event_type());
            }
        };

        let tx_processed_handler = {
            let tx_processed_count = Arc::clone(&tx_processed_count);
            move |event_info: EventInfo| {
                let mut count = tx_processed_count.lock().unwrap();
                *count += 1;
                println!(
                    "Transaction processed handlerevent: {:?}",
                    event_info.event_type()
                );
            }
        };

        client.on_event(EventType::BlockAdded, block_added_handler);
        client.on_event(EventType::TransactionProcessed, tx_processed_handler);

        let client_clone = Arc::clone(&client);
        let client_handle = tokio::spawn(async move {
            client_clone.run().await;
        });

        let test_handler = |event_info: EventInfo| {
            println!(
                "Test handler called with event: {:?}",
                event_info.event_type()
            );
        };

        client.shutdown();

        assert_eq!(*block_added_count.lock().unwrap(), 5);
        assert_eq!(*tx_processed_count.lock().unwrap(), 1);

        client.on_event(EventType::BlockAdded, test_handler);

        assert_eq!(*block_added_count.lock().unwrap(), 5);
        assert_eq!(*tx_processed_count.lock().unwrap(), 1);
    }

    // #[tokio::test]
    // async fn test_wait_for_event_success() {
    //     let server = create_mock_server(None).await;
    //     // Share the client using Arc<Mutex<_>>
    //     let client = Arc::new(Mutex::new(Client::new(&server.url())));
    //     client.lock().unwrap().connect().await.unwrap();

    //     let client_clone = Arc::clone(&client); // Clone the Arc
    //     let client_task = tokio::spawn(async move {
    //         client_clone.lock().unwrap().run().await.unwrap();
    //     });

    //     let predicate = |data: &EventInfo| {
    //         if let EventInfo::BlockAdded(block_data) = data {
    //             if let Some(height) = block_data["height"].as_u64() {
    //                 return height == 13;
    //             }
    //         }
    //         false
    //     };

    //     let timeout = Duration::from_secs(15);
    //     let result = client
    //         .lock()
    //         .unwrap()
    //         .wait_for_event(EventType::BlockAdded, predicate, timeout)
    //         .await; // Lock before calling

    //     match result {
    //         Ok(Some(EventInfo::BlockAdded(block_data))) => {
    //             assert_eq!(block_data["height"].as_u64().unwrap(), 13);
    //         }
    //         Ok(Some(other_event)) => panic!("Expected BlockAdded, got {:?}", other_event),
    //         Ok(None) => panic!("wait_for_event timed out"),
    //         Err(err) => panic!("Unexpected error: {}", err),
    //     }
    // }
}
