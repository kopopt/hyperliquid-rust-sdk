use alloy::signers::local::PrivateKeySigner;
use hyperliquid_rust_sdk::{
    BaseUrl, ExchangeClient, InfoClient, 
    ExchangeResponseStatus, ExchangeDataStatus,
    MarketOrderParams, ClientCancelRequest,
};
use std::env;
use std::time::{Instant, Duration};
use tokio::time::sleep;

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

    // 使用 SDK 默认 client（可能更稳定）
    let info = InfoClient::new(None, Some(BaseUrl::Mainnet)).await?;
    let meta = info.meta().await?;
    
    let exchange = ExchangeClient::new(
        None, 
        wallet, 
        Some(BaseUrl::Mainnet), 
        Some(meta), 
        None 
    ).await?;

    println!("🚀 极速引擎已就绪 | 目标: {}", symbol);
    println!("⚠️  如果延迟 > 200ms，可能是 market_open 内部做了额外请求");
    println!("💡 建议：使用底层 order API 或检查 SDK 源码");

    // 预热：只做一次，建立连接池
    let _ = info.user_state(address).await;
    let all_mids = info.all_mids().await?;
    let mid_price: f64 = all_mids.get(symbol).and_then(|p| p.parse().ok()).unwrap_or(2000.0);
    let buy_px = (mid_price * 1.05).round() as f64;
    println!("预热完成 | 当前价格: ${:.2} | 下单价格: ${:.2}", mid_price, buy_px);

    for i in 1..=5 {
        // 关键优化：只测量下单请求的延迟
        let start = Instant::now();

        let order_params = MarketOrderParams {
            asset: symbol,
            is_buy: true,
            sz: 0.01,
            px: Some(buy_px),
            slippage: None,
            cloid: None,
            wallet: None,
        };
        
        // 注意：market_open 内部可能做了额外请求（获取价格、计算滑点等）
        // 如果延迟还是高，可能需要直接使用底层 order API
        let res = exchange.market_open(order_params).await;

        let ms = start.elapsed().as_secs_f64() * 1000.0;

        match res {
            Ok(ExchangeResponseStatus::Ok(data)) => {
                if let Some(status) = data.data.as_ref().and_then(|d| d.statuses.first()) {
                    match status {
                        ExchangeDataStatus::Filled(f) => {
                            println!("轮次 {}: ⏱️ {:.2} ms | [瞬间成交] OID: {}", i, ms, f.oid);
                        },
                        ExchangeDataStatus::Resting(r) => {
                            println!("轮次 {}: ⏱️ {:.2} ms | [挂单成功] OID: {}", i, ms, r.oid);
                            // 仅测试用：挂单成功后撤单
                            let _ = exchange.cancel(ClientCancelRequest { asset: symbol.to_string(), oid: r.oid }, None).await;
                        },
                        _ => println!("轮次 {}: ⏱️ {:.2} ms | 状态: {:?}", i, ms, status),
                    }
                }
            }
            Err(e) => println!("轮次 {}: ❌ 异常 - {:?}", i, e),
            _ => println!("轮次 {}: ❌ 非预期回执", i),
        }
        sleep(Duration::from_millis(1000)).await;
    }

    Ok(())
}