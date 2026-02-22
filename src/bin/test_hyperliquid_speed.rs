use alloy::signers::local::PrivateKeySigner;
use hyperliquid_rust_sdk::{
    BaseUrl, ExchangeClient, InfoClient, 
    ExchangeResponseStatus, ExchangeDataStatus,
    ClientCancelRequest, ClientLimit, ClientOrder, ClientOrderRequest,
};
use std::env;
use std::time::{Instant, Duration};
use tokio::time::sleep;

// Helper function to round to decimals (same as SDK internal function)
fn round_to_decimals(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let agent_key = env::var("HL_AGENT_KEY")?;
    let wallet: PrivateKeySigner = agent_key.parse()?;
    let address = wallet.address();
    let symbol = "ETH";

    // 注意：reqwest 版本问题可能导致自定义 client 无法被 SDK 使用
    // 如果延迟还是高，尝试移除自定义 client，让 SDK 使用默认配置
    // let custom_client = reqwest::Client::builder()
    //     .local_address(Some(std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)))) 
    //     .tcp_nodelay(true)
    //     .pool_idle_timeout(Duration::from_secs(300))
    //     .build()?;

    // 创建优化的 HTTP client 以减少延迟
    let optimized_client = reqwest::Client::builder()
        .tcp_nodelay(true)  // 禁用 Nagle 算法，减少延迟
        .pool_idle_timeout(std::time::Duration::from_secs(90))  // 保持连接池活跃
        .pool_max_idle_per_host(10)  // 增加每个主机的连接池大小
        .timeout(std::time::Duration::from_secs(30))  // 设置超时
        .build()?;
    
    let info = InfoClient::new(Some(optimized_client.clone()), Some(BaseUrl::Mainnet)).await?;
    let meta = info.meta().await?;
    
    let exchange = ExchangeClient::new(
        Some(optimized_client), 
        wallet, 
        Some(BaseUrl::Mainnet), 
        Some(meta), 
        None 
    ).await?;

    println!("🚀 极速引擎已就绪 | 目标: {}", symbol);
    println!("✅ 已优化：直接使用底层 order API，绕过 market_open");
    println!("💡 如果延迟仍然 > 200ms，可能是网络延迟或 Hyperliquid API 响应慢");

    // 预热：只做一次，建立连接池
    let _ = info.user_state(address).await;
    let all_mids = info.all_mids().await?;
    let mid_price: f64 = all_mids.get(symbol).and_then(|p| p.parse().ok()).unwrap_or(2000.0);
    let buy_px = (mid_price * 1.05).round() as f64;
    println!("预热完成 | 当前价格: ${:.2} | 下单价格: ${:.2}", mid_price, buy_px);

    // 获取资产元数据（用于格式化数量）
    let asset_meta = exchange.meta.universe
        .iter()
        .find(|a| a.name == symbol)
        .ok_or("Asset not found")?;
    let sz_decimals = asset_meta.sz_decimals;

    for i in 1..=5 {
        // 关键优化：直接使用底层 order API，绕过 market_open 和 calculate_slippage_price
        let total_start = Instant::now();
        
        // 步骤 1: 构建订单请求（应该很快，<1ms）
        let build_start = Instant::now();
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
        let build_time = build_start.elapsed().as_secs_f64() * 1000.0;
        
        // 步骤 2: 调用 order 方法（这是实际的网络请求）
        let order_start = Instant::now();
        let res = exchange.order(order, None).await;
        let order_time = order_start.elapsed().as_secs_f64() * 1000.0;

        let total_time = total_start.elapsed().as_secs_f64() * 1000.0;

        match res {
            Ok(ExchangeResponseStatus::Ok(data)) => {
                if let Some(status) = data.data.as_ref().and_then(|d| d.statuses.first()) {
                    match status {
                        ExchangeDataStatus::Filled(f) => {
                            println!("轮次 {}: 总延迟 {:.2} ms (构建: {:.2} ms, 下单: {:.2} ms) | [瞬间成交] OID: {}", 
                                i, total_time, build_time, order_time, f.oid);
                        },
                        ExchangeDataStatus::Resting(r) => {
                            println!("轮次 {}: 总延迟 {:.2} ms (构建: {:.2} ms, 下单: {:.2} ms) | [挂单成功] OID: {}", 
                                i, total_time, build_time, order_time, r.oid);
                            // 仅测试用：挂单成功后撤单
                            let _ = exchange.cancel(ClientCancelRequest { asset: symbol.to_string(), oid: r.oid }, None).await;
                        },
                        _ => println!("轮次 {}: 总延迟 {:.2} ms (构建: {:.2} ms, 下单: {:.2} ms) | 状态: {:?}", 
                            i, total_time, build_time, order_time, status),
                    }
                }
            }
            Err(e) => println!("轮次 {}: ❌ 异常 - {:?} (总延迟: {:.2} ms)", i, e, total_time),
            _ => println!("轮次 {}: ❌ 非预期回执 (总延迟: {:.2} ms)", i, total_time),
        }
        sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}