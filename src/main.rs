//! vmadm compatible jail manager

#![deny(trivial_numeric_casts,
        missing_docs,
        unstable_features,
        unused_import_braces,
)]

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

#[macro_use]
extern crate clap;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate toml;
//extern crate indicatif;

extern crate uuid;
use uuid::Uuid;

#[macro_use]
extern crate slog;
extern crate slog_term;
extern crate slog_async;
#[macro_use]
extern crate slog_scope;
extern crate slog_bunyan;
use slog::Drain;

use std::result;
use std::error::Error;
use std::io;
use std::fs::OpenOptions;

use std::process::Command;

mod zfs;

pub mod jails;

pub mod jail_config;

pub mod jdb;
use jdb::JDB;

mod config;
use config::Config;

pub mod errors;
use errors::{GenericError, NotFoundError};


#[cfg(target_os = "freebsd")]
static JEXEC: &'static str = "jexec";
#[cfg(not(target_os = "freebsd"))]
static JEXEC: &'static str = "echo";


/// Custom Drain logic
struct RuntimeLevelFilter<D> {
    drain: D,
    level: u64,
}

/// Drain to define log leve via `-v` flags
impl<D> Drain for RuntimeLevelFilter<D>
where
    D: Drain,
{
    type Ok = Option<D::Ok>;
    type Err = Option<D::Err>;

    fn log(
        &self,
        record: &slog::Record,
        values: &slog::OwnedKVList,
    ) -> result::Result<Self::Ok, Self::Err> {
        let current_level = match self.level {
            0 => return Ok(None),
            1 => slog::Level::Critical,
            2 => slog::Level::Error,
            3 => slog::Level::Warning,
            4 => slog::Level::Info,
            5 => slog::Level::Debug,
            _ => slog::Level::Trace,
        };
        if record.level().is_at_least(current_level) {
            self.drain.log(record, values).map(Some).map_err(Some)
        } else {
            Ok(None)
        }
    }
}
/// Main function
#[cfg(target_os = "freebsd")]
fn main() {
    let exit_code = run();
    std::process::exit(exit_code)
}

#[cfg(not(target_os = "freebsd"))]
fn main() {
    println!("Jails are not supported, running in dummy mode");
    let exit_code = run();
    std::process::exit(exit_code)
}

fn run() -> i32 {
    use clap::App;
    let yaml = load_yaml!("cli.yml");
    let mut help_app = App::from_yaml(yaml).version(crate_version!());
    let matches = App::from_yaml(yaml).version(crate_version!()).get_matches();

    /// console logger
    let decorator = slog_term::TermDecorator::new().build();
    let term_drain = slog_term::FullFormat::new(decorator).build().fuse();
    let level = matches.occurrences_of("verbose");
    let term_drain = RuntimeLevelFilter {
        drain: term_drain,
        level: level,
    }.fuse();
    let term_drain = slog_async::Async::new(term_drain).build().fuse();

    /// fiel logger
    let log_path = "/var/log/vmadm.log";
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(true)
        .open(log_path)
        .unwrap();

    // create logger
    let file_drain = slog_bunyan::default(file).map(slog::Fuse);
    let file_drain = slog_async::Async::new(file_drain).build().fuse();

    let drain = slog::Duplicate::new(file_drain, term_drain).fuse();

    let root = slog::Logger::root(
        drain,
        o!("req_id" => Uuid::new_v4().hyphenated().to_string()),
    );

    let _guard = slog_scope::set_global_logger(root);

    let config: Config = Config::new().unwrap();
    let r = if matches.is_present("startup") {
        match matches.subcommand() {
            ("", None) => startup(&config),
            _ => Err(GenericError::bx("Can not use startup with a subcommand")),
        }
    } else {
        match matches.subcommand() {
            ("list", Some(list_matches)) => list(&config, list_matches),
            ("create", Some(create_matches)) => create(&config, create_matches),
            ("update", Some(update_matches)) => update(&config, update_matches),
            ("delete", Some(delete_matches)) => delete(&config, delete_matches),
            ("start", Some(start_matches)) => start(&config, start_matches),
            ("reboot", Some(reboot_matches)) => reboot(&config, reboot_matches),
            ("stop", Some(stop_matches)) => stop(&config, stop_matches),
            ("get", Some(get_matches)) => get(&config, get_matches),
            ("console", Some(console_matches)) => console(&config, console_matches),
            ("", None) => {
                help_app.print_help().unwrap();
                println!();
                Ok(0)
            }
            _ => unreachable!(),
        }
    };
    debug!("Execution done");
    match r {
        Ok(0) => 0,
        Ok(exit_code) => exit_code,
        Err(e) => {
            println!("command failed: {}", e);
            crit!("error: {}", e);
            1
        }
    }
}

fn startup(_conf: &Config) -> Result<i32, Box<Error>> {
    Ok(0)
}

fn start(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("Starting jail {}", uuid);
    match db.get(uuid.as_str()) {
        Err(e) => Err(e),
        Ok(jdb::Jail { os: Some(_), .. }) => {
            println!("The vm is alredy started");
            Err(NotFoundError::bx("VM is already started"))
        }
        Ok(jail) => {
            println!("Starting jail {}", jail.idx.uuid);
            jails::start(&jail)
        }
    }
}

fn reboot(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("deleteing jail {}", uuid);
    match db.get(uuid.as_str()) {
        Err(e) => Err(e),
        Ok(jdb::Jail { os: None, .. }) => {
            println!("The vm is not running");
            Err(NotFoundError::bx("The vm is not running"))
        }
        Ok(jail) => {
            println!("Rebooting jail {}", uuid);
            jails::stop(&jail)?;
            jails::start(&jail)
        }
    }
}

fn get(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("Starting jail {}", uuid);
    match db.get(uuid.as_str()) {
        Err(e) => Err(e),
        Ok(jdb::Jail { config: conf, .. }) => {
            let j = serde_json::to_string_pretty(&conf)?;
            println!("{}\n", j);
            Ok(0)
        }
    }
}

fn console(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("Starting jail {}", uuid);
    match db.get(uuid.as_str()) {
        Err(e) => Err(e),
        Ok(jdb::Jail { os: None, .. }) => {
            println!("The vm is not running");
            Err(NotFoundError::bx("VM is not running"))
        }
        Ok(jdb::Jail { os: Some(jid), .. }) => {
            let mut child = Command::new(JEXEC)
                .args(&[jid.id.to_string().as_str(), "/bin/csh"])
                .spawn()
                .expect("failed to execute jsj");
            let ecode = child.wait().expect("failed to wait on child");
            if ecode.success() {
                Ok(0)
            } else {
                Err(GenericError::bx("Failed to execute jail console"))
            }
        }
    }
}

fn stop(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("deleteing jail {}", uuid);
    match db.get(uuid.as_str()) {
        Err(e) => Err(e),
        Ok(jdb::Jail { os: None, .. }) => {
            println!("The vm is alredy stopped");
            Err(NotFoundError::bx("VM is already stooped"))
        }
        Ok(jail) => {
            println!("Stopping jail {}", uuid);
            jails::stop(&jail)
        }
    }
}

fn update(_conf: &Config, _matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    Ok(0)
}

fn list(conf: &Config, _matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let db = JDB::open(conf)?;
    db.print()
}

fn create(conf: &Config, _matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let mut db = JDB::open(conf)?;
    let jail = jail_config::JailConfig::from_reader(io::stdin())?;
    let mut dataset = conf.settings.pool.clone();
    dataset.push('/');
    dataset.push_str(jail.image_uuid.clone().as_str());
    let entry = db.insert(jail)?;
    let snap = zfs::snapshot(dataset.as_str(), entry.uuid.as_str())?;
    zfs::clone(snap.as_str(), entry.root.as_str())?;
    println!("Created jail {}", entry.uuid);
    Ok(0)
}

fn delete(conf: &Config, matches: &clap::ArgMatches) -> Result<i32, Box<Error>> {
    let mut db = JDB::open(conf)?;
    let uuid = value_t!(matches, "uuid", String).unwrap();
    debug!("deleteing jail {}", uuid);
    let res = match db.get(uuid.as_str()) {
        Ok(jail) => {
            if jail.os.is_some() {
                println!("Stopping jail {}", uuid);
                jails::stop(&jail)?;
            };
            let origin = zfs::origin(jail.idx.root.as_str());
            match zfs::destroy(jail.idx.root.as_str()) {
                Ok(_) => debug!("zfs dataset deleted: {}", jail.idx.root),
                Err(e) => warn!("failed to delete dataset: {}", e),
            };
            match origin {
                Ok(origin) => {
                    zfs::destroy(origin.as_str())?;
                    debug!("zfs snapshot deleted: {}", origin)
                }
                Err(e) => warn!("failed to delete origin: {}", e),
            };
            println!("deleted jail {}", uuid);
            Ok(0)
        }
        Err(e) => Err(e),
    };
    db.remove(uuid.as_str())?;
    res
}
