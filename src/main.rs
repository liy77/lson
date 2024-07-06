mod utils;

use std::{env, fs::File, io::{Read, Write}, process::exit};
use clap::{self, arg, Command};
use colored::Colorize;

fn get_project_version() -> Option<String> {
    let mut file = match File::open("Cargo.toml") {
        Ok(file) => file,
        Err(_) => return None,
    };

    let mut contents = String::new();
    if let Err(_) = file.read_to_string(&mut contents) {
        return None; 
    }

    let value = match contents.parse::<toml::Value>() {
        Ok(value) => value,
        Err(_) => return None,
    };

    let version = match value.get("package").and_then(|pkg| pkg.get("version")) {
        Some(version) => version.as_str().map(|s| s.to_string()),
        None => None,
    };

    version
}

fn main() {    
    let raw = Command::new("raw")
    .about("Raw commands mode")
    .subcommand(
        Command::new("compile")
            .about("Compile a kson file and returns the result")
            .arg(
                arg!(-f --file <FILE> "The kson file to compile")
                .required_unless_present("text")
            )
            .arg(
                arg!(--text <TEXT> "The kson text to compile")
                .required_unless_present("file")
            )
            .arg(
                arg!(-t --output_type <TYPE> "The output type (json|lson) of generated file")
                .default_value("lson")
            )
            .arg(
                arg!(--kmodel <KMODEL> "The kmodel file to use")
            )
            .arg_required_else_help(true)
    )
    .disable_help_flag(true)
    .arg(arg!(-h --help "Show this help message"));

    let mut menu = clap::command!()
        .display_name("🐙 LSON")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(arg!(-v --verbose "Print verbose output"))
        .subcommand(raw.clone())
        .subcommand(Command::new("compile")
            .about("Compile a kson file")
            .arg(
                arg!(-f --file <FILE> "The kson file to compile")
                .required_unless_present("text")
            )
            .arg(
                arg!(--text <TEXT> "The kson text to compile")
                .required_unless_present("file")
            )
            .arg(
                arg!(-o --output <OUTPUT> "The output of generated file")
            )
            .arg(
                arg!(-t --output_type <TYPE> "The output type (json|lson) of generated file")
                .default_value("lson")
            )
            .arg(
                arg!(--kmodel <KMODEL> "The kmodel file to use")
            )
            .arg_required_else_help(true)
        )
        .subcommand(Command::new("compile_json")
            .about("Compile a json file to lson")
            .arg(
                arg!(<FILE> "The json file to compile")
                .required(true)
            )
            .arg_required_else_help(true)
        )
        .subcommand(Command::new("parse")
            .about("Parse a lson file")
            .arg(
                arg!(<FILE> "The lson file to parse")
                .required(true)
            )
            .arg_required_else_help(true)
        )
        .arg(arg!(-h --help                       "Show this help message"))
        .arg(arg!(-V --version                    "Show the version of the lson compiler"));
    
    let matches = menu.clone().get_matches();
    
    let version = matches.get_one::<bool>("version").unwrap_or(&false);

    if *version {
        println!("🐙 LSON - v{}", get_project_version().unwrap());
        exit(0);
    }

    let verbose = matches.get_one::<bool>("verbose").unwrap_or(&false);
    let verbose = *verbose;

    match matches.subcommand() {
        Some(("compile", arg_m)) => {
            let file = arg_m.get_one::<String>("file");
            let text = arg_m.get_one::<String>("text");
            let output_type = arg_m.get_one::<String>("output_type").unwrap();
            let kmodel = arg_m.get_one::<String>("kmodel");

            if output_type != "json" && output_type != "lson" {
                eprintln!("Invalid output type: {}, select json or lson", output_type);
                exit(1);
            }

            let out = arg_m.get_one::<String>("output");

            let out = if out.is_some() {
                out.unwrap().to_string()
            } else {
                if !file.is_some() && !text.is_some() {
                    eprintln!("Output file is required when no file is provided.");
                    exit(1);
                }

                let r = format!("{}.{}", file.unwrap().trim_end_matches(".kson"), output_type);
                r
            };

            if output_type == "lson" {
                let lson_result = if file.is_some() {
                    utils::lson::encrypt_file(file.unwrap()).unwrap()
                } else {
                    utils::lson::encrypt(text.unwrap())
                };                
                
                let mut lson_file = File::create(out.clone()).unwrap();

                lson_file.write_all(lson_result.as_bytes()).unwrap();
                println!("{}: {}", "Wrote LSON to".green(), out.yellow());
            } else {
                let json_result = if file.is_some() {
                    utils::kson::kson_items_to_json(utils::kson::read_file(file.unwrap(), kmodel, verbose).unwrap())
                } else {
                    utils::kson::kson_items_to_json(utils::kson::read(text.unwrap(), kmodel, verbose))
                };
                let mut json_file = File::create(out.clone()).unwrap();

                json_file.write_all(json_result.as_bytes()).unwrap();
                println!("{}: {}", "Wrote JSON to".green(), out.yellow());
            }
        },
        Some(("compile_json", arg_m)) => {
            let _file = arg_m.get_one::<String>("file").unwrap();
        },
        Some(("raw", arg_m)) => {
            let cmd = arg_m.subcommand();

            if cmd.is_some() {
                let (name, arg_m) = cmd.unwrap();
                let file = arg_m.get_one::<String>("file");
                let text = arg_m.get_one::<String>("text");
                let output_type = arg_m.get_one::<String>("output_type").unwrap();
                let kmodel = arg_m.get_one::<String>("kmodel");

                if name == "compile" {
                    if output_type == "lson" {
                        let lson_result = if file.is_some() {
                            utils::lson::encrypt_file(file.unwrap()).unwrap()
                        } else {
                            utils::lson::encrypt(text.unwrap())
                        };

                        println!("{}", lson_result);
                    } else {
                        let json_result = if file.is_some() {
                            utils::kson::kson_items_to_json(utils::kson::read_file(file.unwrap(), kmodel, verbose).unwrap())
                        } else {
                            utils::kson::kson_items_to_json(utils::kson::read(text.unwrap(), kmodel, verbose))
                        };

                        println!("{}", json_result);
                    }
                }
            } else {
                raw.clone().print_help().unwrap();
            }
        },
        Some(("parse", arg_m)) => {
            let file = arg_m.get_one::<String>("file").unwrap();
            let kson_result = utils::lson::decrypt_file(file).unwrap();
            
            println!("{}\n{}", "PARSED DATA:".green(), kson_result);
        },
        Some((cmd, _)) => {
            eprintln!("Unknown command: {}", cmd);
            let _ = menu.print_help();
        },
        None => {
            println!("{}", "🐙 LSON - Type-safe configuration file".red().bold());
            let _ = menu.print_help();
        },
    }
}
