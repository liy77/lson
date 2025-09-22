#![allow(dead_code)]

use colored::Colorize;

pub fn debug(verbose: bool, message: &str) {
    if verbose {
        println!("{} {}", "debug".blue(), message);
    }
}

pub fn warn(message: &str) {
    println!("{} {}", "warning".yellow(), message);
}