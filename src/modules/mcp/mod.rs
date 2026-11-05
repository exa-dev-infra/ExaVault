use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{self, BufRead, Write},
};

#[derive(Deserialize, Debug)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize, Debug)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("⚡ ExaVault MCP Server started (stdio JSON-RPC)");

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout();

    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        if let Ok(req) = serde_json::from_str::<JsonRpcRequest>(trimmed) {
            let resp = handle_mcp_request(req).await;
            if let Some(r) = resp {
                let serialized = serde_json::to_string(&r)?;
                stdout.write_all(serialized.as_bytes())?;
                stdout.write_all(b"\n")?;
                stdout.flush()?;
            }
        }

        line.clear();
    }

    Ok(())
}

async fn handle_mcp_request(req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id.clone().unwrap_or(Value::Null);

    match req.method.as_str() {
        "initialize" => {
            let result = json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "exavault-mcp",
                    "version": "0.1.0"
                },
                "capabilities": {
                    "tools": {}
                }
            });
            Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(result),
                error: None,
            })
        }

        "notifications/initialized" => None,

        "tools/list" => {
            let tools = json!({
                "tools": [
                    {
                        "name": "exavault_export_env",
                        "description": "Fetch decrypted environment variables from ExaVault using a service token. Optionally writes to a target file (like .env).",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "token": {
                                    "type": "string",
                                    "description": "ExaVault service token (e.g. exv_...). If omitted, uses EXAVAULT_TOKEN environment variable."
                                },
                                "url": {
                                    "type": "string",
                                    "description": "ExaVault host URL (e.g. http://localhost:3000). If omitted, uses EXAVAULT_URL environment variable."
                                },
                                "format": {
                                    "type": "string",
                                    "enum": ["dotenv", "json", "shell"],
                                    "description": "Output format: dotenv (default), json, or shell"
                                },
                                "write_to_file": {
                                    "type": "string",
                                    "description": "Optional file path to write secrets into (e.g. '.env' or '.env.local')"
                                }
                            }
                        }
                    },
                    {
                        "name": "exavault_pull",
                        "description": "Quick pull: downloads environment variables directly to a local .env file.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "file_path": {
                                    "type": "string",
                                    "description": "Destination file path. Defaults to '.env'"
                                },
                                "token": {
                                    "type": "string",
                                    "description": "ExaVault service token. Defaults to EXAVAULT_TOKEN env var if omitted."
                                }
                            }
                        }
                    }
                ]
            });
            Some(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: Some(tools),
                error: None,
            })
        }

        "tools/call" => {
            let params = req.params.unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));

            match name {
                "exavault_export_env" | "exavault_pull" => {
                    let token = args.get("token")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| env::var("EXAVAULT_TOKEN").ok());

                    let url = args.get("url")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| env::var("EXAVAULT_URL").ok())
                        .unwrap_or_else(|| "http://localhost:3000".to_string());

                    let format = args.get("format")
                        .and_then(|v| v.as_str())
                        .unwrap_or("dotenv");

                    let write_to_file = args.get("write_to_file")
                        .or_else(|| args.get("file_path"))
                        .and_then(|v| v.as_str())
                        .or(if name == "exavault_pull" { Some(".env") } else { None });

                    if token.is_none() {
                        return Some(JsonRpcResponse {
                            jsonrpc: "2.0",
                            id,
                            result: None,
                            error: Some(json!({
                                "code": -32602,
                                "message": "Service token required. Provide 'token' argument or set EXAVAULT_TOKEN env var."
                            })),
                        });
                    }

                    let raw_token = token.unwrap();
                    let clean_url = url.trim_end_matches('/');
                    let export_url = format!("{}/api/v1/export?format={}", clean_url, format);

                    let client = reqwest::Client::new();
                    let res = match client
                        .get(&export_url)
                        .header("Authorization", format!("Bearer {}", raw_token))
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            return Some(JsonRpcResponse {
                                jsonrpc: "2.0",
                                id,
                                result: None,
                                error: Some(json!({
                                    "code": -32000,
                                    "message": format!("Network error connecting to ExaVault: {}", e)
                                })),
                            });
                        }
                    };

                    if !res.status().is_success() {
                        let err_text = res.text().await.unwrap_or_default();
                        return Some(JsonRpcResponse {
                            jsonrpc: "2.0",
                            id,
                            result: None,
                            error: Some(json!({
                                "code": -32000,
                                "message": format!("ExaVault API returned error: {}", err_text)
                            })),
                        });
                    }

                    let content = res.text().await.unwrap_or_default();

                    let text_response = if let Some(path) = write_to_file {
                        if let Err(e) = fs::write(path, &content) {
                            format!("Failed to write to {}: {}", path, e)
                        } else {
                            format!("✅ Successfully pulled secrets from ExaVault and wrote to '{}'", path)
                        }
                    } else {
                        content
                    };

                    Some(JsonRpcResponse {
                        jsonrpc: "2.0",
                        id,
                        result: Some(json!({
                            "content": [
                                {
                                    "type": "text",
                                    "text": text_response
                                }
                            ]
                        })),
                        error: None,
                    })
                }

                _ => Some(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(json!({
                        "code": -32601,
                        "message": format!("Unknown tool: {}", name)
                    })),
                }),
            }
        }

        _ => Some(JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(json!({
                "code": -32601,
                "message": format!("Method not found: {}", req.method)
            })),
        }),
    }
}

