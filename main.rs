use hyper::{Body, Client, Request, Uri};
use hyper_tls::HttpsConnector;
use tokio::fs::File;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, OwnedSemaphorePermit};
use serde_json::Value;
use log::{info, error};
use structopt::StructOpt;
use std::collections::HashMap;
use std::io::Write;
use tokio::time::{Instant, Duration, sleep};
use std::sync::{Arc, Mutex};
use chrono::Local;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;
use rand::Rng;

/// Command-line arguments structure
#[derive(StructOpt)]
struct Cli {
    requests_filepath: String,
    max_requests_per_second: usize,
    max_attempts: usize,
    save_filepath: Option<String>,
}

/// Struct to track the status of requests
#[derive(Debug, Default, Clone)]
pub struct StatusTracker {
    pub num_tasks_started: usize,
    pub num_tasks_in_progress: usize,
    pub num_tasks_succeeded: usize,
    pub num_tasks_failed: usize,
    pub num_rate_limit_errors: usize,
    pub num_api_errors: usize,
    pub num_other_errors: usize,
}

/// Struct representing an API request
#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct APIRequest {
    pub task_id: usize,
    pub request_json: HashMap<String, Value>,
    pub attempts_left: usize,
    pub metadata: Option<HashMap<String, Value>>,
    pub result: Vec<Value>,
    pub original_input: HashMap<String, Value>,
}

/// Append data to a JSONL file
pub fn append_to_jsonl(data: Value, filename: &str) -> std::io::Result<()> {
    let json_string = data.to_string();
    let mut file = std::fs::OpenOptions::new().append(true).create(true).open(filename)?;
    writeln!(file, "{}", json_string)?;
    Ok(())
}

/// Generator for task IDs
pub fn task_id_generator() -> impl Iterator<Item = usize> {
    (0..).into_iter()
}

/// Struct representing an API endpoint
struct Endpoint {
    url: String,
    api_key: String,
    weight: usize,
}

/// Select an endpoint based on weight
fn select_endpoint(endpoints: &[Endpoint]) -> &Endpoint {
    let total_weight: usize = endpoints.iter().map(|e| e.weight).sum();
    let mut rand = rand::thread_rng();
    let mut rand_val = rand.gen_range(0..total_weight);
    for endpoint in endpoints {
        if rand_val < endpoint.weight {
            return endpoint;
        }
        rand_val -= endpoint.weight;
    }
    &endpoints[0] // Fallback
}

/// Process API requests from a file
async fn process_api_requests_from_file(
    requests_filepath: String,
    save_filepath: String,
    send_requests_per_second: usize,
    max_attempts: usize,
) -> io::Result<Arc<Mutex<StatusTracker>>> {
    // Initialize trackers
    let status_tracker = Arc::new(Mutex::new(StatusTracker::default()));
    let mut task_id_gen = task_id_generator();

    // Read the requests file
    let file = File::open(requests_filepath).await?;
    let reader = BufReader::new(file);
    let lines = reader.lines();

    // Initialize the HTTPS client
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    // Channel for queueing requests
    let (tx, mut rx) = mpsc::channel::<APIRequest>(send_requests_per_second * 2); // Buffer for at least 2 seconds worth of requests

    // Producer task to enqueue requests at a steady rate
    let tx_clone = tx.clone();
    let status_tracker_clone = Arc::clone(&status_tracker);

    tokio::spawn(async move {
        let mut lines_stream = LinesStream::new(lines);
        pin_utils::pin_mut!(lines_stream);
        while let Some(line) = lines_stream.next().await {
            match line {
                Ok(line) => {
                    match serde_json::from_str::<Value>(&line) {
                        Ok(request_json) => {
                            let original_input = request_json.clone();

                            let next_request = APIRequest {
                                task_id: task_id_gen.next().unwrap(),
                                request_json: request_json.as_object().unwrap().clone().into_iter().collect(),
                                attempts_left: max_attempts,
                                metadata: None,
                                result: vec![],
                                original_input: original_input.as_object().unwrap().clone().into_iter().collect(),
                            };

                            // Lock and unlock the tracker in a limited scope
                            {
                                let mut tracker = status_tracker_clone.lock().unwrap();
                                tracker.num_tasks_started += 1;
                            }

                            if let Err(e) = tx_clone.send(next_request).await {
                                error!("Failed to enqueue request: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse JSON from line: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read line from file: {}", e);
                }
            }
            sleep(Duration::from_millis(1000 / send_requests_per_second as u64)).await;
        }
    });


    // Consumer tasks to process requests
    let error_filepath = "/home/azureuser/my_project/error.jsonl".to_string();
    while let Some(next_request) = rx.recv().await {
        let client_clone = client.clone();
        let tx_clone = tx.clone();
        let save_filepath_clone = save_filepath.clone();
        let status_tracker_clone = Arc::clone(&status_tracker);
        let error_filepath_clone = error_filepath.clone(); // Clone here

        tokio::spawn(async move {
            send_request(
                client_clone,
                next_request,
                tx_clone,
                save_filepath_clone,
                status_tracker_clone,
                error_filepath_clone, // Use clone here
                max_attempts,
            ).await;
        });
    }

    Ok(status_tracker)
}

/// Send an API request and handle the response
async fn send_request(
    client: Client<HttpsConnector<hyper::client::HttpConnector>>,
    mut request: APIRequest,
    tx: mpsc::Sender<APIRequest>,
    save_filepath: String,
    status_tracker: Arc<Mutex<StatusTracker>>,
    error_filepath: String,
    max_attempts: usize,
) {
    let endpoints = vec![
        Endpoint {
            url: "https://api.example.com/endpoint".to_string(),
            api_key: "your_api_key_here".to_string(),
            weight: 20,
        }
    ];

    let endpoint = select_endpoint(&endpoints);
    let request_url: Uri = endpoint.url.parse().unwrap();
    let api_key = endpoint.api_key.clone();

    let payload = serde_json::json!({
        "messages": [
            {
              "role": "system",
              "content": "Your system message here"
            },
            {
              "role": "user",
              "content": request.request_json.get("input").unwrap().as_str().unwrap()
            }
        ],
        "temperature": 0.4,
        "max_tokens": 120
    });

    let req = Request::post(request_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .body(Body::from(payload.to_string()))
        .unwrap();

    let start = Instant::now();
    let task_id = request.task_id;
    let input = request.request_json.get("input").unwrap().as_str().unwrap().to_string();

    info!("Sent: {} - {} - {}", task_id, input, Local::now().format("%Y-%m-%d %H:%M:%S"));

    match client.request(req).await {
        Ok(response) => {
            let body = hyper::body::to_bytes(response.into_body()).await;
            let duration = start.elapsed();
            match body {
                Ok(body_bytes) => {
                    let result: Result<Value, _> = serde_json::from_slice(&body_bytes);
                    match result {
                        Ok(result_json) => {
                            if result_json.get("errors").is_some() && !result_json.get("errors").unwrap().as_array().unwrap().is_empty() {
                                // Write the failed request to the error file
                                let error_data = serde_json::json!({
                                    "input": request.request_json.get("input").unwrap(),
                                    "error": result_json.get("errors").unwrap(),
                                });
                                tokio::spawn(async move {
                                    append_to_jsonl(error_data, &error_filepath).unwrap();
                                });
                                let mut tracker = status_tracker.lock().unwrap();
                                tracker.num_tasks_failed += 1;
                            } else {
                                // Save the result
                                tokio::spawn(async move {
                                    append_to_jsonl(result_json, &save_filepath).unwrap();
                                });
                                let mut tracker = status_tracker.lock().unwrap();
                                tracker.num_tasks_succeeded += 1;
                            }
                        }
                        Err(e) => {
                            error!("Request {} failed to parse JSON: {}", task_id, e);
                            // Log the raw response body for debugging
                            error!("Raw response body: {:?}", String::from_utf8_lossy(&body_bytes));
                            // Write the failed request to the error file
                            let error_data = serde_json::json!({
                                "input": request.request_json.get("input").unwrap(),
                                "error": e.to_string(),
                            });
                            tokio::spawn(async move {
                                append_to_jsonl(error_data, &error_filepath).unwrap();
                            });
                            let mut tracker = status_tracker.lock().unwrap();
                            tracker.num_tasks_failed += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("Request {} failed to read response body: {}", task_id, e);
                    // Write the failed request to the error file
                    let error_data = serde_json::json!({
                        "input": request.request_json.get("input").unwrap(),
                        "error": e.to_string(),
                    });
                    tokio::spawn(async move {
                        append_to_jsonl(error_data, &error_filepath).unwrap();
                    });
                    let mut tracker = status_tracker.lock().unwrap();
                    tracker.num_tasks_failed += 1;
                }
            }
            info!("Response: {} - {:.1} sec - {} - {}", task_id, duration.as_secs_f64(), input, Local::now().format("%Y-%m-%d %H:%M:%S"));
        }
        Err(e) => {
            error!("Request {} failed: {}", request.task_id, e);
            request.attempts_left -= 1;
            if request.attempts_left > 0 {
                // Add exponential backoff
                let backoff_duration = 2u64.pow((max_attempts - request.attempts_left) as u32);
                sleep(Duration::from_secs(backoff_duration)).await;
                let retry_request = request.clone();
                tx.send(retry_request).await.unwrap();
            } else {
                // Write the failed request to the error file
                let error_data = serde_json::json!({
                    "input": request.request_json.get("input").unwrap(),
                    "error": e.to_string(),
                });
                tokio::spawn(async move {
                    append_to_jsonl(error_data, &error_filepath).unwrap();
                });
                let mut tracker = status_tracker.lock().unwrap();
                tracker.num_tasks_failed += 1;
            }
        }
    }

    let mut tracker = status_tracker.lock().unwrap();
    tracker.num_tasks_in_progress -= 1;
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let args = Cli::from_args();
    let save_filepath = args.save_filepath.clone().unwrap_or_else(|| args.requests_filepath.replace(".jsonl", "_results.jsonl"));

    let status_tracker = process_api_requests_from_file(
        args.requests_filepath,
        save_filepath,
        args.max_requests_per_second,
        args.max_attempts,
    ).await.unwrap();

    let tracker = status_tracker.lock().unwrap();
    info!("Processing completed.");
    info!("Total tasks started: {}", tracker.num_tasks_started);
    info!("Total tasks succeeded: {}", tracker.num_tasks_succeeded);
    info!("Total tasks failed: {}", tracker.num_tasks_failed);
    info!("Total rate limit errors: {}", tracker.num_rate_limit_errors);
    info!("Total API errors: {}", tracker.num_api_errors);
    info!("Total other errors: {}", tracker.num_other_errors);
}
