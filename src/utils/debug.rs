use colored::Colorize;

pub fn debug(verbose: bool, message: &str) {
    if verbose {
        println!("{} {}", "debug".blue(), message);
    }
}