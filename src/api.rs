//! Ferry clients for Hong Kong & Kowloon Ferry (HKKF) and Sun Ferry.
//!
//! Both operators publish keyless open data. HKKF exposes routes, live ETA and
//! a timetable CSV; Sun Ferry exposes live ETA by route code and a timetable
//! CSV per route. The two timetable formats are normalised into the same
//! [`TimetableRow`] shape.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{NaiveTime, Timelike};
use regex::Regex;
use serde::{Deserialize, Serialize};

const HKKF_BASE: &str = "https://www.hkkfeta.com/opendata";
const SUN_BASE: &str = "https://www.sunferry.com.hk/eta";

/// Operators this skill knows how to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Hkkf,
    SunFerry,
}

impl Operator {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "hkkf" | "hk-kf" | "islands" => Ok(Self::Hkkf),
            "sunferry" | "sun-ferry" | "sf" | "sun" => Ok(Self::SunFerry),
            other => bail!("unknown operator '{other}'; use hkkf or sunferry"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hkkf => "hkkf",
            Self::SunFerry => "sunferry",
        }
    }
}

/// A route advertised by one operator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub operator: String,
    /// Operator-specific route id or code, for example `2` or `CECC`.
    pub id: String,
    /// Stable timetable slug when the operator publishes one.
    pub slug: Option<String>,
    pub name: String,
    pub origin: String,
    pub destination: String,
}

/// One live sailing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sailing {
    /// Scheduled departure, `HH:MM`.
    pub depart: String,
    /// Estimated arrival or departure time as published, `HH:MM`.
    pub eta: String,
    /// Full ETA timestamp when the operator publishes one.
    pub eta_at: Option<String>,
    /// Direction (`outbound` / `inbound`) when published.
    pub direction: Option<String>,
    /// Vessel code when published.
    pub vessel: Option<String>,
    /// Operator remark, when published.
    pub remark: Option<String>,
}

/// Live ETA response, normalised across operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtaReport {
    pub operator: String,
    pub route: String,
    pub route_id: String,
    pub direction: Option<String>,
    pub generated_at: String,
    pub count: usize,
    pub sailings: Vec<Sailing>,
}

/// One published timetable row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimetableRow {
    /// Service day description as published.
    pub service_day: String,
    /// Departure time, normalised to `HH:MM`.
    pub time: String,
    /// Marker attached to the time, for example `*`, `#`, `^`.
    pub marker: Option<String>,
    /// Published footnote or remark.
    pub remark: Option<String>,
}

/// Timetable response, normalised across operators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimetableReport {
    pub operator: String,
    pub route: String,
    pub route_id: String,
    pub direction: Option<String>,
    pub count: usize,
    pub rows: Vec<TimetableRow>,
}

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
struct HkkfEta {
    #[serde(default)]
    generated_timestamp: String,
    #[serde(default)]
    data: Vec<HkkfSailing>,
}

#[derive(Debug, Deserialize)]
struct HkkfSailing {
    #[serde(default)]
    direction: String,
    #[serde(default)]
    session_time: String,
    #[serde(default, rename = "ETA")]
    eta: String,
}

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

/// Sun Ferry route registry. The live API needs the route code; the timetable
/// CSV lives under the slug.
pub const SUN_ROUTES: [(&str, &str, &str, &str, &str); 4] = [
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

fn sun_routes() -> Vec<Route> {
    SUN_ROUTES
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

/// Cached ferry client.
pub struct Client {
    http: reqwest::Client,
    cache: Mutex<HashMap<String, (Instant, String)>>,
}

impl Client {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "hongkong-ferry-skill/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/hongkong-ai-labs/hongkong-ferry-skill)"
            ))
            .timeout(Duration::from_secs(20))
            .build()?;
        Ok(Self {
            http,
            cache: Mutex::new(HashMap::new()),
        })
    }

    async fn text(&self, url: &str, ttl: Duration) -> Result<String> {
        if let Some((at, body)) = self
            .cache
            .lock()
            .expect("ferry cache poisoned")
            .get(url)
            .cloned()
            && at.elapsed() < ttl
        {
            return Ok(body);
        }
        let response = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            bail!("{url} returned HTTP {status}: {body}");
        }
        if body.trim_start().starts_with("<!DOCTYPE") || body.trim_start().starts_with("<html") {
            bail!("{url} returned an HTML page instead of ferry data");
        }
        self.cache
            .lock()
            .expect("ferry cache poisoned")
            .insert(url.to_string(), (Instant::now(), body.clone()));
        Ok(body)
    }

    pub async fn routes(&self, operator: Operator) -> Result<Vec<Route>> {
        match operator {
            Operator::SunFerry => Ok(sun_routes()),
            Operator::Hkkf => {
                let body = self
                    .text(
                        &format!("{HKKF_BASE}/route/"),
                        Duration::from_secs(6 * 3600),
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
        }
    }

    pub async fn eta(
        &self,
        operator: Operator,
        route: &Route,
        direction: Option<&str>,
    ) -> Result<EtaReport> {
        match operator {
            Operator::Hkkf => {
                let direction = direction.unwrap_or("outbound");
                let url = format!("{HKKF_BASE}/eta/{}/{direction}", route.id);
                let body = self.text(&url, Duration::from_secs(30)).await?;
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
            Operator::SunFerry => {
                let url = format!("{SUN_BASE}/?route={}", route.id);
                let body = self.text(&url, Duration::from_secs(30)).await?;
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
        }
    }

    pub async fn timetable(
        &self,
        operator: Operator,
        route: &Route,
        direction: Option<&str>,
    ) -> Result<TimetableReport> {
        match operator {
            Operator::Hkkf => {
                let direction = direction.unwrap_or("outbound");
                let url = format!("{HKKF_BASE}/time_table/{}/{direction}", route.id);
                let body = self.text(&url, Duration::from_secs(6 * 3600)).await?;
                let rows = parse_hkkf_timetable(&body)?;
                Ok(TimetableReport {
                    operator: "hkkf".to_string(),
                    route: route.name.clone(),
                    route_id: route.id.clone(),
                    direction: Some(direction.to_string()),
                    count: rows.len(),
                    rows,
                })
            }
            Operator::SunFerry => {
                let slug = route.slug.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("Sun Ferry route {} has no timetable slug", route.id)
                })?;
                let url = format!("{SUN_BASE}/timetable/SunFerry_{slug}_timetable_eng.csv");
                let body = self.text(&url, Duration::from_secs(6 * 3600)).await?;
                let rows = parse_sun_timetable(&body)?;
                Ok(TimetableReport {
                    operator: "sunferry".to_string(),
                    route: route.name.clone(),
                    route_id: route.id.clone(),
                    direction: None,
                    count: rows.len(),
                    rows,
                })
            }
        }
    }
}

/// Resolve a route by id, code, slug or name substring.
pub fn find_route<'a>(routes: &'a [Route], query: &str) -> Option<&'a Route> {
    let needle = query.trim().to_lowercase();
    routes.iter().find(|route| {
        route.id.to_lowercase() == needle
            || route
                .slug
                .as_deref()
                .is_some_and(|slug| slug.eq_ignore_ascii_case(&needle))
            || route.name.to_lowercase().contains(&needle)
    })
}

fn parse_hkkf_timetable(body: &str) -> Result<Vec<TimetableRow>> {
    let body = body.trim_start_matches('\u{feff}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes());
    let time_pattern = Regex::new(r"([*#^]?)\s*(\d{1,2}:\d{2})")?;
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        // HKKF's CSV is fixed-width: route, direction, origin, origin_tc,
        // origin_sc, destination, destination_tc, destination_sc, remarks,
        // remarks_tc, remarks_sc, Monday-Saturday times, Sunday/holiday times.
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

fn parse_sun_timetable(body: &str) -> Result<Vec<TimetableRow>> {
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

/// Convert `12:30 a.m.`, `11:45 p.m.` or `23:30` to `00:30`, `23:45`, `23:30`.
pub fn normalize_time(raw: &str) -> Option<String> {
    let cleaned = raw.trim().to_ascii_lowercase();
    for (suffix, add_twelve) in [("a.m.", false), ("p.m.", true)] {
        if let Some(clock) = cleaned.strip_suffix(suffix) {
            let parsed = NaiveTime::parse_from_str(clock.trim(), "%H:%M").ok()?;
            let hour = if add_twelve {
                if parsed.hour() == 12 {
                    12
                } else {
                    parsed.hour() + 12
                }
            } else if parsed.hour() == 12 {
                0
            } else {
                parsed.hour()
            };
            return Some(format!("{hour:02}:{:02}", parsed.minute()));
        }
    }
    let parsed = NaiveTime::parse_from_str(cleaned.trim(), "%H:%M").ok()?;
    Some(format!("{:02}:{:02}", parsed.hour(), parsed.minute()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_meridiem_times() {
        assert_eq!(normalize_time("12:30 a.m.").as_deref(), Some("00:30"));
        assert_eq!(normalize_time("11:45 p.m.").as_deref(), Some("23:45"));
        assert_eq!(normalize_time("12:15 p.m.").as_deref(), Some("12:15"));
        assert_eq!(normalize_time("7:00 a.m.").as_deref(), Some("07:00"));
        assert_eq!(normalize_time("23:30").as_deref(), Some("23:30"));
    }

    #[test]
    fn parses_sun_timetable() {
        let csv = "\u{feff}Direction,Service Date,Service Hour,Remark\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,12:30 a.m.,\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,6:10 a.m.,1\r\n";
        let rows = parse_sun_timetable(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].time, "06:10");
        assert_eq!(rows[1].remark.as_deref(), Some("1"));
    }

    #[test]
    fn parses_hkkf_timetable() {
        let csv = "\u{feff}route id,direction,origin_en,a,b,c,dest_en,e,f,remarks,g,h,i\r\n\
                   2,outbound,Central,a,b,c,Yung Shue Wan,d,e,note,g,\"['07:00', '*07:30', '^00:30']\",\"['08:00']\"\r\n";
        let rows = parse_hkkf_timetable(csv).unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].time, "07:00");
        assert_eq!(rows[1].marker.as_deref(), Some("*"));
        assert_eq!(rows[2].marker.as_deref(), Some("^"));
    }

    #[test]
    fn operator_aliases_and_labels() {
        for value in ["hkkf", "HK-KF", "islands"] {
            assert_eq!(Operator::parse(value).unwrap(), Operator::Hkkf);
            assert_eq!(Operator::parse(value).unwrap().label(), "hkkf");
        }
        for value in ["sunferry", "Sun-Ferry", "sf", "SUN"] {
            assert_eq!(Operator::parse(value).unwrap(), Operator::SunFerry);
            assert_eq!(Operator::parse(value).unwrap().label(), "sunferry");
        }

        let error = Operator::parse("star").unwrap_err().to_string();
        assert!(error.contains("hkkf or sunferry"), "{error}");
    }

    #[test]
    fn find_route_matches_id_slug_and_name() {
        let routes = sun_routes();

        assert_eq!(find_route(&routes, "cecc").unwrap().id, "CECC");
        assert_eq!(
            find_route(&routes, "central_cheungchau").unwrap().id,
            "CECC"
        );
        assert_eq!(find_route(&routes, "CHEUNG CHAU").unwrap().id, "CECC");
        assert_eq!(find_route(&routes, "  npkc ").unwrap().id, "NPKC");
        assert!(find_route(&routes, "macau").is_none());
    }

    #[test]
    fn sun_registry_is_complete() {
        let routes = sun_routes();
        assert_eq!(routes.len(), 4);
        assert!(
            routes
                .iter()
                .all(|route| route.operator == "sunferry" && route.slug.is_some())
        );
        for code in ["CECC", "CEMW", "NPHH", "NPKC"] {
            assert!(find_route(&routes, code).is_some(), "missing {code}");
        }
    }

    #[test]
    fn rejects_unparseable_times() {
        assert_eq!(normalize_time("25:00"), None);
        assert_eq!(normalize_time("24:00"), None);
        assert_eq!(normalize_time("7pm"), None);
        assert_eq!(normalize_time("noon"), None);
        assert_eq!(normalize_time(""), None);
    }

    #[test]
    fn hkkf_timetable_skips_header_and_short_rows() {
        let csv = "\u{feff}route id,direction,origin_en,a,b,c,dest_en,e,f,remarks,g,h,i\r\n\
                   2,outbound,Central,a,b,c,Yung Shue Wan,d,e,note,g,,\r\n\
                   2,outbound,Central,a,b,c,Yung Shue Wan,d,e,note,g\r\n";
        let rows = parse_hkkf_timetable(csv).unwrap();
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn sun_timetable_skips_rows_without_a_time() {
        let csv = "\u{feff}Direction,Service Date,Service Hour,Remark\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,,\r\n\
                   Central to Cheung Chau,Saturdays,7:15 a.m.,\r\n";
        let rows = parse_sun_timetable(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].time, "07:15");
        assert_eq!(rows[0].service_day, "Saturdays");
        assert_eq!(rows[0].remark, None);
    }

    #[test]
    fn hkkf_eta_payload_deserializes() {
        let payload: HkkfEta = serde_json::from_str(
            r#"{"generated_timestamp":"2026-11-22 09:30:00","data":[
                {"direction":"outbound","session_time":"09:30","ETA":"09:35"},
                {"direction":"inbound","session_time":"10:00","ETA":"10:00"}]}"#,
        )
        .unwrap();

        assert_eq!(payload.generated_timestamp, "2026-11-22 09:30:00");
        assert_eq!(payload.data.len(), 2);
        assert_eq!(payload.data[0].session_time, "09:30");
        assert_eq!(payload.data[0].eta, "09:35");
        assert_eq!(payload.data[1].direction, "inbound");
    }

    #[test]
    fn sun_eta_payload_deserializes_without_optional_fields() {
        let payload: SunEta = serde_json::from_str(
            r#"{"generated_timestamp":null,"data":[
                {"vesselcode":"","depart_time":"10:30","eta":"10:35","rmk_en":" "},
                {"depart_time":null,"eta":null}]}"#,
        )
        .unwrap();

        assert!(payload.generated_timestamp.is_none());
        assert_eq!(payload.data.len(), 2);
        assert_eq!(payload.data[0].depart_time.as_deref(), Some("10:30"));
        assert_eq!(payload.data[0].vesselcode.as_deref(), Some(""));
        assert!(payload.data[1].depart_time.is_none());
        assert!(payload.data[1].eta.is_none());
    }
}
