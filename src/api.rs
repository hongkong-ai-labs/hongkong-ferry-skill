//! Shared ferry models and operator orchestration.
//!
//! HKKF and Sun Ferry each live in `src/sources/` behind a Cargo feature. Both
//! remain keyless open-data feeds.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use chrono::{NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

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
    pub depart: String,
    pub eta: String,
    pub eta_at: Option<String>,
    pub direction: Option<String>,
    pub vessel: Option<String>,
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
    pub service_day: String,
    pub time: String,
    pub marker: Option<String>,
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

    // Used by the compiled-in operator modules.
    #[allow(dead_code)]
    pub(crate) async fn text(&self, url: &str, ttl: Duration) -> Result<String> {
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
            Operator::Hkkf => {
                #[cfg(feature = "source-hkkf")]
                {
                    return crate::sources::hkkf::routes(self).await;
                }
                #[cfg(not(feature = "source-hkkf"))]
                bail!("HKKF source is not compiled into this binary")
            }
            Operator::SunFerry => {
                #[cfg(feature = "source-sunferry")]
                {
                    Ok(crate::sources::sunferry::routes())
                }
                #[cfg(not(feature = "source-sunferry"))]
                bail!("Sun Ferry source is not compiled into this binary")
            }
        }
    }

    /// Every compiled-in operator, skipping one that fails at runtime.
    pub async fn all_routes(&self) -> Result<Vec<Route>> {
        #[allow(unused_mut)]
        let mut routes = Vec::new();
        #[cfg(feature = "source-hkkf")]
        match crate::sources::hkkf::routes(self).await {
            Ok(mut value) => routes.append(&mut value),
            Err(error) => eprintln!("warning: hkkf: {error:#}"),
        }
        #[cfg(feature = "source-sunferry")]
        routes.extend(crate::sources::sunferry::routes());
        Ok(routes)
    }

    pub async fn eta(
        &self,
        operator: Operator,
        route: &Route,
        direction: Option<&str>,
    ) -> Result<EtaReport> {
        match operator {
            Operator::Hkkf => {
                #[cfg(feature = "source-hkkf")]
                {
                    return crate::sources::hkkf::eta(self, route, direction).await;
                }
                #[cfg(not(feature = "source-hkkf"))]
                {
                    let _ = (route, direction);
                    bail!("HKKF source is not compiled into this binary")
                }
            }
            Operator::SunFerry => {
                #[cfg(feature = "source-sunferry")]
                {
                    let _ = direction;
                    return crate::sources::sunferry::eta(self, route).await;
                }
                #[cfg(not(feature = "source-sunferry"))]
                {
                    let _ = (route, direction);
                    bail!("Sun Ferry source is not compiled into this binary")
                }
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
                #[cfg(feature = "source-hkkf")]
                {
                    return crate::sources::hkkf::timetable(self, route, direction).await;
                }
                #[cfg(not(feature = "source-hkkf"))]
                {
                    let _ = (route, direction);
                    bail!("HKKF source is not compiled into this binary")
                }
            }
            Operator::SunFerry => {
                #[cfg(feature = "source-sunferry")]
                {
                    return crate::sources::sunferry::timetable(self, route, direction).await;
                }
                #[cfg(not(feature = "source-sunferry"))]
                {
                    let _ = (route, direction);
                    bail!("Sun Ferry source is not compiled into this binary")
                }
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

/// Convert `12:30 a.m.`, `11:45 p.m.` or `23:30` to `00:30`, `23:45`, `23:30`.
#[allow(dead_code)]
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
        let rows = crate::sources::sunferry::parse_sun_timetable(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].time, "06:10");
        assert_eq!(rows[1].remark.as_deref(), Some("1"));
    }

    #[test]
    fn parses_hkkf_timetable() {
        let csv = "\u{feff}route id,direction,origin_en,a,b,c,dest_en,e,f,remarks,g,h,i\r\n\
                   2,outbound,Central,a,b,c,Yung Shue Wan,d,e,note,g,\"['07:00', '*07:30', '^00:30']\",\"['08:00']\"\r\n";
        let rows = crate::sources::hkkf::parse_hkkf_timetable(csv).unwrap();
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
        let routes = crate::sources::sunferry::sun_routes();

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
        let routes = crate::sources::sunferry::sun_routes();
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
        let rows = crate::sources::hkkf::parse_hkkf_timetable(csv).unwrap();
        assert!(rows.is_empty(), "{rows:?}");
    }

    #[test]
    fn sun_timetable_skips_rows_without_a_time() {
        let csv = "\u{feff}Direction,Service Date,Service Hour,Remark\r\n\
                   Central to Cheung Chau,Mondays to Fridays except public holidays,,\r\n\
                   Central to Cheung Chau,Saturdays,7:15 a.m.,\r\n";
        let rows = crate::sources::sunferry::parse_sun_timetable(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].time, "07:15");
        assert_eq!(rows[0].service_day, "Saturdays");
        assert_eq!(rows[0].remark, None);
    }

    #[test]
    fn hkkf_eta_payload_deserializes() {
        let payload: crate::sources::hkkf::HkkfEta = serde_json::from_str(
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
        let payload: crate::sources::sunferry::SunEta = serde_json::from_str(
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
