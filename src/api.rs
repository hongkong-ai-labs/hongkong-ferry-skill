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
}
