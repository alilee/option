#![recursion_limit = "1024"]

extern crate rusqlite;
extern crate clap;
extern crate rand;

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
use rand::distributions::{Weighted, WeightedChoice, IndependentSample};

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
                        default (date('now','localtime'))); insert into schema_versions(version) \
                        values (0); commit;")
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
           create table events(name text, \
               delta integer, \
               log text, \
               created_at timestamp default (datetime('now','localtime')));
           insert into schema_versions(version) values (1);
           commit;
        ")
            .chain_err(|| "migrating schema version 1")?;
    }

    if current_version < 2 {
        conn.execute_batch("
           begin;
           create table settings(name text, value integer);
           insert into settings(name, value) VALUES ('default_weight', 5);
           insert into schema_versions(version) values (2);
           commit;
        ")
            .chain_err(|| "migrating schema version 2")?;
    }

    Ok(2)
}

fn setting(conn: &Connection, name: &str, default: u32) -> u32 {
    let x: u32 = conn.query_row("SELECT value FROM events WHERE name=?",
                   &[&name],
                   |row| row.get(0))
        .unwrap_or(default);
    x
}

fn add(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("insert into events(name, delta)
                  select ?, value from settings where name = 'default_weight';",
                 &[&name])
        .chain_err(|| "inserting name")?;
    trace!("inserted {}", name);
    Ok(())
}

fn kill(conn: &Connection, name: &str) -> Result<()> {
    let x: i32 = conn.query_row("SELECT sum(ifnull(delta,0)) FROM events WHERE name=?",
                   &[&name],
                   |row| row.get(0))
        .unwrap();
    trace!("{:?}", x);

    conn.execute("INSERT INTO events(name, delta) VALUES (?,?)",
                 &[&name, &(0 - x)])
        .chain_err(|| "inserting negative total delta into name")?;
    trace!("killed {}", name);
    Ok(())
}

fn default(conn: &Connection, weight: &u32) -> Result<()> {
    conn.execute("update settings set value = ?
                  where name = 'default_weight'",
                 &[weight])
        .chain_err(|| "updating weight setting")?;
    trace!("updated default weight to {}", weight);
    Ok(())
}

fn reset(conn: &Connection, weight: &u32) -> Result<()> {
    conn.execute("INSERT INTO events(name, delta, log)
                  SELECT name, ?-sum(ifnull(delta,0)), 'Reset'
                  FROM events
                  GROUP BY name
                  HAVING sum(ifnull(delta,0)) > 0",
                 &[weight])
        .chain_err(|| "resetting all weights")?;
    trace!("resetting all non-zero weights to {}", weight);
    Ok(())
}

fn list(conn: &Connection) -> Result<()> {

    trace!("listing");

    let mut stmt = conn.prepare("
        SELECT name, sum(ifnull(delta,0))
        FROM events
        GROUP \
                  BY name
        HAVING sum(ifnull(delta,0)) > 0
        ORDER BY name
    ")
        .unwrap();

    let choice_iter = stmt.query_map(&[], |row| {
            let name: String = row.get(0);
            let val: u32 = row.get(1);
            (name, val)
        })
        .unwrap();

    for choice in choice_iter {
        let c = choice.unwrap();
        println!("{}: {}", c.0, c.1);
    }
    Ok(())
}

fn choose(conn: &Connection) -> Result<()> {

    trace!("choosing");

    let mut stmt = conn.prepare("
        SELECT name, sum(ifnull(delta,0))
        FROM events
        GROUP \
                  BY name
        HAVING sum(ifnull(delta,0)) > 0
        ORDER BY name
    ")
        .unwrap();

    let mut choices: Vec<Weighted<String>> = stmt.query_map(&[], |row| {
            Weighted::<String> {
                weight: row.get(1),
                item: row.get(0),
            }
        })
        .unwrap()
        .map(|x| x.unwrap())
        .collect();

    let wc = WeightedChoice::new(&mut choices);
    let mut rng = rand::thread_rng();
    println!("selected: {}", wc.ind_sample(&mut rng));

    Ok(())
}

fn log(conn: &Connection, name: &str, message: Option<&str>, delta: i32) -> Result<()> {
    conn.execute("insert into events(name, log, delta)
                  values (?, ?, ?)",
                 &[&name, &message.unwrap_or(""), &delta])
        .chain_err(|| "adding a log")?;
    trace!("inserted an event for {}", name);
    Ok(())
}

fn set(conn: &Connection, name: &str, message: Option<&str>, weight: u32) -> Result<()> {
    conn.execute("INSERT INTO events(name, delta, log)
                  SELECT name, ?-sum(ifnull(delta,0)), ?
                  FROM events
                  WHERE name = ?
                  GROUP BY name
                  HAVING sum(ifnull(delta,0)) > 0",
                 &[&weight, &message.unwrap_or(""), &name])
        .chain_err(|| "resetting all weights")?;
    trace!("resetting weight for {}", name);
    Ok(())
}


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
    trace!("{:?}", cmdline.subcommand());

    match cmdline.subcommand() {
        ("add", Some(sub_m)) => {
            add(&conn, sub_m.value_of("NAME").unwrap()).chain_err(|| "adding option")?;
            Ok(())
        }
        ("kill", Some(sub_m)) => {
            kill(&conn, sub_m.value_of("NAME").unwrap()).chain_err(|| "killing option")?;
            Ok(())
        }
        ("default", Some(sub_m)) => {
            let weight = u32::from_str_radix(sub_m.value_of("WEIGHT").unwrap(),10).chain_err(|| "converting default weight")?;
            default(&conn, &weight).chain_err(|| "setting default weight")?;
            Ok(())
        }
        ("reset", Some(sub_m)) => {
            let weight = match sub_m.value_of("weight") {
                None => setting(&conn, "default_weight", 5),
                Some(w) => u32::from_str_radix(w, 10).chain_err(|| "converting reset value")?,
            };
            reset(&conn, &weight).chain_err(|| "resetting to weight")?;
            Ok(())
        }
        ("list", Some(_)) => {
            list(&conn).chain_err(|| "listing")?;
            Ok(())
        }
        ("choose", Some(_)) => {
            choose(&conn).chain_err(|| "choosing")?;
            Ok(())
        }
        ("log", Some(sub_m)) => {
            log(&conn,
                sub_m.value_of("NAME").unwrap(),
                sub_m.value_of("MESSAGE"),
                0).chain_err(|| "log entry")?;
            Ok(())
        }
        ("like", Some(sub_m)) => {
            log(&conn,
                sub_m.value_of("NAME").unwrap(),
                sub_m.value_of("message"),
                1).chain_err(|| "log entry")?;
            Ok(())
        }
        ("hate", Some(sub_m)) => {
            log(&conn,
                sub_m.value_of("NAME").unwrap(),
                sub_m.value_of("message"),
                -1).chain_err(|| "log entry")?;
            Ok(())
        }
        ("set", Some(sub_m)) => {
            let weight = u32::from_str_radix(sub_m.value_of("WEIGHT").unwrap(),10).chain_err(|| "converting target weight")?;
            set(&conn,
                sub_m.value_of("NAME").unwrap(),
                sub_m.value_of("message"),
                weight).chain_err(|| "log entry")?;
            Ok(())
        }
        _ => bail!("incorrect options"),
    }
}
