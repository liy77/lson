#![allow(dead_code)]

use colored::Colorize;
use regex::Regex;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    process::exit,
};

use crate::utils::debug::debug;

#[derive(Debug)]
pub enum KType {
    Any,
    Unknown,
    String,
    Char,
    Integer,
    Float,
    Boolean,
    Array(Box<KType>),
    Optional(Box<KType>),
}

impl KType {
    pub fn is_required(&self) -> bool {
        match self {
            KType::Optional(_) => false,
            _ => true,
        }
    }
}

impl ToString for KType {
    fn to_string(&self) -> String {
        match self {
            KType::String => "String".to_string(),
            KType::Char => "Char".to_string(),
            KType::Integer => "Integer".to_string(),
            KType::Float => "Float".to_string(),
            KType::Boolean => "Bool".to_string(),
            KType::Any => "Any".to_string(),
            KType::Unknown => "Unknown".to_string(),
            KType::Array(kind) => format!("Array<{}>", kind.to_string()),
            KType::Optional(k) => format!("{}?", k.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum KModelSection {
    Required(String),
    Optional(String),
}

impl KModelSection {
    pub fn is_required(&self) -> bool {
        match self {
            KModelSection::Required(_) => true,
            KModelSection::Optional(_) => false,
        }
    }
}

impl ToString for KModelSection {
    fn to_string(&self) -> String {
        match self {
            KModelSection::Required(k) => k.to_string(),
            KModelSection::Optional(k) => k.to_string(),
        }
    }
}

#[derive(Debug)]
pub enum KItemType {
    Section(KModelSection, Vec<KItemType>),
    Property(String, KType),
}

#[derive(Debug)]
pub struct KModel {
    pub properties: Vec<KItemType>,
    pub _sections: Vec<String>,
}

impl KModel {
    pub fn new(properties: Vec<KItemType>) -> KModel {
        KModel {
            properties,
            _sections: vec![],
        }
    }

    pub fn get_property(&self, key: &str) -> Option<&KType> {
        for item in &self.properties {
            if let KItemType::Property(k, v) = item {
                if k == key {
                    return Some(v);
                }
            }
        }

        None
    }

    pub fn get_section(&self, section: &str) -> Option<&Vec<KItemType>> {
        for item in &self.properties {
            if let KItemType::Section(k, v) = item {
                let k = k.to_string();

                if k == section {
                    return Some(v);
                }
            }
        }

        None
    }

    pub fn get_section_property(&self, section: &str, key: &str) -> Option<&KType> {
        if let Some(section) = self.get_section(section) {
            for item in section {
                if let KItemType::Property(k, v) = item {
                    if k == key {
                        return Some(v);
                    }
                }
            }
        }

        None
    }

    pub fn last_section(&self) -> Option<&str> {
        if let Some(section) = self._sections.last() {
            Some(section.as_str())
        } else {
            None
        }
    }

    pub fn push_section(&mut self, section: KModelSection) {
        self.properties
            .push(KItemType::Section(section.clone(), vec![]));
        self._sections.push(section.to_string());
    }

    pub fn pop_section(&mut self) {
        self._sections.pop();
    }

    pub fn attr(&mut self, item: KItemType) {
        if let Some(_section) = self.last_section() {
            if let Some(KItemType::Section(_section, properties)) = self.properties.last_mut() {
                properties.push(item);
            }
        } else {
            self.properties.push(item);
        }
    }
}

pub fn get_kmodel_colored() -> String {
    let mut kmodel_string = String::new();
    kmodel_string.push_str(&"K".red().to_string());
    kmodel_string.push_str(&"Model".blue().to_string());

    kmodel_string
}

pub fn read(file_path: &str, verbose: bool) -> KModel {
    // Open the file
    let file = File::open(file_path).expect("MISSING_KMODEL".on_bright_red().to_string().as_str());

    // Read the file line by line
    let reader = BufReader::new(file);
    let mut kson = KModel::new(vec![]);

    let kmodel_string = get_kmodel_colored();

    for line in reader.lines() {
        let line = line.expect("Error reading line");

        if line.starts_with("$") {
            let section = line[1..].trim();
            debug(
                verbose,
                &format!(
                    "{} Entering section: {}",
                    kmodel_string,
                    section.bold().bright_cyan()
                ),
            );

            if section.ends_with("?") {
                kson.push_section(KModelSection::Optional(
                    section[..section.len() - 1].to_string(),
                ));
            } else {
                kson.push_section(KModelSection::Required(section.to_string()));
            }
        } else if let Some((key, value)) = parse_property_line(&line) {
            if kson._sections.len() > 0 {
                if !line.starts_with("   ".repeat(kson._sections.len()).as_str()) {
                    debug(
                        verbose,
                        &format!(
                            "{} Exiting from section: {}",
                            kmodel_string,
                            kson.last_section().unwrap().bold().bright_red()
                        ),
                    );
                    kson.pop_section();
                }
            }

            if line.starts_with(&key) && kson._sections.len() > 0 {
                debug(
                    verbose,
                    &format!("{} Exiting from all sections", kmodel_string),
                );
                kson._sections.clear();
            }

            debug(
                verbose,
                &format!(
                    "{} Adding property: {}: {}",
                    kmodel_string,
                    key.bold().black(),
                    value.red()
                ),
            );

            if value.ends_with("?") {
                kson.attr(KItemType::Property(
                    key,
                    KType::Optional(Box::new(parse_type(&value[..value.len() - 1]))),
                ));
            } else {
                kson.attr(KItemType::Property(key, parse_type(&value)));
            }
        }
    }

    kson
}

fn parse_type(t: &str) -> KType {
    match t {
        "String" => KType::String,
        "Char" => KType::Char,
        "Integer" => KType::Integer,
        "Float" => KType::Float,
        "Bool" => KType::Boolean,
        "Any" => KType::Any,
        k if k.starts_with("Array") => {
            if k.contains("<") && k.contains(">") {
                let re = Regex::new(r"<([^<>]+)>").unwrap();
                let kind = re.captures(k);

                if kind.is_some() {
                    let (_, [kind]) = kind.unwrap().extract();

                    if kind.ends_with("?") {
                        return KType::Array(Box::new(KType::Optional(Box::new(parse_type(
                            &kind[..kind.len() - 1],
                        )))));
                    }

                    return KType::Array(Box::new(parse_type(kind)));
                }
            }

            eprintln!("Invalid array type received");
            exit(1);
        }
        _ => KType::Unknown,
    }
}

fn parse_property_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}
