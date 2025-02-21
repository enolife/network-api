use crate::config;
use crate::flops::measure_flops;
use crate::memory_stats::get_memory_info;
use crate::nexus_orchestrator::{
    GetProofTaskRequest, GetProofTaskResponse, NodeTelemetry, NodeType, SubmitProofRequest,
};
use prost::Message;
use reqwest::Client;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use futures::future::join_all;

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

    /// Sends multiple concurrent requests and stops once one is successful
    async fn make_concurrent_requests<T, U>(
        &self,
        url: &str,
        method: &str,
        request_data: &T,
        attempts: usize,
    ) -> Result<U, Box<dyn std::error::Error>>
    where
        T: Message + Send + Sync + 'static,
        U: Message + Default + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<Result<U, Box<dyn std::error::Error>>>(1);
        
        let tasks: Vec<_> = (0..attempts)
            .map(|_| {
                let tx = tx.clone();
                let request_data = request_data.clone();
                let client = self.client.clone();
                let url = format!("{}{}", self.base_url, url);
                let method = method.to_string();

                tokio::spawn(async move {
                    let request_bytes = request_data.encode_to_vec();

                    let response = match method.as_str() {
                        "POST" => client.post(&url).header("Content-Type", "application/octet-stream").body(request_bytes).send().await,
                        "GET" => client.get(&url).send().await,
                        _ => return,
                    };

                    if let Ok(resp) = response {
                        if resp.status().is_success() {
                            let response_bytes = resp.bytes().await.unwrap_or_default();
                            if let Ok(msg) = U::decode(response_bytes) {
                                let _ = tx.send(Ok(msg)).await;
                                return;
                            }
                        }
                    }

                    // If failed, send an error
                    let _ = tx.send(Err("[ERROR] Request failed.".into())).await;
                })
            })
            .collect();

        // Wait for any response
        if let Some(Ok(result)) = rx.recv().await {
            return result;
        }

        // Cancel all tasks after first success
        drop(tasks);

        Err("[ERROR] All attempts failed.".into())
    }

    /// Sends up to 20 requests concurrently and stops when one succeeds.
    pub async fn get_proof_task(
        &self,
        node_id: &str,
    ) -> Result<GetProofTaskResponse, Box<dyn std::error::Error>> {
        let request = GetProofTaskRequest {
            node_id: node_id.to_string(),
            node_type: NodeType::CliProver as i32,
        };

        self.make_concurrent_requests("/tasks", "POST", &request, 20).await
    }

    /// Sends up to 20 `submit_proof` requests concurrently and stops when one succeeds.
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
            proof,
            node_telemetry: Some(NodeTelemetry {
                flops_per_sec: Some(flops as i32),
                memory_used: Some(program_memory),
                memory_capacity: Some(total_memory),
                location: Some("US".to_string()),
            }),
        };

        self.make_concurrent_requests::<SubmitProofRequest, ()>("/tasks/submit", "POST", &request, 20).await?;

        Ok(())
    }
}
