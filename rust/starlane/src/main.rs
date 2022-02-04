#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate tablestream;


use std::fs::File;
use std::io::{Read, Write};
use std::io;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use clap::{App, Arg, ArgMatches, SubCommand};
use starlane_core::error::Error;
use tracing_subscriber::FmtSubscriber;
use tracing::dispatcher::set_global_default;
use tokio::runtime::Runtime;
use starlane_core::starlane::StarlaneMachine;
use starlane_core::template::ConstellationLayout;
use starlane_core::util::shutdown;
use starlane_core::util;
use starlane_core::starlane::api::StarlaneApi;
use std::convert::TryInto;
use mesh_portal_serde::version::latest::entity::request::create::Require;
use tokio::io::AsyncReadExt;
use starlane_core::command::cli::{CliClient, outlet};
use starlane_core::command::cli::outlet::Frame;
use starlane_core::command::compose::CommandOp;
use starlane_core::star::shell::sys::SysCall::Create;


pub mod cli;
pub mod resource;


fn main() -> Result<(), Error> {
    let rt = Runtime::new().unwrap();
    rt.block_on( async move { go().await });
    Ok(())
}

async fn go() -> Result<(),Error> {
    let subscriber = FmtSubscriber::default();
    set_global_default(subscriber.into()).expect("setting global default tracer failed");

    ctrlc::set_handler(move || {
        std::process::exit(1);
    })
    .expect("expected to be able to set ctrl-c handler");

    let mut clap_app = App::new("Starlane")
        .version("0.1.0")
        .author("Scott Williams <scott@mightydevco.com>")
        .about("A Resource Mesh").subcommands(vec![SubCommand::with_name("serve").usage("serve a starlane machine instance").arg(Arg::with_name("with-external").long("with-external").takes_value(false).required(false)).display_order(0),
                                                            SubCommand::with_name("config").subcommands(vec![SubCommand::with_name("set-shell").usage("set the shell that the starlane CLI connects to").arg(Arg::with_name("hostname").required(true).help("the hostname of the starlane instance you wish to connect to")).display_order(0),
                                                                                                                            SubCommand::with_name("get-shell").usage("get the shell that the starlane CLI connects to")]).usage("read or manipulate the cli config").display_order(1).display_order(1),
                                                            SubCommand::with_name("exec").usage("execute a command").args(vec![Arg::with_name("command_line").required(true).help("command line to execute")].as_slice()),

    ]);

    let matches = clap_app.clone().get_matches();

    if let Option::Some(serve) = matches.subcommand_matches("serve") {
            let starlane = StarlaneMachine::new("server".to_string()).unwrap();
            let layout = match serve.is_present("with-external") {
                false => ConstellationLayout::standalone().unwrap(),
                true => ConstellationLayout::standalone_with_external().unwrap(),
            };

            starlane
                .create_constellation("standalone", layout)
                .await
                .unwrap();
            starlane.listen().await.expect("expected listen to work");
            starlane.join().await;
    } else if let Option::Some(matches) = matches.subcommand_matches("config") {
        if let Option::Some(_) = matches.subcommand_matches("get-shell") {
            let config = crate::cli::CLI_CONFIG.lock()?;
            println!("{}", config.hostname);
        } else if let Option::Some(args) = matches.subcommand_matches("set-shell") {
            let mut config = crate::cli::CLI_CONFIG.lock()?;
            config.hostname = args
                .value_of("hostname")
                .ok_or("expected hostname")?
                .to_string();
            config.save()?;
        } else {
            clap_app.print_long_help().unwrap_or_default();
        }
    } else if let Option::Some(args) = matches.subcommand_matches("exec") {
        exec(args.clone()).await.unwrap();
    } else {
        clap_app.print_long_help().unwrap_or_default();
    }

    Ok(())
}

async fn exec(args: ArgMatches<'_>) -> Result<(), Error> {
    let mut client = client().await?;
    let line = args.value_of("command_line").ok_or("expected command line")?.to_string();

    let op = CommandOp::from_str(line.as_str() )?;
    let requires = op.requires();

    let mut exchange = client.send(line).await?;

    for require in requires {
        match require {
            Require::File(name) => {
                println!("transfering: '{}'",name.as_str());
                let mut file = File::open(name.clone()).unwrap();
                let mut buf = vec![];
                file.read_to_end(&mut buf)?;
                let bin = Arc::new(buf);
                exchange.file( name, bin).await?;
            }
        }
    }

    exchange.end_requires().await?;

    while let Option::Some(Ok(frame)) = exchange.read().await {
        match frame {
            outlet::Frame::StdOut(line) => {
                println!("{}", line);
            }
            outlet::Frame::StdErr(line) => {
                eprintln!("{}", line);
            }
            outlet::Frame::EndOfCommand(code) => {
                std::process::exit(code);
            }
        }
    }

    Ok(())
}

async fn publish(args: ArgMatches<'_>) -> Result<(), Error> {
    unimplemented!();
    /*
    let bundle = Address::from_str(args.value_of("address").ok_or("expected address")?)?;

    let input = Path::new(args.value_of("dir").ok_or("expected directory")?);

    let mut zipfile = if input.is_dir() {
        let zipfile = tempfile::NamedTempFile::new()?;
        util::zip(
            args.value_of("dir")
                .expect("expected directory")
                .to_string()
                .as_str(),
            &zipfile.reopen()?,
            zip::CompressionMethod::Deflated,
        )?;
        zipfile.reopen()?
    } else {
        File::open(input)?
    };

    let mut data = Vec::with_capacity(zipfile.metadata()?.len() as _);
    zipfile.read_to_end(&mut data).unwrap();
    let data = Arc::new(data);

    let starlane_api = starlane_api().await?;


    let template = Template::new()
    let create = starlane_api.create()
    create.submit().await?;

    Ok(())

     */
}

/*
async fn cp(args: ArgMatches<'_>) -> Result<(), Error> {

    let starlane_api = starlane_api().await?;

    let src = args.value_of("src").ok_or("expected src")?;
    let dst = args.value_of("dst").ok_or( "expected dst")?;

    if dst.contains(":") {
        let dst = ResourcePath::from_str(dst)?;
        let src = Path::new(src );
        // copying from src to dst
        let mut src = File::open(src )?;
        let mut content= Vec::with_capacity(src.metadata()?.len() as _);
        src.read_to_end(&mut content ).unwrap();
        let content = Arc::new(content);
        let content = BinSrc::Memory(content);
        let mut state = DataSet::new();
        state.insert("content".to_string(), content );

        let meta = Meta::new();
        let meta = BinSrc::Memory(Arc::new(meta.bin()?));
        state.insert("meta".to_string(), meta );

        let create = ResourceCreate {
            parent: dst
                .parent()
                .ok_or("must have an address with a parent")?
                .into(),
            key: KeyCreationSrc::None,
            address: AddressCreationSrc::Exact(dst),
            archetype: ResourceArchetype {
                kind: ResourceKind::File(FileKind::File),
                specific: None,
                config: ConfigSrc::None,
            },
            state_src: AssignResourceStateSrc::Direct(state),
            registry_info: Option::None,
            owner: Option::None,
            strategy: ResourceCreateStrategy::CreateOrUpdate,
            from: MessageFrom::Inject
        };

        starlane_api.create_resource(create).await?;

        starlane_api.shutdown();

    } else  if src.contains(":") {
      let src = ResourcePath::from_str(src)?;
      let content = starlane_api.get_resource_state(src.into()).await?.remove("content").expect("expected 'content' state aspect");
      let filename = dst.clone();
      let dst = Path::new(dst );
      let mut dst = File::create(dst).expect(format!("could not open file for writing: {}", filename ).as_str() );
      match content {
          BinSrc::Memory(bin) => {
              dst.write_all(bin.as_slice() ).expect(format!("could not write to file: {}", filename ).as_str() )
          }
      }
    } else {
        unimplemented!("copy from starlane to local not yet supported")
    }

    Ok(())
}

 */



pub async fn client() -> Result<CliClient, Error> {
    let host = {
        let config = crate::cli::CLI_CONFIG.lock()?;
        config.hostname.clone()
    };
    CliClient::new(host).await
}
