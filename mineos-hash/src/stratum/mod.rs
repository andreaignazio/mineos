//! Stratum protocol implementation for mining pools

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use anyhow::Result;
use crate::{BlockHeader, Hash256};

/// Stratum client for pool mining
pub struct StratumClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    worker: String,
    password: String,
    job_id: Option<String>,
    pub extranonce1: Option<String>,
    pub extranonce2_size: usize,
    pub difficulty: f64,
}

#[derive(Debug, Deserialize)]
struct StratumResponse {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<Value>,
    method: Option<String>,
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct StratumRequest {
    id: u64,
    method: String,
    params: Vec<Value>,
}

impl StratumClient {
    /// Connect to a mining pool
    pub fn connect(pool_url: &str, worker: &str, password: &str) -> Result<Self> {
        println!("Connecting to pool: {}", pool_url);
        
        let stream = TcpStream::connect(pool_url)?;
        let reader = BufReader::new(stream.try_clone()?);
        
        let mut client = Self {
            stream,
            reader,
            worker: worker.to_string(),
            password: password.to_string(),
            job_id: None,
            extranonce1: None,
            extranonce2_size: 0,
            difficulty: 1.0,
        };
        
        // Subscribe to mining notifications
        client.subscribe()?;
        
        // Authorize worker
        client.authorize()?;
        
        Ok(client)
    }
    
    /// Subscribe to mining.notify
    fn subscribe(&mut self) -> Result<()> {
        let request = StratumRequest {
            id: 1,
            method: "mining.subscribe".to_string(),
            params: vec![json!("MineOS/1.0")],
        };
        
        self.send_request(&request)?;
        let response = self.read_response()?;
        
        if let Some(result) = response.result {
            if let Some(arr) = result.as_array() {
                if arr.len() >= 2 {
                    // Extract extranonce1 and extranonce2_size
                    if let Some(extranonce1) = arr[1].as_str() {
                        self.extranonce1 = Some(extranonce1.to_string());
                    }
                    if let Some(size) = arr[2].as_u64() {
                        self.extranonce2_size = size as usize;
                    }
                }
            }
        }
        
        println!("Subscribed to pool");
        Ok(())
    }
    
    /// Authorize worker
    fn authorize(&mut self) -> Result<()> {
        let request = StratumRequest {
            id: 2,
            method: "mining.authorize".to_string(),
            params: vec![json!(self.worker), json!(self.password)],
        };
        
        self.send_request(&request)?;
        let response = self.read_response()?;
        
        if response.result == Some(json!(true)) {
            println!("Worker authorized: {}", self.worker);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Authorization failed"))
        }
    }
    
    /// Get new mining job
    pub fn get_job(&mut self) -> Result<MiningJob> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        
        let response: StratumResponse = serde_json::from_str(&line)?;
        
        if let Some(method) = response.method {
            if method == "mining.notify" {
                if let Some(params) = response.params {
                    return self.parse_job(params);
                }
            } else if method == "mining.set_difficulty" {
                if let Some(params) = response.params {
                    if let Some(arr) = params.as_array() {
                        if let Some(diff) = arr[0].as_f64() {
                            self.difficulty = diff;
                            println!("Difficulty set to: {}", diff);
                        }
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("No job available"))
    }
    
    /// Parse mining job from params
    fn parse_job(&mut self, params: Value) -> Result<MiningJob> {
        let arr = params.as_array().ok_or(anyhow::anyhow!("Invalid job params"))?;
        
        if arr.len() < 9 {
            return Err(anyhow::anyhow!("Incomplete job params"));
        }
        
        let job_id = arr[0].as_str().ok_or(anyhow::anyhow!("Invalid job_id"))?;
        let prevhash = arr[1].as_str().ok_or(anyhow::anyhow!("Invalid prevhash"))?;
        let coinb1 = arr[2].as_str().ok_or(anyhow::anyhow!("Invalid coinb1"))?;
        let coinb2 = arr[3].as_str().ok_or(anyhow::anyhow!("Invalid coinb2"))?;
        let merkle_branch = arr[4].as_array().ok_or(anyhow::anyhow!("Invalid merkle"))?;
        let version = arr[5].as_str().ok_or(anyhow::anyhow!("Invalid version"))?;
        let nbits = arr[6].as_str().ok_or(anyhow::anyhow!("Invalid nbits"))?;
        let ntime = arr[7].as_str().ok_or(anyhow::anyhow!("Invalid ntime"))?;
        let clean_jobs = arr[8].as_bool().unwrap_or(false);
        
        self.job_id = Some(job_id.to_string());
        
        Ok(MiningJob {
            job_id: job_id.to_string(),
            prevhash: prevhash.to_string(),
            coinb1: coinb1.to_string(),
            coinb2: coinb2.to_string(),
            merkle_branch: merkle_branch.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect(),
            version: version.to_string(),
            nbits: nbits.to_string(),
            ntime: ntime.to_string(),
            clean_jobs,
            difficulty: self.difficulty,
        })
    }
    
    /// Submit a share
    pub fn submit_share(&mut self, nonce: u64, hash: &Hash256) -> Result<bool> {
        let job_id = self.job_id.as_ref().ok_or(anyhow::anyhow!("No active job"))?;
        
        let request = StratumRequest {
            id: 3,
            method: "mining.submit".to_string(),
            params: vec![
                json!(self.worker),
                json!(job_id),
                json!(format!("{:08x}", 0)), // extranonce2
                json!(format!("{:08x}", nonce)),
                json!(hash.to_hex()),
            ],
        };
        
        self.send_request(&request)?;
        let response = self.read_response()?;
        
        Ok(response.result == Some(json!(true)))
    }
    
    /// Send request to pool
    fn send_request(&mut self, request: &StratumRequest) -> Result<()> {
        let json = serde_json::to_string(request)?;
        writeln!(self.stream, "{}", json)?;
        self.stream.flush()?;
        Ok(())
    }
    
    /// Read response from pool
    fn read_response(&mut self) -> Result<StratumResponse> {
        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }
}

/// Mining job from pool
#[derive(Debug, Clone)]
pub struct MiningJob {
    pub job_id: String,
    pub prevhash: String,
    pub coinb1: String,
    pub coinb2: String,
    pub merkle_branch: Vec<String>,
    pub version: String,
    pub nbits: String,
    pub ntime: String,
    pub clean_jobs: bool,
    pub difficulty: f64,
}

impl MiningJob {
    /// Convert to BlockHeader for mining with proper coinbase construction
    pub fn to_block_header(&self, extranonce1: &str, extranonce2: u32) -> Result<BlockHeader> {
        let prev_hash = Hash256::from_hex(&self.prevhash)?;
        
        // Build coinbase with both extranonces
        let coinbase = format!("{}{}{:08x}{}", 
            self.coinb1, 
            extranonce1,
            extranonce2,
            self.coinb2
        );
        
        // Calculate coinbase hash
        let coinbase_bytes = hex::decode(&coinbase)?;
        let coinbase_hash = crate::common::hash_types::double_sha256(&coinbase_bytes);
        
        // Build merkle root from coinbase and merkle branches
        let mut root = coinbase_hash;
        for branch in &self.merkle_branch {
            let branch_hash = Hash256::from_hex(branch)?;
            let mut combined = [0u8; 64];
            combined[..32].copy_from_slice(&root.0);
            combined[32..].copy_from_slice(&branch_hash.0);
            root = crate::common::hash_types::double_sha256(&combined);
        }
        
        Ok(BlockHeader {
            prev_hash,
            merkle_root: root,
            timestamp: u32::from_str_radix(&self.ntime, 16)?,
            bits: u32::from_str_radix(&self.nbits, 16)?,
            nonce: 0,
            height: 0, // Height calculated from epoch if needed
        })
    }
}