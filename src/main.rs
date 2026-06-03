#![allow(clippy::too_many_arguments)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::to_string_trait_impl)]
#![allow(clippy::expect_fun_call)]
#![allow(clippy::manual_strip)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::single_match)]
#![allow(dead_code)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::search_is_some)]
#![allow(clippy::map_clone)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::nonminimal_bool)]

mod utils;

use clap::{self, arg, Command};
use colored::Colorize;
use std::{fs, fs::File, io::Write, process::exit};

fn main() {
    let key_arg = || {
        arg!(--key <KEY> "Encryption passphrase (overrides LSON_KEY env var)").required(false)
    };

    let raw = Command::new("raw")
        .about("Raw commands mode (output to stdout)")
        .subcommand(
            Command::new("compile")
                .about("Compile a kson file and print the result to stdout")
                .arg(
                    arg!(-f --file <FILE> "The kson file to compile")
                        .required_unless_present("text"),
                )
                .arg(arg!(--text <TEXT> "The kson text to compile").required_unless_present("file"))
                .arg(
                    arg!(-t --output_type <TYPE> "Output type: json | lson")
                        .default_value("lson"),
                )
                .arg(arg!(--kmodel <KMODEL> "The kmodel file to validate against"))
                .arg(key_arg())
                .arg_required_else_help(true),
        )
        .disable_help_flag(true)
        .arg(arg!(-h --help "Show this help message"));

    let mut menu = clap::command!()
        .display_name("🔒 LSON")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(arg!(-v --verbose "Print verbose output"))
        .subcommand(raw.clone())
        .subcommand(
            Command::new("compile")
                .about("Compile a kson file to json or lson")
                .arg(
                    arg!(-f --file <FILE> "The kson file to compile")
                        .required_unless_present("text"),
                )
                .arg(arg!(--text <TEXT> "The kson text to compile").required_unless_present("file"))
                .arg(arg!(-o --output <OUTPUT> "Output file path"))
                .arg(
                    arg!(-t --output_type <TYPE> "Output type: json | lson")
                        .default_value("lson"),
                )
                .arg(arg!(--kmodel <KMODEL> "The kmodel file to validate against"))
                .arg(key_arg())
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("compile_json")
                .about("Compile a json file to lson")
                .arg(arg!(<file> "The json file to compile").required(true))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("parse")
                .about("Decrypt and print an lson file")
                .arg(arg!(<file> "The lson file to decrypt").required(true))
                .arg(key_arg())
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("verify")
                .about("Check whether a kson file matches the sealed hash inside an lson file (no key needed)")
                .arg(arg!(-f --file <FILE> "The source kson file").required(true))
                .arg(arg!(--lson <LSON> "The compiled lson file").required(true))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("lock")
                .about("Write a .lock file pinning the resolved config (canonical JSON + KSON hash)")
                .arg(arg!(-f --file <FILE> "The kson file to lock").required(true))
                .arg(arg!(-o --output <OUTPUT> "Output .lock file (defaults to <file>.lock)"))
                .arg(arg!(--kmodel <KMODEL> "The kmodel file to validate against"))
                .arg_required_else_help(true),
        )
        .arg(arg!(-h --help "Show this help message"))
        .arg(arg!(-V --version "Show the version"));

    let matches = menu.clone().get_matches();

    if *matches.get_one::<bool>("version").unwrap_or(&false) {
        println!("🔒 LSON - v{}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }

    let verbose = *matches.get_one::<bool>("verbose").unwrap_or(&false);

    match matches.subcommand() {
        // ── compile ──────────────────────────────────────────────────────────
        Some(("compile", arg_m)) => {
            let file = arg_m.get_one::<String>("file");
            let text = arg_m.get_one::<String>("text");
            let output_type = arg_m.get_one::<String>("output_type").unwrap();
            let kmodel = arg_m.get_one::<String>("kmodel");
            let explicit_key = arg_m.get_one::<String>("key").map(|s| s.as_str());

            if output_type != "json" && output_type != "lson" {
                eprintln!("Invalid output type '{}' — choose json or lson", output_type);
                exit(1);
            }

            let out = match arg_m.get_one::<String>("output") {
                Some(o) => o.to_string(),
                None => match file {
                    Some(f) => format!("{}.{}", f.trim_end_matches(".kson"), output_type),
                    None => {
                        eprintln!("--output is required when using --text without a file");
                        exit(1);
                    }
                },
            };

            if output_type == "lson" {
                let key = utils::lson::resolve_key(explicit_key).unwrap_or_else(|e| {
                    eprintln!("{}", e.to_string().red());
                    exit(1);
                });

                eprintln!("{}", "Deriving key with Argon2id (this takes a moment)…".bright_black());

                let lson_result = if let Some(f) = file {
                    utils::lson::encrypt_file(f, &key)
                } else {
                    utils::lson::encrypt(text.unwrap(), &key)
                }
                .unwrap_or_else(|e| {
                    eprintln!("{}: {}", "error".red().bold(), e);
                    exit(1);
                });

                let mut out_file = File::create(&out).unwrap_or_else(|e| {
                    eprintln!("{}: {}", "error".red().bold(), e);
                    exit(1);
                });
                out_file.write_all(lson_result.as_bytes()).unwrap();
                println!("{}: {}", "Sealed LSON →".green(), out.yellow());
            } else {
                let json_result = if let Some(f) = file {
                    utils::kson::kson_items_to_json(
                        utils::kson::read_file(f, kmodel, verbose).unwrap(),
                    )
                } else {
                    utils::kson::kson_items_to_json(utils::kson::read(text.unwrap(), kmodel, verbose))
                };

                let mut out_file = File::create(&out).unwrap_or_else(|e| {
                    eprintln!("{}: {}", "error".red().bold(), e);
                    exit(1);
                });
                out_file.write_all(json_result.as_bytes()).unwrap();
                println!("{}: {}", "Wrote JSON →".green(), out.yellow());
            }
        }

        // ── compile_json ─────────────────────────────────────────────────────
        Some(("compile_json", _)) => {
            eprintln!("{}", "compile_json is not yet implemented".yellow());
            exit(1);
        }

        // ── parse ─────────────────────────────────────────────────────────────
        Some(("parse", arg_m)) => {
            let file = arg_m.get_one::<String>("file").unwrap();
            let explicit_key = arg_m.get_one::<String>("key").map(|s| s.as_str());

            let key = utils::lson::resolve_key(explicit_key).unwrap_or_else(|e| {
                eprintln!("{}", e.to_string().red());
                exit(1);
            });

            eprintln!("{}", "Deriving key with Argon2id…".bright_black());

            let plaintext = utils::lson::decrypt_file(file, &key).unwrap_or_else(|e| {
                eprintln!("{}: {}", "error".red().bold(), e);
                exit(1);
            });

            println!("{}\n{}", "── DECRYPTED KSON ──".green().bold(), plaintext);
        }

        // ── verify ────────────────────────────────────────────────────────────
        Some(("verify", arg_m)) => {
            let kson_file = arg_m.get_one::<String>("file").unwrap();
            let lson_file = arg_m.get_one::<String>("lson").unwrap();

            let kson_content = fs::read_to_string(kson_file).unwrap_or_else(|e| {
                eprintln!("{}: cannot read '{}': {}", "error".red().bold(), kson_file, e);
                exit(1);
            });

            let lson_content = fs::read_to_string(lson_file).unwrap_or_else(|e| {
                eprintln!("{}: cannot read '{}': {}", "error".red().bold(), lson_file, e);
                exit(1);
            });

            let sealed_hash = utils::lson::kson_hash_from_lson(&lson_content).unwrap_or_else(|e| {
                eprintln!("{}: {}", "error".red().bold(), e);
                exit(1);
            });

            let current_hash = utils::lson::sha256_hex(kson_content.as_bytes());

            if current_hash == sealed_hash {
                println!(
                    "{} Source KSON matches the sealed hash — no drift detected.",
                    "✓".green().bold()
                );
                println!("  sha256: {}", current_hash.bright_black());
            } else {
                eprintln!(
                    "{} Source KSON has changed since the LSON was compiled!",
                    "✗".red().bold()
                );
                eprintln!("  sealed:  {}", sealed_hash.yellow());
                eprintln!("  current: {}", current_hash.yellow());
                exit(1);
            }
        }

        // ── lock ──────────────────────────────────────────────────────────────
        Some(("lock", arg_m)) => {
            let file = arg_m.get_one::<String>("file").unwrap();
            let kmodel = arg_m.get_one::<String>("kmodel");

            let kson_content = fs::read_to_string(file).unwrap_or_else(|e| {
                eprintln!("{}: {}", "error".red().bold(), e);
                exit(1);
            });

            let json_result = utils::kson::kson_items_to_json(
                utils::kson::read_file(file, kmodel, verbose).unwrap(),
            );

            // Re-serialise through serde_json to guarantee sorted, canonical keys.
            let canonical = match serde_json::from_str::<serde_json::Value>(&json_result) {
                Ok(v) => serde_json::to_string_pretty(&v).unwrap_or(json_result),
                Err(_) => json_result,
            };

            let kson_hash = utils::lson::sha256_hex(kson_content.as_bytes());

            let out = arg_m
                .get_one::<String>("output")
                .cloned()
                .unwrap_or_else(|| format!("{}.lock", file.trim_end_matches(".kson")));

            let mut lock_file = File::create(&out).unwrap_or_else(|e| {
                eprintln!("{}: {}", "error".red().bold(), e);
                exit(1);
            });

            // Embed the KSON hash as a comment at the top of the lock file.
            writeln!(lock_file, "// kson-hash: {kson_hash}").unwrap();
            lock_file.write_all(canonical.as_bytes()).unwrap();
            lock_file.write_all(b"\n").unwrap();

            println!("{}: {}", "Wrote lock →".green(), out.yellow());
            println!("  kson-hash: {}", kson_hash.bright_black());
        }

        // ── raw ───────────────────────────────────────────────────────────────
        Some(("raw", arg_m)) => {
            if let Some((name, sub)) = arg_m.subcommand() {
                let file = sub.get_one::<String>("file");
                let text = sub.get_one::<String>("text");
                let output_type = sub.get_one::<String>("output_type").unwrap();
                let kmodel = sub.get_one::<String>("kmodel");
                let explicit_key = sub.get_one::<String>("key").map(|s| s.as_str());

                if name == "compile" {
                    if output_type == "lson" {
                        let key = utils::lson::resolve_key(explicit_key).unwrap_or_else(|e| {
                            eprintln!("{}", e.to_string().red());
                            exit(1);
                        });
                        let result = if let Some(f) = file {
                            utils::lson::encrypt_file(f, &key)
                        } else {
                            utils::lson::encrypt(text.unwrap(), &key)
                        }
                        .unwrap_or_else(|e| {
                            eprintln!("{}: {}", "error".red().bold(), e);
                            exit(1);
                        });
                        print!("{}", result);
                    } else {
                        let result = if let Some(f) = file {
                            utils::kson::kson_items_to_json(
                                utils::kson::read_file(f, kmodel, verbose).unwrap(),
                            )
                        } else {
                            utils::kson::kson_items_to_json(utils::kson::read(
                                text.unwrap(),
                                kmodel,
                                verbose,
                            ))
                        };
                        print!("{}", result);
                    }
                }
            } else {
                raw.clone().print_help().unwrap();
            }
        }

        Some((cmd, _)) => {
            eprintln!("Unknown command: {}", cmd);
            let _ = menu.print_help();
        }
        None => {
            println!("{}", "🔒 LSON — Type-safe encrypted configuration".cyan().bold());
            let _ = menu.print_help();
        }
    }
}
