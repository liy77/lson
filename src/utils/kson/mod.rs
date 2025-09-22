use std::env;
use std::fs::File;
use std::io::{Result, Read};
use std::process::exit;
use colored::Colorize;
use dotenvy::dotenv;
use regex::Regex;

use crate::utils::kson;

use super::debug::{debug, warn};

pub mod kmodel;

#[derive(Debug)]
pub enum KSONItem {
    Section(String, Vec<KSONItem>),
    Property(String, String),
}

pub struct KSON {
    pub properties: Vec<KSONItem>,
    pub _sections: Vec<String>,
    pub _section_indents: Vec<usize>, // Track indentation level for each section
    pub env_vars: Vec<String>,
}

impl KSON {
    pub fn new(properties: Vec<KSONItem>) -> KSON {
        KSON {
            properties,
            _sections: vec![],
            _section_indents: vec![],
            env_vars: vec![],
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
        self._section_indents.pop();
    }

    pub fn push_section(&mut self, section: &str, indent: usize) {
        let new_section = KSONItem::Section(section.to_string(), vec![]);
        
        if self._sections.is_empty() {
            // Add to root level
            self.properties.push(new_section);
        } else {
            // Add to the current nested section
            let sections = self._sections.clone();
            Self::add_to_nested_section(&mut self.properties, &sections, 0, new_section);
        }
        
        self._sections.push(section.to_string());
        self._section_indents.push(indent);
    }

    pub fn attr(&mut self, item: KSONItem) {
        if self._sections.is_empty() {
            // Add to root level
            self.properties.push(item);
        } else {
            // Navigate to the correct nested section
            let sections = self._sections.clone();
            Self::add_to_nested_section(&mut self.properties, &sections, 0, item);
        }
    }
    
    fn add_to_nested_section(properties: &mut Vec<KSONItem>, sections: &[String], depth: usize, item: KSONItem) {
        if depth >= sections.len() {
            properties.push(item);
            return;
        }
        
        let target_section = &sections[depth];
        
        // Find the matching section and navigate deeper
        for prop in properties.iter_mut().rev() {
            if let KSONItem::Section(section_name, ref mut section_props) = prop {
                if section_name == target_section {
                    Self::add_to_nested_section(section_props, sections, depth + 1, item);
                    return;
                }
            }
        }
        
        // If we reach here, something went wrong
        properties.push(item);
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
    let dotenv = dotenv();

    if dotenv.is_err() {
        warn(&format!("{}: {}. If you are using the {} keyword, an error may occur", "MISSING_ENV_FILE", dotenv.err().unwrap(), "@env".bold().black()));
    }

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

        if line.starts_with("@env") {
            let env_var = line[4..].trim();
            let env_var = env_var.trim_start_matches('(').trim_end_matches(')');
            kson.env_vars.push(env_var.to_string());
            let env_var = env::var(env_var).expect(&format!("{}: {}", "MISSING_ENV".on_bright_red(), env_var));

            debug(verbose, &format!("Adding env var: {} = {}", env_var.bold().black(), env_var.red()));

            continue;
        }

        // Skip comments
        if line.trim().starts_with('#') {
            continue;
        }
 
        // Check if line contains a section (starts with $ after whitespace)
        let trimmed_line = line.trim_start();
        if trimmed_line.starts_with("$") {
            let section = trimmed_line[1..].trim();
            
            // Calculate indentation level of the section
            let leading_whitespace = line.len() - line.trim_start().len();
            
            debug(verbose, &format!("Section {} found at indentation {}", section.bold().bright_cyan(), leading_whitespace));
            
            // Exit sections that are at equal or greater indentation level
            while !kson._section_indents.is_empty() {
                let last_indent = *kson._section_indents.last().unwrap();
                if leading_whitespace <= last_indent {
                    debug(verbose, &format!("Exiting from section: {} (section indentation: {} <= {})", kson.last_section().unwrap().bold().bright_red(), leading_whitespace, last_indent));
                    kson.pop_section();
                } else {
                    break;
                }
            }
            
            debug(verbose, &format!("Entering section: {}", section.bold().bright_cyan()));
            kson.push_section(section, leading_whitespace);
        } else if let Some((key, mut value)) = parse_property_line(&line) {
            // Calculate the indentation level of the property
            let leading_whitespace = line.len() - line.trim_start().len();
            
            // Exit sections if property is at the same or lesser indentation level
            while !kson._section_indents.is_empty() {
                let last_indent = *kson._section_indents.last().unwrap();
                if leading_whitespace <= last_indent {
                    debug(verbose, &format!("Exiting from section: {} (property indentation: {} <= {})", kson.last_section().unwrap().bold().bright_red(), leading_whitespace, last_indent));
                    kson.pop_section();
                } else {
                    break;
                }
            }

            if kson.env_vars.contains(&value.to_string()) {
                let env_var = env::var(&value).expect(&format!("{}: {}", "MISSING_ENV".on_bright_red(), value));
                debug(verbose, &format!("{}: {} = {}", "Replacing env var".yellow(), value.red(), env_var.red()));

                value = format!("\"{}\"", env_var);
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
                                warn(&format!("{} Use of Any type is not recommended", kmodel_string));
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