use crate::config;
use crate::flops::measure_flops;
use crate::memory_stats::get_memory_info;
use crate::nexus_orchestrator::{
    GetProofTaskRequest, GetProofTaskResponse, NodeType, SubmitProofRequest,
};
use prost::Message;
use reqwest::Client;
use tokio::sync::mpsc;

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
    ) -> Result<U, Box<dyn std::error::Error>>
    where
        T: Message + Send + Sync + 'static,
        U: Message + Default + Send + 'static,
    {
        let request_bytes = request_data.encode_to_vec();
        let url = format!("{}{}", self.base_url, url);

        let response = match method {
            "POST" => self
                .client
                .post(&url)
                .header("Content-Type", "application/octet-stream")
                .body(request_bytes)
                .send()
                .await,
            "GET" => self.client.get(&url).send().await,
            _ => return Err("[METHOD] Unsupported HTTP method".into()),
        };

        if let Ok(resp) = response {
            if resp.status().is_success() {
                let response_bytes = resp.bytes().await.unwrap_or_default();
                if let Ok(msg) = U::decode(response_bytes) {
                    return Ok(msg);
                }
            }
        }

        Err("[ERROR] Request failed.".into())
    }

    async fn make_concurrent_requests<T, U>(
        &self,
        url: &str,
        method: &str,
        request_data: &T,
        attempts: usize,
    ) -> Result<U, Box<dyn std::error::Error>>
    where
        T: Message + Send + Sync + Clone + 'static,
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
                    let result: Result<U, Box<dyn std::error::Error>> =
                        OrchestratorClient { client, base_url: url.clone() }
                            .make_request(&url, &method, &request_data)
                            .await;

                    let _ = tx.send(result).await;
                })
            })
            .collect();

        if let Some(Ok(result)) = rx.recv().await {
            return Ok(result);  // ✅ Fix applied here
        }

        // Cancel all tasks after first success
        drop(tasks);

        Err("[ERROR] All attempts failed.".into())
    }

    pub async fn get_proof_task(
        &self,
        node_id: &str,
    ) -> Result<GetProofTaskResponse, Box<dyn std::error::Error>> {
        let request = GetProofTaskRequest {
            node_id: node_id.to_string(),
            node_type: NodeType::CliProver as i32,
        };

        self.make_concurrent_requests("/tasks", "POST", &request, 20)
            .await
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
            proof,
            node_telemetry: Some(crate::nexus_orchestrator::NodeTelemetry {
                flops_per_sec: Some(flops as i32),
                memory_used: Some(program_memory),
                memory_capacity: Some(total_memory),
                location: Some("US".to_string()),
            }),
        };

        self.make_concurrent_requests("/tasks/submit", "POST", &request, 20)
            .await?;

        Ok(())
    }
}
