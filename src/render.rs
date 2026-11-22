//! Tables for the ferry CLI.

use crate::api::{EtaReport, Route, TimetableReport};

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
    }
    let mut out = String::new();
    out.push_str(
        headers
            .iter()
            .enumerate()
            .map(|(index, header)| format!("{header:<width$}", width = widths[index]))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end(),
    );
    out.push('\n');
    out.push_str(
        &widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("  "),
    );
    out.push('\n');
    for row in rows {
        out.push_str(
            row.iter()
                .enumerate()
                .map(|(index, cell)| format!("{cell:<width$}", width = widths[index]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end(),
        );
        out.push('\n');
    }
    out
}

pub fn routes(routes: &[Route]) -> String {
    let rows: Vec<Vec<String>> = routes
        .iter()
        .map(|route| {
            vec![
                route.operator.clone(),
                route.id.clone(),
                route.name.clone(),
                format!("{} -> {}", route.origin, route.destination),
            ]
        })
        .collect();
    table(&["OPERATOR", "ID", "ROUTE", "PIERS"], &rows)
        .trim_end()
        .to_string()
}

pub fn eta(report: &EtaReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} ({})",
        report.operator, report.route, report.route_id
    ));
    if let Some(direction) = &report.direction {
        out.push_str(&format!(" — {direction}"));
    }
    out.push_str(&format!(" — updated {}\n\n", report.generated_at));
    if report.sailings.is_empty() {
        out.push_str("No upcoming sailings are currently published.\n");
        out.push_str("This can happen outside service hours; check the timetable.");
        return out.trim_end().to_string();
    }
    let rows: Vec<Vec<String>> = report
        .sailings
        .iter()
        .map(|sailing| {
            vec![
                sailing.depart.clone(),
                sailing.eta.clone(),
                sailing.direction.clone().unwrap_or_else(|| "-".to_string()),
                sailing.vessel.clone().unwrap_or_else(|| "-".to_string()),
                sailing.remark.clone().unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();
    out.push_str(&table(
        &["DEPART", "ETA", "DIRECTION", "VESSEL", "REMARK"],
        &rows,
    ));
    out.trim_end().to_string()
}

pub fn timetable(report: &TimetableReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} ({})",
        report.operator, report.route, report.route_id
    ));
    if let Some(direction) = &report.direction {
        out.push_str(&format!(" — {direction}"));
    }
    out.push_str(&format!(" — {} sailings\n\n", report.count));
    if report.rows.is_empty() {
        out.push_str("No timetable rows were published.");
        return out.trim_end().to_string();
    }
    let rows: Vec<Vec<String>> = report
        .rows
        .iter()
        .map(|row| {
            vec![
                row.service_day.clone(),
                row.time.clone(),
                row.marker.clone().unwrap_or_default(),
                row.remark.clone().unwrap_or_default(),
            ]
        })
        .collect();
    out.push_str(&table(&["SERVICE DAY", "TIME", "MARK", "REMARK"], &rows));
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Sailing, TimetableRow};

    fn route() -> Route {
        Route {
            operator: "sunferry".to_string(),
            id: "CECC".to_string(),
            slug: Some("central_cheungchau".to_string()),
            name: "Central - Cheung Chau".to_string(),
            origin: "Central (Pier 5)".to_string(),
            destination: "Cheung Chau".to_string(),
        }
    }

    fn eta_report(sailings: Vec<Sailing>) -> EtaReport {
        EtaReport {
            operator: "sunferry".to_string(),
            route: "Central - Cheung Chau".to_string(),
            route_id: "CECC".to_string(),
            direction: Some("outbound".to_string()),
            generated_at: "2026-11-22 09:30".to_string(),
            count: sailings.len(),
            sailings,
        }
    }

    #[test]
    fn routes_table_lists_piers() {
        let out = routes(&[route()]);
        assert!(out.contains("sunferry"));
        assert!(out.contains("Central - Cheung Chau"));
        assert!(out.contains("Central (Pier 5) -> Cheung Chau"));
    }

    #[test]
    fn eta_without_sailings_explains_the_gap() {
        let out = eta(&eta_report(Vec::new()));
        assert!(out.contains("CECC"));
        assert!(out.contains("outbound"));
        assert!(out.contains("No upcoming sailings"));
        assert!(out.contains("check the timetable"));
    }

    #[test]
    fn eta_renders_direction_vessel_and_remark() {
        let out = eta(&eta_report(vec![Sailing {
            depart: "10:30".to_string(),
            eta: "10:35".to_string(),
            eta_at: None,
            direction: Some("outbound".to_string()),
            vessel: Some("HKKF3".to_string()),
            remark: Some("via Sok Kwu Wan".to_string()),
        }]));
        assert!(out.contains("10:30"));
        assert!(out.contains("10:35"));
        assert!(out.contains("HKKF3"));
        assert!(out.contains("via Sok Kwu Wan"));
    }

    #[test]
    fn timetable_without_rows_says_so() {
        let report = TimetableReport {
            operator: "hkkf".to_string(),
            route: "Central - Yung Shue Wan".to_string(),
            route_id: "2".to_string(),
            direction: None,
            count: 0,
            rows: Vec::new(),
        };
        let out = timetable(&report);
        assert!(out.contains("No timetable rows were published."));
    }

    #[test]
    fn timetable_renders_markers_and_remarks() {
        let report = TimetableReport {
            operator: "hkkf".to_string(),
            route: "Central - Yung Shue Wan".to_string(),
            route_id: "2".to_string(),
            direction: None,
            count: 1,
            rows: vec![TimetableRow {
                service_day: "Sunday and Public Holidays".to_string(),
                time: "00:30".to_string(),
                marker: Some("^".to_string()),
                remark: Some("Via Peng Chau".to_string()),
            }],
        };
        let out = timetable(&report);
        assert!(out.contains("Sunday and Public Holidays"));
        assert!(out.contains("00:30"));
        assert!(out.contains('^'));
        assert!(out.contains("Via Peng Chau"));
    }
}
