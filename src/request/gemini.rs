use anyhow::Result;
use converter::models::gemini_cli;

const GEMINI_API_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse";

pub async fn gemini_request(req: gemini_cli::Request, token: &str) -> Result<reqwest::Response> {
    let resp = reqwest::Client::new()
        .post(GEMINI_API_URL)
        .bearer_auth(token)
        .json(&req)
        .send()
        .await?;
    Ok(resp)
}
