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
        &headers
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
            &row.iter()
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
