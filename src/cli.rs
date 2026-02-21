use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::commands;

#[derive(Debug, Parser)]
#[command(name = "MOON")]
#[command(about = "OpenClaw context optimization installer/repair CLI")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Install(InstallArgs),
    Verify(VerifyArgs),
    Repair(RepairArgs),
    PostUpgrade,
    Status,
    MoonStatus,
    MoonSnapshot(MoonSnapshotArgs),
    MoonIndex(MoonIndexArgs),
    MoonWatch(MoonWatchArgs),
    MoonRecall(MoonRecallArgs),
    MoonDistill(MoonDistillArgs),
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub apply: bool,
}

#[derive(Debug, Args, Default)]
pub struct VerifyArgs {
    #[arg(long)]
    pub strict: bool,
}

#[derive(Debug, Args, Default)]
pub struct RepairArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Default)]
pub struct MoonSnapshotArgs {
    #[arg(long)]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct MoonIndexArgs {
    #[arg(long, default_value = "history")]
    pub name: String,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub reproject: bool,
}

#[derive(Debug, Args, Default)]
pub struct MoonWatchArgs {
    #[arg(long)]
    pub once: bool,
    #[arg(long)]
    pub daemon: bool,
}

#[derive(Debug, Args)]
pub struct MoonRecallArgs {
    #[arg(long)]
    pub query: String,
    #[arg(long, default_value = "history")]
    pub name: String,
    #[arg(long)]
    pub channel_key: Option<String>,
}

#[derive(Debug, Args)]
pub struct MoonDistillArgs {
    #[arg(long)]
    pub archive: String,
    #[arg(long)]
    pub session_id: Option<String>,
    #[arg(long)]
    pub allow_large_archive: bool,
}

fn print_report(report: &commands::CommandReport, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }

    println!("command: {}", report.command);
    println!("ok: {}", report.ok);
    if !report.details.is_empty() {
        println!("details:");
        for detail in &report.details {
            println!("- {detail}");
        }
    }
    if !report.issues.is_empty() {
        println!("issues:");
        for issue in &report.issues {
            println!("- {issue}");
        }
    }
    Ok(())
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    let report = match &cli.command {
        Command::Install(args) => commands::install::run(&commands::install::InstallOptions {
            force: args.force,
            dry_run: args.dry_run,
            apply: args.apply,
        })?,
        Command::Verify(args) => commands::verify::run(&commands::verify::VerifyOptions {
            strict: args.strict,
        })?,
        Command::Repair(args) => {
            commands::repair::run(&commands::repair::RepairOptions { force: args.force })?
        }
        Command::PostUpgrade => commands::post_upgrade::run()?,
        Command::Status => commands::status::run()?,
        Command::MoonStatus => commands::moon_status::run()?,
        Command::MoonSnapshot(args) => {
            commands::moon_snapshot::run(&commands::moon_snapshot::MoonSnapshotOptions {
                source: args.source.clone(),
                dry_run: args.dry_run,
            })?
        }
        Command::MoonIndex(args) => {
            commands::moon_index::run(&commands::moon_index::MoonIndexOptions {
                collection_name: args.name.clone(),
                dry_run: args.dry_run,
                reproject: args.reproject,
            })?
        }
        Command::MoonWatch(args) => {
            commands::moon_watch::run(&commands::moon_watch::MoonWatchOptions {
                once: args.once,
                daemon: args.daemon,
            })?
        }
        Command::MoonRecall(args) => {
            commands::moon_recall::run(&commands::moon_recall::MoonRecallOptions {
                query: args.query.clone(),
                collection_name: args.name.clone(),
                channel_key: args.channel_key.clone(),
            })?
        }
        Command::MoonDistill(args) => {
            commands::moon_distill::run(&commands::moon_distill::MoonDistillOptions {
                archive_path: args.archive.clone(),
                session_id: args.session_id.clone(),
                allow_large_archive: args.allow_large_archive,
            })?
        }
    };

    print_report(&report, cli.json)?;

    if report.ok {
        Ok(())
    } else {
        std::process::exit(2);
    }
}
