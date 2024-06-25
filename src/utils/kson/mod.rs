use std::fs::File;
use std::io::{Result, Read};
use std::process::exit;
use colored::Colorize;
use regex::Regex;

use crate::utils::kson;

use super::debug::debug;

pub mod kmodel;

#[derive(Debug)]
pub enum KSONItem {
    Section(String, Vec<KSONItem>),
    Property(String, String),
}

pub struct KSON {
    pub properties: Vec<KSONItem>,
    pub _sections: Vec<String>,
}

impl KSON {
    pub fn new(properties: Vec<KSONItem>) -> KSON {
        KSON {
            properties,
            _sections: vec![],
        }
    }

    pub fn get_property(&self, key: &str) -> Option<&str> {
        for item in &self.properties {
            if let KSONItem::Property(k, v) = item {
                if k == key {
                    return Some(v);
                }
            }
        }

        None
    }

    pub fn get_section(&self, section: &str) -> Option<&Vec<KSONItem>> {
        for item in &self.properties {
            if let KSONItem::Section(k, v) = item {
                if k == section {
                    return Some(v);
                }
            }
        }

        None
    }

    #[allow(unused)]
    pub fn get_section_property(&self, section: &str, key: &str) -> Option<&str> {
        if let Some(section) = self.get_section(section) {
            for item in section {
                if let KSONItem::Property(k, v) = item {
                    if k == key {
                        return Some(v);
                    }
                }
            }
        }

        None
    }

    fn last_section(&self) -> Option<&str> {
        if let Some(section) = self._sections.last() {
            Some(section.as_str())
        } else {
            None
        }
    }

    pub fn pop_section(&mut self) {
        self._sections.pop();
    }

    pub fn push_section(&mut self, section: &str) {
        self.properties.push(KSONItem::Section(section.to_string(), vec![]));
        self._sections.push(section.to_string());
    }

    pub fn attr(&mut self, item: KSONItem) {
        if let Some(_section) = self.last_section() {
            if let Some(KSONItem::Section(_section, properties)) = self.properties.last_mut() {
                properties.push(item);
            }
        } else {
            self.properties.push(item);
        }
    }
}

pub fn read_file(file_path: &str, kmodel_file: Option<&String>, verbose: bool) -> Result<Vec<KSONItem>> {
    // Open the file
    let mut file = File::open(file_path).expect("FILE_NOT_FOUND".on_bright_red().to_string().as_str());
    let mut text = String::new();

    file.read_to_string(&mut text)?;

   Ok(read(&text, kmodel_file, verbose))
}

pub fn read(text: &str, kmodel_file: Option<&String>, verbose: bool) -> Vec<KSONItem> {
    let mut kson = KSON::new(vec![]);
    let mut ksonmodel: Option<kmodel::KModel> = None;
    let kmodel_string = kson::kmodel::get_kmodel_colored();
    let mut any_warn_emitted = false;

    for line in text.lines() {
        if line.starts_with("@model") {
            let model = line[6..].trim();
            let model = model
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim_start_matches('"')
                .trim_end_matches('"');

            ksonmodel = Some(kmodel::read(model, verbose));

            debug(verbose, &format!("KModel: {}", model
                .bold()
                .yellow()
            ));

            continue;
        } else if kmodel_file.is_some() {
            let model = kmodel_file.unwrap();
            ksonmodel = Some(kmodel::read(model, verbose));

            debug(verbose, &format!("KModel: {}", model
                .bold()
                .yellow()
            ));
        }

        // Skip comments
        if line.trim().starts_with('#') {
            continue;
        }
 
        if line.starts_with("$") {
            let section = line[1..].trim();
            debug(verbose, &format!("Entering section: {}", section.bold().bright_cyan()));

            if kson._sections.len() > 0 {                
                debug(verbose, "Exiting from all sections before entering a new section");
                kson._sections.clear();
            }

            kson.push_section(section);
        } else if let Some((key, value)) = parse_property_line(&line) {
            if kson._sections.len() > 0 {
                if !line.starts_with("   ".repeat(kson._sections.len()).as_str()) {
                    debug(verbose, &format!("Exiting from section: {}", kson.last_section().unwrap().bold().bright_red()));
                    kson.pop_section();
                }
            }
            
            if line.starts_with(&key) && kson._sections.len() > 0 {
                kson._sections.clear();
            }
            
            debug(verbose, &format!("Adding property: {} = {}", key.bold().black(), value.red()));

            if ksonmodel.is_some() {
                let ksonmodel = ksonmodel.as_ref().unwrap();
                
                let kt_value = if kson._sections.len() > 0 {                
                    ksonmodel.get_section_property(kson.last_section().unwrap(), &key)
                } else {
                    ksonmodel.get_property(&key)
                };
                
                match kt_value {
                    Some(kt_value) => {
                        debug(verbose, &format!("{}: {}: {}", kmodel_string, key.bold().black(), kt_value.to_string().red()));

                        let kt_value = kt_value.to_string().trim_end_matches('?').to_string();
                        let array_re: Regex = Regex::new(r"\[(.*?)\]").unwrap();
                        let array_cap = array_re.captures_iter(&value).collect::<Vec<_>>();

                        if kt_value == "Any" {
                            if !any_warn_emitted {
                                debug(verbose, &format!("{}: {} Use of Any type is not recommended", kmodel_string, "[WARN]".yellow()));
                                any_warn_emitted = true;
                            }

                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                            continue;
                        }

                        if value.starts_with('"') && value.ends_with('"') && kt_value == "String" {
                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                        } else if value.starts_with('\'') && value.ends_with('\'') && kt_value == "Char" {
                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                        } else if value.parse::<i32>().is_ok() && kt_value == "Integer" {
                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                        } else if value.parse::<f32>().is_ok() && kt_value == "Float" {
                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                        } else if value.parse::<bool>().is_ok() && kt_value == "Bool" {
                            kson.attr(KSONItem::Property(key.clone(), value.clone()));
                        } else if array_cap.len() > 0 && kt_value.starts_with("Array") {
                            let re = Regex::new(r"<([^<>]+)>").unwrap();
                            let (_, [mut kind]) = re.captures(&kt_value).unwrap().extract();

                            if kind.ends_with("?") {
                                kind = &kind[..kind.len() - 1];
                            }
                            
                            let mut index = 0;
                            for v in array_cap {
                                let value = &v[1];

                                debug(verbose, &format!("Checking index {index} type"));
                                
                                index += 1;

                                if kind.starts_with("Array") {
                                    eprintln!("{}: Cannot put array inside array", kmodel_string);
                                    exit(1);
                                }

                                if value.starts_with('"') && value.ends_with('"') && kind == "String" {
                                    continue;
                                } else if value.starts_with('\'') && value.ends_with('\'') && kind == "Char" {
                                    continue;
                                } else if value.parse::<i32>().is_ok() && kind == "Integer" {
                                    continue;
                                } else if value.parse::<f32>().is_ok() && kind == "Float" {
                                    continue;
                                } else if value.parse::<bool>().is_ok() && kind == "Bool" {
                                    continue;
                                }

                                eprintln!("{}: Invalid value {} for {} at index[{}] of property: {}\nExpected value of type: {}", kmodel_string, value.red(), kt_value.bold().black(), index.to_string().bold(), key.bold().black(), kind.red()); 
                                exit(1);
                            }

                            kson.attr(KSONItem::Property(key.clone(), value.clone()))
                        } else {
                            eprintln!("{}: Invalid value for property: {} = {}\nExpected value of type: {}", kmodel_string, key.bold().black(), value.red(), kt_value.red());
                            exit(1);
                        }
                    },
                    None => {
                    }
                }
            } else {
                kson.attr(KSONItem::Property(key, value));                
            }
        }
    }

    if ksonmodel.is_some() {
        let ksonmodel = ksonmodel.unwrap();

        for item in ksonmodel.properties {
            match item {
                kmodel::KItemType::Section(key, properties) => {
                    let kson_section = if key.is_required() {
                        let key = key.to_string();                        
                        
                        if kson.get_section(&key).is_none() {
                            eprintln!("{} Section {} is required by {}", "error".red(), key.to_string().bold().black(), kson::kmodel::get_kmodel_colored());
                            exit(1);
                        }

                        kson.get_section(&key)
                    } else {
                        let key = key.to_string();



                        kson.get_section(&key)
                    };

                    if kson_section.is_some() {
                        let kson_section = kson_section.unwrap();

                        for kitem in properties {
                            match kitem {
                                kmodel::KItemType::Property(k, v) => {
                                    if kson_section.iter().find(|&item| {
                                        if let KSONItem::Property(k2, _) = item {
                                            k2 == &k
                                        } else {
                                            false
                                        }
                                    }).is_none() && v.is_required() {
                                        eprintln!("{} Property {} in {} is required by {}", "error".red(), k.bold().black(), key.to_string().bold().bright_cyan(), kson::kmodel::get_kmodel_colored());
                                        exit(1);
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                },
                kmodel::KItemType::Property(key, value) => {
                    if kson.get_property(&key).is_none() && value.is_required() {
                        eprintln!("{} Property {} is required by {}", "error".red(), key.bold().black(), kson::kmodel::get_kmodel_colored());
                        exit(1);
                    }
                },
                
            }
        }
    }

    kson.properties
}

fn parse_property_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split('=').map(|s| s.trim()).collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

pub fn kson_items_to_json(items: Vec<KSONItem>) -> String {
    let mut json = String::from("{");

    for item in items {
        match item {
            KSONItem::Property(key, value) => {
                if value.starts_with('\'') && value.ends_with('\'') {
                    json.push_str(&format!("\"{}\": \"{}\",", key, value.trim_start_matches('\'').trim_end_matches('\'')));
                    continue;
                }
                
                json.push_str(&format!("\"{}\": {},", key, value));
            },
            KSONItem::Section(key, properties) => {
                json.push_str(&format!("\"{}\": {},", key, kson_items_to_json(properties)));
            },
        }
    }

    json.pop();
    json.push('}');
    json
}