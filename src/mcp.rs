//! MCP over stdio for ferry routes, ETAs and timetables.

use std::io::{BufRead, Write};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::api::{self, Client, Operator};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: [&str; 2] = ["2025-06-18", "2024-11-05"];

pub async fn serve(client: Client) -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    eprintln!("hkferry MCP server listening on stdio");
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(value) => dispatch(&client, &value).await,
            Err(error) => Some(error_response(
                None,
                -32700,
                &format!("parse error: {error}"),
            )),
        };
        if let Some(response) = response {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

async fn dispatch(client: &Client, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = match request.get("method").and_then(Value::as_str) {
        Some(method) => method,
        None => {
            return Some(error_response(
                id,
                -32600,
                "invalid request: missing method",
            ));
        }
    };
    if method.starts_with("notifications/") {
        return None;
    }
    match method {
        "initialize" => Some(success(id, initialize(request.get("params")))),
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, tools())),
        "tools/call" => Some(call(client, id, request.get("params")).await),
        other => Some(error_response(
            id,
            -32601,
            &format!("method not found: {other}"),
        )),
    }
}

fn initialize(params: Option<&Value>) -> Value {
    let requested = params
        .and_then(|value| value.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = requested
        .filter(|value| SUPPORTED_VERSIONS.contains(value))
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "hongkong-ferry-skill",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

fn tools() -> Value {
    json!({ "tools": [
        {
            "name": "list_ferry_routes",
            "description": "List Hong Kong ferry routes for HKKF, Sun Ferry, or both.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operator": {"type": "string", "enum": ["all", "hkkf", "sunferry"], "description": "Operator filter. Default all."}
                }
            }
        },
        {
            "name": "get_ferry_eta",
            "description": "Next sailings for one Hong Kong ferry route.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operator": {"type": "string", "enum": ["hkkf", "sunferry"]},
                    "route": {"type": "string", "description": "Route id, code, slug or name substring, e.g. '2', 'CECC', 'Cheung Chau'."},
                    "direction": {"type": "string", "enum": ["outbound", "inbound"], "description": "HKKF direction. Default outbound."}
                },
                "required": ["operator", "route"]
            }
        },
        {
            "name": "get_ferry_timetable",
            "description": "Published timetable for one Hong Kong ferry route.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "operator": {"type": "string", "enum": ["hkkf", "sunferry"]},
                    "route": {"type": "string", "description": "Route id, code, slug or name substring."},
                    "direction": {"type": "string", "enum": ["outbound", "inbound"], "description": "HKKF direction. Default outbound."}
                },
                "required": ["operator", "route"]
            }
        }
    ]})
}

async fn call(client: &Client, id: Option<Value>, params: Option<&Value>) -> Value {
    let Some(params) = params else {
        return error_response(id, -32602, "missing params");
    };
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome: Result<Value> = async {
        match name {
            "list_ferry_routes" => {
                let operator = args
                    .get("operator")
                    .and_then(Value::as_str)
                    .unwrap_or("all");
                let routes = if operator.eq_ignore_ascii_case("all") {
                    let mut routes = client.routes(Operator::Hkkf).await?;
                    routes.extend(client.routes(Operator::SunFerry).await?);
                    routes
                } else {
                    client.routes(Operator::parse(operator)?).await?
                };
                Ok(serde_json::to_value(routes)?)
            }
            "get_ferry_eta" => {
                let operator = required_operator(&args)?;
                let query = args
                    .get("route")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("route is required"))?;
                let direction = optional_direction(&args)?;
                let routes = client.routes(operator).await?;
                let route = api::find_route(&routes, query)
                    .ok_or_else(|| anyhow::anyhow!("unknown route '{query}'"))?;
                let report = client.eta(operator, route, direction.as_deref()).await?;
                Ok(serde_json::to_value(report)?)
            }
            "get_ferry_timetable" => {
                let operator = required_operator(&args)?;
                let query = args
                    .get("route")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow::anyhow!("route is required"))?;
                let direction = optional_direction(&args)?;
                let routes = client.routes(operator).await?;
                let route = api::find_route(&routes, query)
                    .ok_or_else(|| anyhow::anyhow!("unknown route '{query}'"))?;
                let report = client
                    .timetable(operator, route, direction.as_deref())
                    .await?;
                Ok(serde_json::to_value(report)?)
            }
            other => bail!("unknown tool: {other}"),
        }
    }
    .await;
    match outcome {
        Ok(value) => success(
            id,
            json!({
                "content": [{
                    "type": "text",
                    "text": serde_json::to_string_pretty(&value).unwrap_or_default()
                }],
                "structuredContent": value,
                "isError": false
            }),
        ),
        Err(error) => success(
            id,
            json!({
                "content": [{"type": "text", "text": format!("{error:#}")}],
                "isError": true
            }),
        ),
    }
}

fn required_operator(args: &Value) -> Result<Operator> {
    let operator = args
        .get("operator")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("operator is required"))?;
    Operator::parse(operator)
}

fn optional_direction(args: &Value) -> Result<Option<String>> {
    let direction = args
        .get("direction")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(value) = &direction
        && !matches!(value.as_str(), "outbound" | "inbound")
    {
        bail!("direction must be outbound or inbound");
    }
    Ok(direction)
}

fn success(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
