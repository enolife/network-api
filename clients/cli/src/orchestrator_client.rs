use crate::config;
use crate::flops::measure_flops;
use crate::memory_stats::get_memory_info;
use crate::nexus_orchestrator::{
    GetProofTaskRequest, GetProofTaskResponse, NodeType, SubmitProofRequest, NodeTelemetry,
};
use prost::Message;
use reqwest::Client;
use serde::Serialize;
use std::fs::File;
use std::io::{self, Write};
use base64;

/// Struct for serializing `SubmitProofRequest` to JSON
#[derive(Serialize)]
struct SubmitProofRequestJson {
    node_id: String,
    node_type: i32,
    proof_hash: String,
    proof: String, // Base64-encoded proof data
    node_telemetry: Option<NodeTelemetryJson>,
}

/// Struct for serializing `NodeTelemetry`
#[derive(Serialize)]
struct NodeTelemetryJson {
    flops_per_sec: Option<i32>,
    memory_used: Option<i64>,
    memory_capacity: Option<i64>,
    location: Option<String>,
}

pub struct OrchestratorClient {
    client: Client,
    base_url: String,
}

impl OrchestratorClient {
    pub fn new(environment: config::Environment) -> Self {
        Self {
            client: Client::new(),
            base_url: environment.orchestrator_url(),
        }
    }

    async fn make_request<T, U>(
        &self,
        url: &str,
        method: &str,
        request_data: &T,
    ) -> Result<Option<U>, Box<dyn std::error::Error>>
    where
        T: Message,
        U: Message + Default,
    {
        let request_bytes = request_data.encode_to_vec();
        let url = format!("{}{}", self.base_url, url);

        let response = match method {
            "POST" => self.client.post(&url)
                .header("Content-Type", "application/octet-stream")
                .body(request_bytes)
                .send()
                .await,
            "GET" => self.client.get(&url).send().await,
            _ => return Err("[METHOD] Unsupported HTTP method".into()),
        };

        let friendly_messages = match response {
            Ok(resp) => resp,
            Err(_) => return Err("[CONNECTION] Unable to reach server.".into()),
        };

        if !friendly_messages.status().is_success() {
            let status = friendly_messages.status();
            let error_text = friendly_messages.text().await?;

            let clean_error = if error_text.contains("<html>") {
                format!("HTTP {}", status.as_u16())
            } else {
                error_text
            };

            return Err(format!("[{}] Unexpected error: {}", status, clean_error).into());
        }

        let response_bytes = friendly_messages.bytes().await?;
        if response_bytes.is_empty() {
            return Ok(None);
        }

        match U::decode(response_bytes) {
            Ok(msg) => Ok(Some(msg)),
            Err(_) => Ok(None),
        }
    }

    pub async fn get_proof_task(
        &self,
        node_id: &str,
    ) -> Result<GetProofTaskResponse, Box<dyn std::error::Error>> {
        let request = GetProofTaskRequest {
            node_id: node_id.to_string(),
            node_type: NodeType::CliProver as i32,
        };

        let response = self
            .make_request("/tasks", "POST", &request)
            .await?
            .ok_or("No response received from get_proof_task")?;

        Ok(response)
    }

    pub async fn submit_proof(
        &self,
        node_id: &str,
        proof_hash: &str,
        proof: Vec<u8>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (program_memory, total_memory) = get_memory_info();
        let flops = measure_flops();

        let request = SubmitProofRequest {
            node_id: node_id.to_string(),
            node_type: NodeType::CliProver as i32,
            proof_hash: proof_hash.to_string(),
            proof: proof.clone(),
            node_telemetry: Some(NodeTelemetry {
                flops_per_sec: Some(flops as i32),
                memory_used: Some(program_memory),
                memory_capacity: Some(total_memory),
                location: Some("US".to_string()),
            }),
        };

        // Convert to JSON and save
        let json_request = convert_to_json(&request);
        if let Ok(json) = serde_json::to_string_pretty(&json_request) {
            let _ = save_to_file("submit_proof.json", &json);
        }

        // Save binary payload
        let _ = save_binary_to_file("submit_proof.bin", &proof);

        self.make_request::<SubmitProofRequest, ()>("/tasks/submit", "POST", &request)
            .await?;

        Ok(())
    }
}

/// Converts `SubmitProofRequest` to a JSON-friendly struct
fn convert_to_json(request: &SubmitProofRequest) -> SubmitProofRequestJson {
    SubmitProofRequestJson {
        node_id: request.node_id.clone(),
        node_type: request.node_type,
        proof_hash: request.proof_hash.clone(),
        proof: base64::encode(&request.proof), // Encode binary data as Base64
        node_telemetry: request.node_telemetry.as_ref().map(|t| NodeTelemetryJson {
            flops_per_sec: t.flops_per_sec,
            memory_used: t.memory_used,
            memory_capacity: t.memory_capacity,
            location: t.location.clone(),
        }),
    }
}

/// Saves a string (JSON) to a file
fn save_to_file(filename: &str, content: &str) -> io::Result<()> {
    let mut file = File::create(filename)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

/// Saves binary data to a file
fn save_binary_to_file(filename: &str, data: &[u8]) -> io::Result<()> {
    let mut file = File::create(filename)?;
    file.write_all(data)?;
    Ok(())
}
