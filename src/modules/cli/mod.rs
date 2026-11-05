use std::fs;

pub async fn run_pull_command(
    url: &str,
    token: &str,
    format: &str,
    out: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_url = url.trim_end_matches('/');
    let export_url = format!("{}/api/v1/export?format={}", clean_url, format);

    println!("⚡ ExaVault Pulling secrets from: {}", clean_url);

    let client = reqwest::Client::new();
    let res = client
        .get(&export_url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    if !res.status().is_success() {
        let err_text = res.text().await.unwrap_or_default();
        eprintln!("❌ Failed to export secrets: {}", err_text);
        return Err("Export API error".into());
    }

    let content = res.text().await?;

    if let Some(out_path) = out {
        fs::write(out_path, &content)?;
        println!("✅ Successfully exported secrets to {}", out_path);
    } else {
        println!("{}", content);
    }

    Ok(())
}

