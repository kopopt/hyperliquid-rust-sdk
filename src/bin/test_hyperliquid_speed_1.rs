use alloy::signers::local::PrivateKeySigner;
use hyperliquid_rust_sdk::{
    BaseUrl, InfoClient, ExchangeClient,
    ClientLimit, ClientOrder, ClientOrderRequest,
    ExchangeResponseStatus, ExchangeDataStatus,
};
use std::env;
use std::time::{Instant, Duration};
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr};

// Helper function to round to decimals (same as SDK internal function)
fn round_to_decimals(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化（全部放在计时器外）
    let agent_key = env::var("HL_AGENT_KEY")?;
    let wallet: PrivateKeySigner = agent_key.parse()?;
    let symbol = "ETH";

    // 打造高性能 Client：强制 IPv4，TCP_NODELAY
    let optimized_client = Client::builder()
        .tcp_nodelay(true)
        .pool_idle_timeout(Duration::from_secs(300))
        .pool_max_idle_per_host(10)
        .local_address(Some(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)))) 
        .timeout(Duration::from_secs(30))
        .build()?;

    let info = InfoClient::new(Some(optimized_client.clone()), Some(BaseUrl::Mainnet)).await?;
    
    // 预抓取 Meta 存入内存，下单时绝不重新请求
    println!("📡 正在预取 Meta 数据...");
    let meta = info.meta().await?;
    
    // 创建 ExchangeClient，传入已缓存的 meta
    let exchange = ExchangeClient::new(
        Some(optimized_client.clone()),
        wallet,
        Some(BaseUrl::Mainnet),
        Some(meta),
        None,
    ).await?;

    // 获取资产 ID 和元数据
    let asset_meta = exchange.meta.universe
        .iter()
        .find(|c| c.name == symbol)
        .expect("找不到币种");
    let sz_decimals = asset_meta.sz_decimals;

    println!("🚀 极速执行器就绪 | 目标: {} (sz_decimals: {})", symbol, sz_decimals);

    // 预热连接隧道
    let _ = optimized_client.get("https://api.hyperliquid.xyz/info").send().await?;

    for i in 1..=5 {
        // 模拟策略：在计时器外获取最新价格
        let all_mids = info.all_mids().await?;
        let mid_price: f64 = all_mids.get(symbol).unwrap().parse()?;
        let buy_px = (mid_price * 1.05).round() as f64; // 算好滑点价

        // --- 核心执行区 (Latency Sensitive) ---
        let start = Instant::now();

        // 直接使用 SDK 的 order 方法（已经优化过，使用缓存的 meta）
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

        let res = exchange.order(order, None).await;

        let ms = start.elapsed().as_secs_f64() * 1000.0;
        
        // --- 结果分析 ---
        match res {
            Ok(ExchangeResponseStatus::Ok(data)) => {
                if let Some(status) = data.data.as_ref().and_then(|d| d.statuses.first()) {
                    match status {
                        ExchangeDataStatus::Filled(f) => {
                            println!("轮次 {}: ⏱️ {:.2} ms | [瞬间成交] OID: {}", i, ms, f.oid);
                        },
                        ExchangeDataStatus::Resting(r) => {
                            println!("轮次 {}: ⏱️ {:.2} ms | [挂单成功] OID: {}", i, ms, r.oid);
                        },
                        _ => println!("轮次 {}: ⏱️ {:.2} ms | 状态: {:?}", i, ms, status),
                    }
                } else {
                    println!("轮次 {}: ⏱️ {:.2} ms | 状态: OK (无状态信息)", i, ms);
                }
            }
            Ok(ExchangeResponseStatus::Err(e)) => {
                println!("轮次 {}: ⏱️ {:.2} ms | ❌ 错误: {:?}", i, ms, e);
            }
            Err(e) => {
                println!("轮次 {}: ⏱️ {:.2} ms | ❌ 异常: {:?}", i, ms, e);
            }
        }

        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}
