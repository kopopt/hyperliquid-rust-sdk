use alloy::signers::local::PrivateKeySigner;
use hyperliquid_rust_sdk::{
    BaseUrl, InfoClient, ExchangeClient,
    ClientLimit, ClientOrder, ClientOrderRequest,
};
use std::env;
use std::time::{Instant, Duration};
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr};
use std::collections::HashMap;

// Helper function to round to decimals
fn round_to_decimals(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 SDK 内部延迟详细分析");
    println!("{}", "=".repeat(60));
    
    let agent_key = env::var("HL_AGENT_KEY")?;
    let wallet: PrivateKeySigner = agent_key.parse()?;
    let symbol = "ETH";

    let optimized_client = Client::builder()
        .tcp_nodelay(true)
        .pool_idle_timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(10)
        .local_address(Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0))))
        .timeout(Duration::from_secs(30))
        .build()?;

    let info = InfoClient::new(Some(optimized_client.clone()), Some(BaseUrl::Mainnet)).await?;
    let meta = info.meta().await?;
    
    let exchange = ExchangeClient::new(
        Some(optimized_client.clone()),
        wallet,
        Some(BaseUrl::Mainnet),
        Some(meta),
        None,
    ).await?;

    let asset_meta = exchange.meta.universe
        .iter()
        .find(|c| c.name == symbol)
        .expect("找不到币种");
    let sz_decimals = asset_meta.sz_decimals;

    // 预热
    let _ = info.all_mids().await?;
    let all_mids = info.all_mids().await?;
    let mid_price: f64 = all_mids.get(symbol).unwrap().parse()?;
    let buy_px = (mid_price * 1.05).round() as f64;
    
    println!("目标: {} | 价格: ${:.2}", symbol, buy_px);
    println!();

    // 测试：分析 SDK 内部每个步骤
    println!("📊 SDK 内部步骤延迟分析（3 轮测试）");
    println!("{}", "-".repeat(60));

    for i in 1..=3 {
        println!("\n--- 轮次 {} ---", i);
        
        // 步骤 1: 构建订单请求
        let step1_start = Instant::now();
        let order = ClientOrderRequest {
            asset: symbol.to_string(),
            is_buy: true,
            reduce_only: false,
            limit_px: buy_px,
            sz: round_to_decimals(0.01, sz_decimals),
            cloid: None,
            order_type: ClientOrder::Limit(ClientLimit {
                tif: "Ioc".to_string(),
            }),
        };
        let step1_time = step1_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 1 - 构建订单请求: {:.2} ms", step1_time);

        // 步骤 2: 转换订单（order.convert）
        let step2_start = Instant::now();
        let transformed_order = order.convert(&exchange.coin_to_asset)?;
        let step2_time = step2_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 2 - 转换订单格式: {:.2} ms", step2_time);

        // 步骤 3: 构建 Action
        use hyperliquid_rust_sdk::exchange::actions::BulkOrder;
        let step3_start = Instant::now();
        let action = hyperliquid_rust_sdk::Actions::Order(BulkOrder {
            orders: vec![transformed_order],
            grouping: "na".to_string(),
            builder: None,
        });
        let step3_time = step3_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 3 - 构建 Action: {:.2} ms", step3_time);

        // 步骤 4: 生成 nonce
        use hyperliquid_rust_sdk::helpers::next_nonce;
        let step4_start = Instant::now();
        let timestamp = next_nonce();
        let step4_time = step4_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 4 - 生成 nonce: {:.2} ms", step4_time);

        // 步骤 5: 计算 connection_id (hash)
        use alloy::primitives::keccak256;
        use rmp_serde;
        let step5_start = Instant::now();
        let mut bytes = rmp_serde::to_vec_named(&action)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        bytes.extend(timestamp.to_be_bytes());
        bytes.push(0); // no vault_address
        let connection_id = keccak256(bytes);
        let step5_time = step5_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 5 - 计算 hash (序列化+hash): {:.2} ms", step5_time);

        // 步骤 6: 签名
        use hyperliquid_rust_sdk::signature::sign_l1_action;
        let step6_start = Instant::now();
        let is_mainnet = exchange.http_client.is_mainnet();
        let signature = sign_l1_action(&exchange.wallet, connection_id, is_mainnet)?;
        let step6_time = step6_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 6 - EIP-712 签名: {:.2} ms", step6_time);

        // 步骤 7: 序列化为 JSON
        let step7_start = Instant::now();
        let action_json = serde_json::to_value(&action)
            .map_err(|e| format!("Failed to serialize to JSON: {}", e))?;
        let step7_time = step7_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 7 - 序列化为 JSON: {:.2} ms", step7_time);

        // 步骤 8: 构建 payload
        use serde_json::json;
        use alloy::primitives::Signature;
        fn serialize_sig(sig: &Signature) -> serde_json::Value {
            json!({
                "r": format!("0x{:064x}", sig.r()),
                "s": format!("0x{:064x}", sig.s()),
                "v": 27 + sig.v() as u64,
            })
        }
        let step8_start = Instant::now();
        let payload = json!({
            "action": action_json,
            "nonce": timestamp,
            "signature": serialize_sig(&signature),
        });
        let step8_time = step8_start.elapsed().as_secs_f64() * 1000.0;
        println!("  步骤 8 - 构建 payload: {:.2} ms", step8_time);

        // 步骤 9: 发送 HTTP 请求（这是实际的网络请求）
        let step9_start = Instant::now();
        let payload_str = serde_json::to_string(&payload)?;
        let res = optimized_client
            .post("https://api.hyperliquid.xyz/exchange")
            .header("Content-Type", "application/json")
            .body(payload_str)
            .send()
            .await?;
        let step9_time = step9_start.elapsed().as_secs_f64() * 1000.0;
        let status = res.status();
        let _body = res.text().await?;
        println!("  步骤 9 - HTTP 请求: {:.2} ms | 状态: {}", step9_time, status);

        let total = step1_time + step2_time + step3_time + step4_time + 
                    step5_time + step6_time + step7_time + step8_time + step9_time;
        println!("  总计: {:.2} ms", total);

        tokio::time::sleep(Duration::from_millis(2000)).await;
    }

    println!();
    println!("{}", "=".repeat(60));
    println!("💡 分析");
    println!("{}", "-".repeat(60));
    println!("如果步骤 9 (HTTP 请求) 延迟接近总延迟，说明:");
    println!("  - SDK 处理很快（签名、序列化等 < 10ms）");
    println!("  - 延迟主要来自 Hyperliquid 服务器处理订单的时间");
    println!();
    println!("如果步骤 1-8 的延迟很高，说明:");
    println!("  - SDK 内部处理有优化空间");

    Ok(())
}
