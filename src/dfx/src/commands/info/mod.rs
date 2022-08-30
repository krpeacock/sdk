mod webserver_port;

use crate::commands::info::webserver_port::get_webserver_port;
use crate::lib::info;
use crate::{DfxResult, Environment};
use clap::Parser;

#[derive(clap::ValueEnum, Clone, Debug)]
enum InfoType {
    ReplicaRev,
    WebserverPort,
}

#[derive(Parser)]
#[clap(name("info"))]
pub struct InfoOpts {
    #[clap(value_enum)]
    info_type: InfoType,
}

pub fn exec(env: &dyn Environment, opts: InfoOpts) -> DfxResult {
    let value = match opts.info_type {
        InfoType::ReplicaRev => info::replica_rev().to_string(),
        InfoType::WebserverPort => get_webserver_port(env)?,
    };
    println!("{}", value);
    Ok(())
}
