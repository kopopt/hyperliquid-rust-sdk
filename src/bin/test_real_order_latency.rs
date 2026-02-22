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

// Helper function to round to decimals
fn round_to_decimals(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 真实订单延迟分析");
    println!("{}", "=".repeat(60));
    
    let agent_key = env::var("HL_AGENT_KEY")?;
    let wallet: PrivateKeySigner = agent_key.parse()?;
    let symbol = "ETH";

    // 创建优化的 HTTP client
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

    println!("目标: {} (sz_decimals: {})", symbol, sz_decimals);
    println!();

    // 预热
    let _ = info.all_mids().await?;
    let all_mids = info.all_mids().await?;
    let mid_price: f64 = all_mids.get(symbol).unwrap().parse()?;
    let buy_px = (mid_price * 1.05).round() as f64;
    println!("当前价格: ${:.2} | 下单价格: ${:.2}", mid_price, buy_px);
    println!();

    // 测试：详细分析每个步骤的延迟
    println!("📊 详细延迟分析（5 轮测试）");
    println!("{}", "-".repeat(60));

    let mut total_times = Vec::new();
    let mut build_times = Vec::new();
    let mut order_times = Vec::new();

    for i in 1..=5 {
        let total_start = Instant::now();
        
        // 步骤 1: 构建订单请求
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
        build_times.push(build_time);
        
        // 步骤 2: 调用 order 方法（包含签名、序列化、网络请求）
        let order_start = Instant::now();
        let res = exchange.order(order, None).await;
        let order_time = order_start.elapsed().as_secs_f64() * 1000.0;
        order_times.push(order_time);
        
        let total_time = total_start.elapsed().as_secs_f64() * 1000.0;
        total_times.push(total_time);

        match res {
            Ok(ExchangeResponseStatus::Ok(data)) => {
                if let Some(status) = data.data.as_ref().and_then(|d| d.statuses.first()) {
                    match status {
                        ExchangeDataStatus::Filled(f) => {
                            println!("轮次 {}: 总={:.2}ms (构建={:.2}ms, 下单={:.2}ms) | [成交] OID: {}", 
                                i, total_time, build_time, order_time, f.oid);
                        },
                        ExchangeDataStatus::Resting(r) => {
                            println!("轮次 {}: 总={:.2}ms (构建={:.2}ms, 下单={:.2}ms) | [挂单] OID: {}", 
                                i, total_time, build_time, order_time, r.oid);
                        },
                        _ => {
                            println!("轮次 {}: 总={:.2}ms (构建={:.2}ms, 下单={:.2}ms) | 状态: {:?}", 
                                i, total_time, build_time, order_time, status);
                        }
                    }
                }
            }
            Ok(ExchangeResponseStatus::Err(e)) => {
                println!("轮次 {}: 总={:.2}ms (构建={:.2}ms, 下单={:.2}ms) | ❌ 错误: {:?}", 
                    i, total_time, build_time, order_time, e);
            }
            Err(e) => {
                println!("轮次 {}: 总={:.2}ms (构建={:.2}ms, 下单={:.2}ms) | ❌ 异常: {:?}", 
                    i, total_time, build_time, order_time, e);
            }
        }

        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    // 统计
    println!();
    println!("{}", "=".repeat(60));
    println!("📈 统计结果");
    println!("{}", "-".repeat(60));
    
    let avg_total = total_times.iter().sum::<f64>() / total_times.len() as f64;
    let min_total = total_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_total = total_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    
    let avg_build = build_times.iter().sum::<f64>() / build_times.len() as f64;
    let min_build = build_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_build = build_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    
    let avg_order = order_times.iter().sum::<f64>() / order_times.len() as f64;
    let min_order = order_times.iter().fold(f64::INFINITY, |a: f64, &b| a.min(b));
    let max_order = order_times.iter().fold(0.0f64, |a: f64, &b| a.max(b));
    
    println!("构建订单请求:");
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", avg_build, min_build, max_build);
    println!();
    println!("下单请求 (包含签名+序列化+网络+处理):");
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", avg_order, min_order, max_order);
    println!();
    println!("总延迟:");
    println!("  平均: {:.2} ms | 最小: {:.2} ms | 最大: {:.2} ms", avg_total, min_total, max_total);
    println!();
    
    // 分析
    println!("{}", "=".repeat(60));
    println!("💡 延迟分析");
    println!("{}", "-".repeat(60));
    println!("网络测试显示:");
    println!("  - POST /info: ~5ms");
    println!("  - POST /exchange (无效请求): ~8ms");
    println!("  - DNS 解析: ~0.4ms");
    println!();
    println!("实际下单延迟: {:.2}ms", avg_order);
    println!();
    
    if avg_order < 50.0 {
        println!("✅ 延迟正常！下单延迟 ({:.2}ms) 接近网络延迟 (~8ms)", avg_order);
        println!("   说明 SDK 和 Hyperliquid API 都很快");
    } else if avg_order < 200.0 {
        println!("⚠️  延迟略高，但可接受");
        println!("   下单延迟 ({:.2}ms) 比网络延迟 (~8ms) 高 {:.2}ms", avg_order, avg_order - 8.0);
        println!("   可能是签名/序列化处理时间");
    } else {
        println!("❌ 延迟较高！");
        println!("   下单延迟 ({:.2}ms) 比网络延迟 (~8ms) 高 {:.2}ms", avg_order, avg_order - 8.0);
        println!("   可能的原因:");
        println!("   1. SDK 内部处理有延迟");
        println!("   2. Hyperliquid 处理有效订单需要更多时间");
        println!("   3. 签名/序列化过程较慢");
    }

    Ok(())
}
