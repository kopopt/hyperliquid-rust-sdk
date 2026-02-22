use reqwest::Client;
use std::time::{Instant, Duration};
use std::net::{IpAddr, Ipv4Addr};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 网络延迟诊断工具");
    println!("{}", "=".repeat(50));
    
    // 创建优化的 HTTP client
    let client = Client::builder()
        .tcp_nodelay(true)
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(10)
        .local_address(Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))))
        .timeout(Duration::from_secs(30))
        .build()?;

    let base_url = "https://api.hyperliquid.xyz";
    
    // 测试 1: 简单的 GET 请求（/info 端点，获取 meta）
    println!("\n📊 测试 1: GET /info (获取 meta)");
    println!("{}", "-".repeat(50));
    
    let mut get_times = Vec::new();
    for i in 1..=5 {
        let start = Instant::now();
        let res = client
            .get(format!("{}/info", base_url))
            .header("Content-Type", "application/json")
            .send()
            .await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        get_times.push(elapsed);
        
        let status = res.status();
        let body_size = res.text().await?.len();
        println!("  轮次 {}: {:.2} ms | 状态: {} | 响应大小: {} bytes", 
            i, elapsed, status, body_size);
        
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    let avg_get = get_times.iter().sum::<f64>() / get_times.len() as f64;
    let min_get = get_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_get = get_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", avg_get, min_get, max_get);

    // 测试 2: POST /info (查询 allMids)
    println!("\n📊 测试 2: POST /info (查询 allMids)");
    println!("{}", "-".repeat(50));
    
    let mut post_info_times = Vec::new();
    for i in 1..=5 {
        let start = Instant::now();
        let payload = json!({
            "type": "allMids"
        });
        
        let res = client
            .post(format!("{}/info", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&payload)?)
            .send()
            .await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        post_info_times.push(elapsed);
        
        let status = res.status();
        let body_size = res.text().await?.len();
        println!("  轮次 {}: {:.2} ms | 状态: {} | 响应大小: {} bytes", 
            i, elapsed, status, body_size);
        
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    let avg_post_info = post_info_times.iter().sum::<f64>() / post_info_times.len() as f64;
    let min_post_info = post_info_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_post_info = post_info_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", 
        avg_post_info, min_post_info, max_post_info);

    // 测试 3: POST /exchange (模拟下单请求，但不签名，应该会返回错误)
    println!("\n📊 测试 3: POST /exchange (无效请求，测试端点响应时间)");
    println!("{}", "-".repeat(50));
    
    let mut post_exchange_times = Vec::new();
    for i in 1..=5 {
        let start = Instant::now();
        // 发送一个无效的请求（没有签名），API 应该快速拒绝
        let payload = json!({
            "action": {
                "type": "order",
                "orders": [],
                "grouping": "na"
            },
            "nonce": 1234567890,
            "signature": {
                "r": "0x0",
                "s": "0x0",
                "v": 27
            }
        });
        
        let res = client
            .post(format!("{}/exchange", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&payload)?)
            .send()
            .await?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        post_exchange_times.push(elapsed);
        
        let status = res.status();
        let body = res.text().await?;
        println!("  轮次 {}: {:.2} ms | 状态: {} | 响应: {}...", 
            i, elapsed, status, 
            if body.len() > 50 { &body[..50] } else { &body });
        
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    let avg_post_exchange = post_exchange_times.iter().sum::<f64>() / post_exchange_times.len() as f64;
    let min_post_exchange = post_exchange_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_post_exchange = post_exchange_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", 
        avg_post_exchange, min_post_exchange, max_post_exchange);

    // 测试 4: DNS 解析时间
    println!("\n📊 测试 4: DNS 解析时间");
    println!("{}", "-".repeat(50));
    
    use std::net::ToSocketAddrs;
    let mut dns_times = Vec::new();
    for i in 1..=5 {
        let start = Instant::now();
        let _addrs: Vec<_> = "api.hyperliquid.xyz:443"
            .to_socket_addrs()?
            .collect();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        dns_times.push(elapsed);
        println!("  轮次 {}: {:.2} ms", i, elapsed);
    }
    
    let avg_dns = dns_times.iter().sum::<f64>() / dns_times.len() as f64;
    println!("  平均: {:.2} ms", avg_dns);

    // 总结
    println!("\n📈 总结");
    println!("{}", "=".repeat(50));
    println!("GET /info (meta):       平均 {:.2} ms", avg_get);
    println!("POST /info (allMids):   平均 {:.2} ms", avg_post_info);
    println!("POST /exchange:          平均 {:.2} ms", avg_post_exchange);
    println!("DNS 解析:               平均 {:.2} ms", avg_dns);
    println!("\n💡 分析：");
    println!("  - 如果 POST /exchange 延迟接近 GET/POST /info，说明是网络延迟");
    println!("  - 如果 POST /exchange 明显更慢，说明 Hyperliquid 处理订单需要更多时间");
    println!("  - 正常的网络 RTT 应该在 50-200ms（取决于地理位置）");
    println!("  - 如果延迟 > 500ms，可能是网络问题或服务器负载高");

    Ok(())
}
