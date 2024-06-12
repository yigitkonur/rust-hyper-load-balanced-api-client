# 10K req/sec LLM Requester w/ Load Balancer

To develop new features for Wope's AI expension we required extensive need of processing few thousand request per second for long-running tasks like LLM requests. We needed to handle up to 10,000 requests per second efficiently. As part of Cloudflare's startup program, we received significant support, including access to a GPU cluster. However, consuming this capacity on an average machine (8 cores, 16 GB RAM) presented significant challenges. Python's Global Interpreter Lock (GIL) limits prompted us to seek a compiled language solution. While our backend primarily uses Go, I decided to explore Rust's capabilities for this high-performance task.

## Introduction

`rust-hyper-load-balanced-api-client` is a high-performance Rust tool designed to send API requests (specifically to LLMs) with built-in weighted load balancing, retry mechanisms, and rate limiting. Utilizing the `hyper` library for fast request handling, this tool manages large volumes of asynchronous requests, optimized to handle up to 10,000 requests per second.

## Features

- **Weighted Load Balancing**: Distribute API requests across multiple LLM endpoints based on their weights.
- **Retry Mechanisms**: Automatically retry failed requests with exponential backoff.
- **Rate Limiting**: Control the number of requests sent per second.
- **Asynchronous I/O**: Read from and write to files asynchronously, ensuring high throughput.
- **Error Logging**: Save error responses to a dedicated `errors.jsonl` file.
- **Configurable via CLI**: Easily configure the tool using command-line flags.

## Usage

### Prerequisites

- Rust toolchain installed
- Cargo package manager

### Installation

Clone the repository and build the project:

```sh
git clone https://github.com/yourusername/rust-hyper-load-balanced-api-client.git
cd rust-hyper-load-balanced-api-client
cargo build --release
```

### CLI Configuration

The tool can be configured using the following command-line flags:

- `--requests_filepath`: Path to the JSONL file containing the requests.
- `--max_requests_per_second`: Maximum number of requests to send per second.
- `--max_attempts`: Maximum number of retry attempts for failed requests.
- `--save_filepath`: Path to save the successful responses (optional).

Example usage:

```sh
./target/release/api_processor --requests_filepath "/path/to/requests.jsonl" --max_requests_per_second 10000 --max_attempts 3 --save_filepath "/path/to/save.jsonl"
```

### JSON Schema

The input JSONL file should contain one JSON object per line, structured as follows:

```json
{
  "input": "User prompt or input text here"
}
```

### Error Logging

Errors are logged in a separate `errors.jsonl` file, with each error entry structured as follows:

```json
{
  "input": "User prompt or input text here",
  "error": "Error message"
}
```

## Example

### Input File: `requests.jsonl`

```json
{"input": "User prompt or input text here"}
```

### Output File: `save.jsonl`

```json
{"response": "LLM response"}
```

### Error File: `errors.jsonl`

```json
{"input": "User prompt or input text here", "error": "Error message"}
```

## Code Explanation

The project consists of a single main Rust file (`main.rs`) that handles all the functionalities:

1. **Reading Requests**: The tool reads from a JSONL file asynchronously, ensuring it doesn't block the processing of other requests.
2. **Sending Requests**: It uses `hyper` to send HTTP requests to the specified LLM endpoints. Requests are sent at a rate controlled by the `max_requests_per_second` parameter.
3. **Load Balancing**: The tool uses weighted load balancing to distribute requests across multiple endpoints.
4. **Retry Mechanisms**: Failed requests are retried with exponential backoff until the maximum number of attempts (`max_attempts`) is reached.
5. **Logging**: Successful responses are saved to a specified file, while errors are logged to `errors.jsonl`.

### Main Functions

- `process_api_requests_from_file`: Manages reading requests from the file and sending them asynchronously.
- `send_request`: Sends individual API requests and handles retries and logging.

### Logging / Demo

https://github.com/yigitkonur/rust-hyper-load-balanced-api-client/assets/9989650/fabceab5-b802-45e4-9e20-8feaea6fba6d
Processing only 600 / sec to not make video size too large - but you can consume up to 10K req / sec.

Click the image above to watch a video demonstrating the speed and efficiency of the tool.

## Conclusion

`rust-hyper-load-balanced-api-client` is a robust and high-performance tool designed to handle the demanding task of sending a large volume of API requests efficiently. By leveraging Rust's capabilities and the `hyper` library, it achieves high throughput and reliability, making it an excellent choice for applications requiring extensive API interactions, such as consuming LLM services.

I welcome contributions and feedback from the community to further enhance this tool.
