#![recursion_limit = "1024"]

extern crate rusqlite;
extern crate clap;

#[macro_use]
extern crate log;
extern crate env_logger;

#[macro_use]
extern crate error_chain;

mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain!{}
}

use rusqlite::Connection;
use clap::{Arg, App, SubCommand};
use errors::*;
use std::path::Path;

fn main() {

    env_logger::init().unwrap();
    trace!("Starting");

    if let Err(ref e) = run() {
        use ::std::io::Write;
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        // The backtrace is not always generated. Try to run this example
        // with `RUST_BACKTRACE=1`.
        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }

    trace!("Complete");
}

fn initialise_db(conn: &Connection) -> Result<()> {
    conn.execute_batch("begin; create table schema_versions(version integer, installed_at date \
                        default (datetime('now'))); insert into schema_versions(version) values \
                        (0); commit;")
        .chain_err(|| "creating schema_versions")?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<u32> {
    let v: std::result::Result<u32, rusqlite::Error> =
        conn.query_row("SELECT MAX(version) FROM schema_versions",
                       &[],
                       |row| row.get(0));
    let current_version = match v {
        Err(_) => {
            initialise_db(conn).chain_err(|| "initialising database")?;
            0
        }
        Ok(v) => v,
    };

    if current_version < 1 {
        conn.execute_batch("
           begin;
           create table events(id integer, name text, delta integer, log text);
           insert into schema_versions(version) values (1);
           commit;
        ");
    }

    Ok(1)
}

fn add(name: )

fn run() -> Result<()> {
    let app = App::new("Option")
        .version("0.1.0")
        .author("Alister Lee <dev@shortepic.com>")
        .about("Help optimise choices based on historic preferences.")
        .arg(Arg::with_name("dbname")
            .short("d")
            .long("dbname")
            .value_name("FILE")
            .help("Overrides default database filename")
            .takes_value(true)
            .default_value("options.db"))
        .arg(Arg::with_name("v")
            .short("v")
            .multiple(true)
            .help("Sets the level of verbosity"))
        .subcommand(SubCommand::with_name("add")
            .about("add an option")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option to add to list")))
        .subcommand(SubCommand::with_name("kill")
            .about("remove an option")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option to remove from list")))
        .subcommand(SubCommand::with_name("default")
            .about("set the default weight for new options")
            .arg(Arg::with_name("WEIGHT")
                .required(true)
                .help("weight for new options added to list")))
        .subcommand(SubCommand::with_name("reset")
            .about("reset all the weights to WEIGHT or default")
            .arg(Arg::with_name("weight").help("weight to reset all options to")))
        .subcommand(SubCommand::with_name("list").about("show all defined options"))
        .subcommand(SubCommand::with_name("choose")
            .about("randomly choose between options with bias from weights"))
        .subcommand(SubCommand::with_name("log")
            .about("add a log entry for a choice, without changing weights")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option add log message for"))
            .arg(Arg::with_name("MESSAGE")
                .required(true)
                .help("message for log entry")))
        .subcommand(SubCommand::with_name("like")
            .about("declare experience with option entry as favourable, increasing bias for \
                    future choices")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option indicate favour"))
            .arg(Arg::with_name("message").help("message for log entry")))
        .subcommand(SubCommand::with_name("hate")
            .about("declare experience with option entry as unfavourable, decreasing bias for \
                    future choices")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option indicate unfavour"))
            .arg(Arg::with_name("message").help("message for log entry")))
        .subcommand(SubCommand::with_name("set")
            .about("set bias for entry to absolute weight")
            .arg(Arg::with_name("NAME")
                .required(true)
                .help("name of option indicate unfavour"))
            .arg(Arg::with_name("WEIGHT")
                .required(true)
                .help("weight to set for named option"))
            .arg(Arg::with_name("message").help("message for log entry")));
    let cmdline = app.get_matches();

    let conn = Connection::open(Path::new(cmdline.value_of("dbname").unwrap())).chain_err(|| "unable to open sqlite db")?;

    let version: u32 = migrate(&conn).chain_err(|| "here")?;
    info!("version: {}", version);

    match cmdline.subcommand() {
        ("add", Some(sub_m)) => add(sub_m.value_of("name").unwrap()).chain_err(|| "adding option")?,
        ("kill", Some(sub_m)) => {}
        ("default", Some(sub_m)) => {}
        ("reset", Some(sub_m)) => {}
        ("list", Some(sub_m)) => {}
        ("choose", Some(sub_m)) => {}
        ("log", Some(sub_m)) => {}
        ("like", Some(sub_m)) => {}
        ("hate", Some(sub_m)) => {}
        ("set", Some(sub_m)) => {}
        _ => app.print_help().chain_err(|| "printing usage")?,
    }

    Ok(())
}
