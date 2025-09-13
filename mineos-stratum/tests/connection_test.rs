use mineos_stratum::{PoolConfig, StratumClient, StratumConfig, FailoverStrategy};
use std::time::Duration;

#[tokio::test]
async fn test_stratum_client_creation() {
    let config = StratumConfig {
        pools: vec![
            PoolConfig {
                name: "test_pool".to_string(),
                url: "stratum+tcp://test.pool.com:3333".to_string(),
                username: "test_user".to_string(),
                password: "x".to_string(),
                priority: 0,
                weight: 1,
                enabled: true,
            },
        ],
        failover_strategy: FailoverStrategy::Priority,
        connection_timeout: Duration::from_secs(5),
        response_timeout: Duration::from_secs(10),
        keepalive_interval: Duration::from_secs(30),
        max_reconnect_attempts: 3,
        reconnect_backoff_ms: 1000,
        max_reconnect_backoff_ms: 30000,
        share_buffer_size: 100,
        user_agent: "MineOS/1.0".to_string(),
    };
    
    let (client, _job_rx) = StratumClient::new(config);
    
    // Just verify we can create the client
    assert!(client.get_stats().await.shares_accepted == 0);
}

#[tokio::test]
async fn test_pool_config_parsing() {
    let pool = PoolConfig {
        name: "test".to_string(),
        url: "stratum+tcp://pool.example.com:3333".to_string(),
        username: "user".to_string(),
        password: "pass".to_string(),
        priority: 0,
        weight: 1,
        enabled: true,
    };
    
    let (host, port) = pool.parse_url().unwrap();
    assert_eq!(host, "pool.example.com");
    assert_eq!(port, 3333);
}