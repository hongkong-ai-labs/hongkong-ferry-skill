//! `hkferry` — Hong Kong ferry ETAs and timetables CLI and MCP server.

mod api;
mod mcp;
mod render;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

use api::{Client, Operator};

mod update;

#[derive(Parser)]
#[command(
    name = "hkferry",
    version,
    about = "Hong Kong ferry ETAs and timetables — CLI and MCP server",
    arg_required_else_help = true
)]
struct Cli {
    /// Print machine-readable JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List routes for one operator or both
    Routes {
        /// hkkf, sunferry, or all
        #[arg(long, default_value = "all")]
        operator: String,
    },
    /// Live next sailings
    #[command(alias = "next")]
    Eta {
        /// hkkf or sunferry
        operator: String,
        /// Route id, code, timetable slug or name substring
        route: String,
        /// outbound or inbound (HKKF only; default outbound)
        #[arg(long, short)]
        direction: Option<String>,
    },
    /// Published timetable
    #[command(alias = "schedule")]
    Timetable {
        /// hkkf or sunferry
        operator: String,
        /// Route id, code, timetable slug or name substring
        route: String,
        /// outbound or inbound (HKKF only; default outbound)
        #[arg(long, short)]
        direction: Option<String>,
    },
    /// Update this binary using the method it was installed with
    Update {
        /// Only report the current version, latest release and install method
        #[arg(long)]
        check: bool,
        /// Reinstall even when the current version is up to date
        #[arg(long)]
        force: bool,
    },
    /// Run the MCP server over stdio
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new()?;

    if let Command::Mcp = cli.command {
        return mcp::serve(client).await;
    }

    match cli.command {
        Command::Routes { operator } => {
            let routes = if operator.eq_ignore_ascii_case("all") {
                let mut routes = client.routes(Operator::Hkkf).await?;
                routes.extend(client.routes(Operator::SunFerry).await?);
                routes
            } else {
                client.routes(Operator::parse(&operator)?).await?
            };
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&routes)?);
            } else {
                println!("{}", render::routes(&routes));
            }
        }
        Command::Eta {
            operator,
            route,
            direction,
        } => {
            let operator = Operator::parse(&operator)?;
            let directions = ["outbound", "inbound"];
            if let Some(direction) = &direction
                && !directions.contains(&direction.as_str())
            {
                bail!("--direction must be outbound or inbound");
            }
            let routes = client.routes(operator).await?;
            let route = api::find_route(&routes, &route).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown {} route '{route}'; run `hkferry routes --operator {}`",
                    operator.label(),
                    operator.label()
                )
            })?;
            let report = client.eta(operator, route, direction.as_deref()).await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", render::eta(&report));
            }
        }
        Command::Timetable {
            operator,
            route,
            direction,
        } => {
            let operator = Operator::parse(&operator)?;
            let directions = ["outbound", "inbound"];
            if let Some(direction) = &direction
                && !directions.contains(&direction.as_str())
            {
                bail!("--direction must be outbound or inbound");
            }
            let routes = client.routes(operator).await?;
            let route = api::find_route(&routes, &route).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown {} route '{route}'; run `hkferry routes --operator {}`",
                    operator.label(),
                    operator.label()
                )
            })?;
            let report = client
                .timetable(operator, route, direction.as_deref())
                .await?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&report)?);
            } else {
                println!("{}", render::timetable(&report));
            }
        }
        Command::Update { check, force } => {
            update::run(check, force)?;
        }
        Command::Mcp => unreachable!(),
    }
    Ok(())
}
