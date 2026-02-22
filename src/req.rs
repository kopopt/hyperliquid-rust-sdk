use reqwest::{Client, Response};
use serde::Deserialize;

use crate::{prelude::*, BaseUrl, Error};

#[derive(Deserialize, Debug)]
struct ErrorData {
    data: String,
    code: u16,
    msg: String,
}

#[derive(Debug)]
pub struct HttpClient {
    pub client: Client,
    pub base_url: String,
}

async fn parse_response(response: Response) -> Result<String> {
    let status_code = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| Error::GenericRequest(e.to_string()))?;

    if status_code < 400 {
        return Ok(text);
    }
    let error_data = serde_json::from_str::<ErrorData>(&text);
    if (400..500).contains(&status_code) {
        let client_error = match error_data {
            Ok(error_data) => Error::ClientRequest {
                status_code,
                error_code: Some(error_data.code),
                error_message: error_data.msg,
                error_data: Some(error_data.data),
            },
            Err(err) => Error::ClientRequest {
                status_code,
                error_message: text,
                error_code: None,
                error_data: Some(err.to_string()),
            },
        };
        return Err(client_error);
    }

    Err(Error::ServerRequest {
        status_code,
        error_message: text,
    })
}

impl HttpClient {
    pub async fn post(&self, url_path: &'static str, data: String) -> Result<String> {
        let perf_profile = std::env::var("HL_PERF_PROFILE").is_ok();
        let http_start = if perf_profile { Some(std::time::Instant::now()) } else { None };

        // Step 1: Build request
        let step1_start = if perf_profile { Some(std::time::Instant::now()) } else { None };
        let full_url = format!("{}{url_path}", self.base_url);
        let request = self
            .client
            .post(full_url)
            .header("Content-Type", "application/json")
            .body(data)
            .build()
            .map_err(|e| Error::GenericRequest(e.to_string()))?;
        if let Some(start) = step1_start {
            let time = start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("[PERF] HTTP Step 1 - Build request: {:.2}ms", time);
        }

        // Step 2: Execute request (network round trip + server processing)
        let step2_start = if perf_profile { Some(std::time::Instant::now()) } else { None };
        let result = self
            .client
            .execute(request)
            .await
            .map_err(|e| Error::GenericRequest(e.to_string()))?;
        if let Some(start) = step2_start {
            let step2_time = start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("[PERF] HTTP Step 2 - Execute (network + server): {:.2}ms", step2_time);
        }

        // Step 3: Parse response
        let step3_start = if perf_profile { Some(std::time::Instant::now()) } else { None };
        let result = parse_response(result).await;
        if let Some(start) = step3_start {
            let time = start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("[PERF] HTTP Step 3 - Parse response: {:.2}ms", time);
        }
        if let Some(start) = http_start {
            let time = start.elapsed().as_secs_f64() * 1000.0;
            eprintln!("[PERF] HTTP total time: {:.2}ms", time);
        }
        
        result
    }

    pub fn is_mainnet(&self) -> bool {
        self.base_url == BaseUrl::Mainnet.get_url()
    }
}
