use crate::cmd::{fetch_and_persist_accounts_with_client, simulate};
use crate::swap::SwapDirection;
use axum::{Router, http::StatusCode, response::Json, routing::post};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, sleep};

pub async fn run_service(port: u16, rpc_url: String, fetch_interval_ms: u64) -> eyre::Result<()> {
    tokio::spawn(async move { fetch_state_task(rpc_url, fetch_interval_ms).await });

    // Give the fetcher a moment to populate state
    sleep(Duration::from_millis(500)).await;

    let app = Router::new().route("/", post(handle_jsonrpc));

    let addr = format!("0.0.0.0:{}", port);
    println!("Starting RPC server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn fetch_state_task(rpc_url: String, interval_ms: u64) {
    let mut interval = interval(Duration::from_millis(interval_ms));

    let client = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());

    loop {
        interval.tick().await;

        if let Err(e) = fetch_and_persist_accounts_with_client(&client).await {
            eprintln!("Failed to fetch accounts: {}", e);
        }
    }
}

#[derive(Deserialize)]
struct JsonRpcRequest {
    method: String,
    id: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    id: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Serialize)]
struct PriceQuote {
    amount_sol: f64,
    price_usdc: f64,
    best_market: String,
}

#[derive(Serialize)]
struct PricesResponse {
    sell_sol: Vec<PriceQuote>,
    buy_sol: Vec<PriceQuote>,
    timestamp: u64,
}

async fn handle_jsonrpc(
    Json(req): Json<JsonRpcRequest>,
) -> Result<Json<JsonRpcResponse>, StatusCode> {
    let response = match req.method.as_str() {
        "get_prices" => handle_get_prices(req.id).await,
        _ => Json(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: "Method not found".to_string(),
                data: None,
            }),
            id: req.id,
        }),
    };

    Ok(response)
}

async fn handle_get_prices(id: Option<Value>) -> Json<JsonRpcResponse> {
    let amounts = vec![1.0, 10.0, 100.0];
    let mut sell_sol_quotes = Vec::new();
    let mut buy_sol_quotes = Vec::new();

    for amount in &amounts {
        match simulate(SwapDirection::SolToUsdc, Some(*amount), None, false, false) {
            Ok(results) => {
                if let Some(best) = results
                    .iter()
                    .filter_map(|r| r.out_amount.map(|out| (r, out)))
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                {
                    sell_sol_quotes.push(PriceQuote {
                        amount_sol: *amount,
                        price_usdc: best.1 / amount,
                        best_market: best.0.market.clone(),
                    });
                }
            }
            Err(e) => {
                return Json(JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: format!("Failed to simulate sell: {}", e),
                        data: None,
                    }),
                    id,
                });
            }
        }
    }

    for target_sol in &amounts {
        let initial_price =
            if !sell_sol_quotes.is_empty() { sell_sol_quotes[0].price_usdc * 1.01 } else { 150.0 };

        let mut estimated_usdc = target_sol * initial_price;
        let mut best_result = None;
        let mut iterations = 0;
        const MAX_ITERATIONS: i32 = 10;
        const TOLERANCE: f64 = 0.01;

        while iterations < MAX_ITERATIONS {
            match simulate(SwapDirection::UsdcToSol, Some(estimated_usdc), None, false, false) {
                Ok(results) => {
                    if let Some(best) = results
                        .iter()
                        .filter_map(|r| r.out_amount.map(|out| (r, out)))
                        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    {
                        let sol_received = best.1;
                        let ratio = sol_received / target_sol;

                        if (ratio - 1.0).abs() < TOLERANCE {
                            best_result =
                                Some((best.0.market.clone(), estimated_usdc, sol_received));
                            break;
                        }

                        estimated_usdc = estimated_usdc / ratio;
                        best_result = Some((best.0.market.clone(), estimated_usdc, sol_received));
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
            iterations += 1;
        }

        if let Some((market, usdc_needed, sol_received)) = best_result {
            buy_sol_quotes.push(PriceQuote {
                amount_sol: sol_received,
                price_usdc: usdc_needed / sol_received,
                best_market: market,
            });
        }
    }

    let response = PricesResponse {
        sell_sol: sell_sol_quotes,
        buy_sol: buy_sol_quotes,
        timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
    };

    Json(JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        result: Some(serde_json::to_value(response).unwrap()),
        error: None,
        id,
    })
}
