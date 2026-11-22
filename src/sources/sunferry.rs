//! Sun Ferry open data.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::api::{
    Client, EtaReport, Route, Sailing, TimetableReport, TimetableRow, normalize_time,
};

const BASE: &str = "https://www.sunferry.com.hk/eta";

#[derive(Debug, Deserialize)]
struct SunEta {
    #[serde(default)]
    generated_timestamp: Option<String>,
    #[serde(default)]
    data: Vec<SunSailing>,
}

#[derive(Debug, Deserialize)]
struct SunSailing {
    #[serde(default)]
    vesselcode: Option<String>,
    #[serde(default)]
    depart_time: Option<String>,
    #[serde(default)]
    eta: Option<String>,
    #[serde(default)]
    rmk_en: Option<String>,
}

/// The live API needs the route code; the timetable CSV lives under the slug.
pub const ROUTES: [(&str, &str, &str, &str, &str); 4] = [
    (
        "CECC",
        "central_cheungchau",
        "Central - Cheung Chau",
        "Central (Pier 5)",
        "Cheung Chau",
    ),
    (
        "CEMW",
        "central_muiwo",
        "Central - Mui Wo",
        "Central (Pier 6)",
        "Mui Wo",
    ),
    (
        "NPHH",
        "northpoint_hunghom",
        "North Point - Hung Hom",
        "North Point",
        "Hung Hom",
    ),
    (
        "NPKC",
        "northpoint_kowlooncity",
        "North Point - Kowloon City",
        "North Point",
        "Kowloon City",
    ),
];

pub fn routes() -> Vec<Route> {
    ROUTES
        .iter()
        .map(|(code, slug, name, origin, destination)| Route {
            operator: "sunferry".to_string(),
            id: code.to_string(),
            slug: Some(slug.to_string()),
            name: name.to_string(),
            origin: origin.to_string(),
            destination: destination.to_string(),
        })
        .collect()
}

pub async fn eta(client: &Client, route: &Route) -> Result<EtaReport> {
    let url = format!("{BASE}/?route={}", route.id);
    let body = client
        .text(&url, std::time::Duration::from_secs(30))
        .await?;
    let parsed: SunEta = serde_json::from_str(&body)
        .with_context(|| format!("decoding Sun Ferry ETA for {}", route.id))?;
    let sailings: Vec<Sailing> = parsed
        .data
        .into_iter()
        .map(|sailing| Sailing {
            depart: sailing.depart_time.unwrap_or_default(),
            eta: sailing.eta.unwrap_or_default(),
            eta_at: None,
            direction: None,
            vessel: sailing.vesselcode.filter(|code| !code.trim().is_empty()),
            remark: sailing.rmk_en.filter(|remark| !remark.trim().is_empty()),
        })
        .collect();
    Ok(EtaReport {
        operator: "sunferry".to_string(),
        route: route.name.clone(),
        route_id: route.id.clone(),
        direction: None,
        generated_at: parsed.generated_timestamp.unwrap_or_default(),
        count: sailings.len(),
        sailings,
    })
}

pub async fn timetable(
    client: &Client,
    route: &Route,
    _direction: Option<&str>,
) -> Result<TimetableReport> {
    let slug = route
        .slug
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Sun Ferry route {} has no timetable slug", route.id))?;
    let url = format!("{BASE}/timetable/SunFerry_{slug}_timetable_eng.csv");
    let body = client
        .text(&url, std::time::Duration::from_secs(6 * 3600))
        .await?;
    let rows = parse_timetable(&body)?;
    Ok(TimetableReport {
        operator: "sunferry".to_string(),
        route: route.name.clone(),
        route_id: route.id.clone(),
        direction: None,
        count: rows.len(),
        rows,
    })
}

fn parse_timetable(body: &str) -> Result<Vec<TimetableRow>> {
    let body = body.trim_start_matches('\u{feff}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(body.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        let service_day = record.get(1).unwrap_or_default().trim().to_string();
        let raw_time = record.get(2).unwrap_or_default().trim();
        if raw_time.is_empty() {
            continue;
        }
        let time = normalize_time(raw_time).unwrap_or_else(|| raw_time.to_string());
        let remark = record
            .get(3)
            .map(str::trim)
            .filter(|remark| !remark.is_empty())
            .map(ToOwned::to_owned);
        rows.push(TimetableRow {
            service_day,
            time,
            marker: None,
            remark,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sun_timetable() {
        let csv = "\u{feff}Direction,Service Date,Service Hour,Remark\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,12:30 a.m.,\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,6:10 a.m.,1\r\n";
        let rows = parse_timetable(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].time, "06:10");
        assert_eq!(rows[1].remark.as_deref(), Some("1"));
    }
}
