//! Hong Kong & Kowloon Ferry open data.

use anyhow::{Context, Result};
use regex::Regex;
use serde::Deserialize;

use crate::api::{
    Client, EtaReport, Route, Sailing, TimetableReport, TimetableRow, normalize_time,
};

const BASE: &str = "https://www.hkkfeta.com/opendata";

#[derive(Debug, Deserialize)]
struct HkkfRoutes {
    #[serde(default)]
    data: Vec<HkkfRoute>,
}

#[derive(Debug, Deserialize)]
struct HkkfRoute {
    route_id: u32,
    #[serde(default)]
    route_name_en: String,
    #[serde(default)]
    origin_en: String,
    #[serde(default)]
    destination_en: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HkkfEta {
    #[serde(default)]
    pub(crate) generated_timestamp: String,
    #[serde(default)]
    pub(crate) data: Vec<HkkfSailing>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct HkkfSailing {
    #[serde(default)]
    pub(crate) direction: String,
    #[serde(default)]
    pub(crate) session_time: String,
    #[serde(default, rename = "ETA")]
    pub(crate) eta: String,
}

pub async fn routes(client: &Client) -> Result<Vec<Route>> {
    let body = client
        .text(
            &format!("{BASE}/route/"),
            std::time::Duration::from_secs(6 * 3600),
        )
        .await?;
    let parsed: HkkfRoutes = serde_json::from_str(&body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|route| Route {
            operator: "hkkf".to_string(),
            id: route.route_id.to_string(),
            slug: None,
            name: route.route_name_en,
            origin: route.origin_en,
            destination: route.destination_en,
        })
        .collect())
}

pub async fn eta(client: &Client, route: &Route, direction: Option<&str>) -> Result<EtaReport> {
    let direction = direction.unwrap_or("outbound");
    let url = format!("{BASE}/eta/{}/{direction}", route.id);
    let body = client
        .text(&url, std::time::Duration::from_secs(30))
        .await?;
    let parsed: HkkfEta = serde_json::from_str(&body)
        .with_context(|| format!("decoding HKKF ETA for route {}", route.id))?;
    let sailings: Vec<Sailing> = parsed
        .data
        .into_iter()
        .map(|sailing| Sailing {
            depart: sailing.session_time,
            eta: sailing.eta.clone(),
            eta_at: Some(sailing.eta),
            direction: Some(sailing.direction),
            vessel: None,
            remark: None,
        })
        .collect();
    Ok(EtaReport {
        operator: "hkkf".to_string(),
        route: route.name.clone(),
        route_id: route.id.clone(),
        direction: Some(direction.to_string()),
        generated_at: parsed.generated_timestamp,
        count: sailings.len(),
        sailings,
    })
}

#[cfg(test)]
pub(crate) fn parse_hkkf_timetable(body: &str) -> Result<Vec<TimetableRow>> {
    parse_timetable(body)
}

pub async fn timetable(
    client: &Client,
    route: &Route,
    direction: Option<&str>,
) -> Result<TimetableReport> {
    let direction = direction.unwrap_or("outbound");
    let url = format!("{BASE}/time_table/{}/{direction}", route.id);
    let body = client
        .text(&url, std::time::Duration::from_secs(6 * 3600))
        .await?;
    let rows = parse_timetable(&body)?;
    Ok(TimetableReport {
        operator: "hkkf".to_string(),
        route: route.name.clone(),
        route_id: route.id.clone(),
        direction: Some(direction.to_string()),
        count: rows.len(),
        rows,
    })
}

pub(crate) fn parse_timetable(body: &str) -> Result<Vec<TimetableRow>> {
    let body = body.trim_start_matches('\u{feff}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes());
    let time_pattern = Regex::new(r"([*#^]?)\s*(\d{1,2}:\d{2})")?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        if record.len() < 13 {
            continue;
        }
        let route_id = record.get(0).unwrap_or_default().trim();
        if route_id.is_empty() || route_id.contains("route id") {
            continue;
        }
        for (index, label) in [
            (11, "Monday to Saturday"),
            (12, "Sunday and Public Holidays"),
        ] {
            let cell = record.get(index).unwrap_or_default();
            for capture in time_pattern.captures_iter(cell) {
                let marker = capture.get(1).map(|value| value.as_str()).unwrap_or("");
                let time = capture.get(2).map(|value| value.as_str()).unwrap_or("");
                rows.push(TimetableRow {
                    service_day: label.to_string(),
                    time: normalize_time(time).unwrap_or_else(|| time.to_string()),
                    marker: if marker.is_empty() {
                        None
                    } else {
                        Some(marker.to_string())
                    },
                    remark: None,
                });
            }
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hkkf_timetable() {
        let csv = "\u{feff}route id,direction,origin_en,a,b,c,dest_en,e,f,remarks,g,h,i\r\n\
                   2,outbound,Central,a,b,c,Yung Shue Wan,d,e,note,g,\"['07:00', '*07:30', '^00:30']\",\"['08:00']\"\r\n";
        let rows = parse_timetable(csv).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].time, "07:00");
        assert_eq!(rows[1].marker.as_deref(), Some("*"));
        assert_eq!(rows[2].marker.as_deref(), Some("^"));
    }
}
