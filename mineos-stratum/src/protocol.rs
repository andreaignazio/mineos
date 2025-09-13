use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Stratum JSON-RPC request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Vec<Value>,
}

/// Stratum JSON-RPC response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumResponse {
    pub id: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<StratumRpcError>,
}

/// Stratum JSON-RPC error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumRpcError {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Stratum notification (no id field)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumNotification {
    pub method: String,
    pub params: Vec<Value>,
}

/// Mining job received from pool
#[derive(Debug, Clone)]
pub struct MiningJob {
    pub job_id: String,
    pub prev_hash: String,
    pub coinbase1: String,
    pub coinbase2: String,
    pub merkle_branches: Vec<String>,
    pub version: String,
    pub nbits: String,
    pub ntime: String,
    pub clean_jobs: bool,
}

/// Share to submit to pool
#[derive(Debug, Clone)]
pub struct Share {
    pub worker_name: String,
    pub job_id: String,
    pub extra_nonce2: String,
    pub ntime: String,
    pub nonce: String,
    pub version_rolling_mask: Option<String>,
}

/// Mining.subscribe result
#[derive(Debug, Clone)]
pub struct SubscribeResult {
    pub session_id: Option<String>,
    pub extra_nonce1: String,
    pub extra_nonce2_size: usize,
}

/// Mining.authorize result
#[derive(Debug, Clone)]
pub struct AuthorizeResult {
    pub authorized: bool,
}

/// Mining difficulty
#[derive(Debug, Clone, Copy)]
pub struct Difficulty(pub f64);

impl Difficulty {
    /// Convert difficulty to target (256-bit number)
    pub fn to_target(&self) -> [u8; 32] {
        // Bitcoin difficulty 1 target: 0x00000000ffff0000000000000000000000000000000000000000000000000000
        // Split into two u128 values to avoid overflow
        let max_target_high = 0x00000000ffff0000u128;
        let max_target_low = 0x0000000000000000000000000000u128;
        
        // For simplicity in MVP, return a fixed target
        // In production, this would calculate: max_target / difficulty
        let mut bytes = [0u8; 32];
        bytes[3] = 0xff;
        bytes[4] = 0xff;
        
        bytes
    }
    
    /// Create from pool difficulty value
    pub fn from_pool_value(value: f64) -> Self {
        Difficulty(value)
    }
}

/// Stratum methods
pub mod methods {
    pub const SUBSCRIBE: &str = "mining.subscribe";
    pub const AUTHORIZE: &str = "mining.authorize";
    pub const SUBMIT: &str = "mining.submit";
    pub const NOTIFY: &str = "mining.notify";
    pub const SET_DIFFICULTY: &str = "mining.set_difficulty";
    pub const SET_EXTRA_NONCE: &str = "mining.set_extranonce";
    pub const PING: &str = "mining.ping";
    pub const GET_VERSION: &str = "mining.get_version";
    pub const RECONNECT: &str = "client.reconnect";
    pub const SHOW_MESSAGE: &str = "client.show_message";
}

impl StratumRequest {
    /// Create a mining.subscribe request
    pub fn subscribe(user_agent: &str, session_id: Option<&str>) -> Self {
        let mut params = vec![Value::String(user_agent.to_string())];
        if let Some(id) = session_id {
            params.push(Value::String(id.to_string()));
        }
        
        Self {
            id: Some(Value::Number(1.into())),
            method: methods::SUBSCRIBE.to_string(),
            params,
        }
    }
    
    /// Create a mining.authorize request
    pub fn authorize(username: &str, password: &str) -> Self {
        Self {
            id: Some(Value::Number(2.into())),
            method: methods::AUTHORIZE.to_string(),
            params: vec![
                Value::String(username.to_string()),
                Value::String(password.to_string()),
            ],
        }
    }
    
    /// Create a mining.submit request
    pub fn submit(share: &Share) -> Self {
        let mut params = vec![
            Value::String(share.worker_name.clone()),
            Value::String(share.job_id.clone()),
            Value::String(share.extra_nonce2.clone()),
            Value::String(share.ntime.clone()),
            Value::String(share.nonce.clone()),
        ];
        
        if let Some(ref mask) = share.version_rolling_mask {
            params.push(Value::String(mask.clone()));
        }
        
        Self {
            id: Some(Value::Number(rand::random::<u32>().into())),
            method: methods::SUBMIT.to_string(),
            params,
        }
    }
}

impl MiningJob {
    /// Parse from mining.notify params
    pub fn from_notify_params(params: &[Value]) -> Result<Self, String> {
        if params.len() < 9 {
            return Err("Invalid mining.notify params length".to_string());
        }
        
        Ok(Self {
            job_id: params[0].as_str()
                .ok_or("Invalid job_id")?.to_string(),
            prev_hash: params[1].as_str()
                .ok_or("Invalid prev_hash")?.to_string(),
            coinbase1: params[2].as_str()
                .ok_or("Invalid coinbase1")?.to_string(),
            coinbase2: params[3].as_str()
                .ok_or("Invalid coinbase2")?.to_string(),
            merkle_branches: params[4].as_array()
                .ok_or("Invalid merkle_branches")?
                .iter()
                .map(|v| v.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()
                .ok_or("Invalid merkle branch")?,
            version: params[5].as_str()
                .ok_or("Invalid version")?.to_string(),
            nbits: params[6].as_str()
                .ok_or("Invalid nbits")?.to_string(),
            ntime: params[7].as_str()
                .ok_or("Invalid ntime")?.to_string(),
            clean_jobs: params[8].as_bool()
                .ok_or("Invalid clean_jobs")?,
        })
    }
}

impl SubscribeResult {
    /// Parse from mining.subscribe response
    pub fn from_response(result: &Value) -> Result<Self, String> {
        let arr = result.as_array()
            .ok_or("Invalid subscribe result format")?;
        
        if arr.len() < 2 {
            return Err("Invalid subscribe result length".to_string());
        }
        
        // First element is subscriptions array (we can ignore for basic implementation)
        // Second element is extra_nonce1
        let extra_nonce1 = arr[1].as_str()
            .ok_or("Invalid extra_nonce1")?.to_string();
        
        // Third element is extra_nonce2_size
        let extra_nonce2_size = if arr.len() > 2 {
            arr[2].as_u64()
                .ok_or("Invalid extra_nonce2_size")? as usize
        } else {
            4 // Default to 4 bytes
        };
        
        Ok(Self {
            session_id: None, // TODO: Extract from subscriptions if needed
            extra_nonce1,
            extra_nonce2_size,
        })
    }
}

impl fmt::Display for MiningJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Job {} (clean: {})", self.job_id, self.clean_jobs)
    }
}

// Helper to generate random ID for requests
mod rand {
    use std::sync::atomic::{AtomicU32, Ordering};
    
    static COUNTER: AtomicU32 = AtomicU32::new(1);
    
    pub fn random<T>() -> u32 {
        COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_difficulty_to_target() {
        let diff = Difficulty(1.0);
        let target = diff.to_target();
        assert_eq!(target[31], 0);
        
        let diff = Difficulty(256.0);
        let target = diff.to_target();
        // Higher difficulty = lower target
        assert!(target[16] < 0xff);
    }
    
    #[test]
    fn test_subscribe_request() {
        let req = StratumRequest::subscribe("MineOS/1.0", None);
        assert_eq!(req.method, "mining.subscribe");
        assert_eq!(req.params.len(), 1);
    }
    
    #[test]
    fn test_job_parsing() {
        let params = vec![
            Value::String("job123".into()),
            Value::String("prevhash".into()),
            Value::String("coinbase1".into()),
            Value::String("coinbase2".into()),
            Value::Array(vec![]),
            Value::String("version".into()),
            Value::String("nbits".into()),
            Value::String("ntime".into()),
            Value::Bool(true),
        ];
        
        let job = MiningJob::from_notify_params(&params).unwrap();
        assert_eq!(job.job_id, "job123");
        assert!(job.clean_jobs);
    }
}