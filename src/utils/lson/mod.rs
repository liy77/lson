#![allow(dead_code)]

use std::fs::File;
use std::io::{BufReader, Read, Result};
use colored::Colorize;
use base64::{Engine as _, engine::general_purpose};

fn text_to_binary(text: &str) -> String {
    let mut result = String::new();
    for c in text.chars() {
        result.push_str(&format!("{:08b}", c as u8));
    }
    result
}

fn binary_to_text(binary: &str) -> String {
    let mut result = String::new();
    for chunk in binary.chars().collect::<Vec<char>>().chunks(8) {
        let byte: String = chunk.iter().collect();
        if let Ok(byte_value) = u8::from_str_radix(&byte, 2) {
            result.push(char::from(byte_value));
        }
    }
    result
}

// Simple XOR encryption for demo purposes
const KEY: &[u8] = b"lson_encryption_key_32_bytes_long";

fn xor_encrypt_decrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    data.iter()
        .enumerate()
        .map(|(i, &byte)| byte ^ key[i % key.len()])
        .collect()
}

pub fn encrypt(text: &str) -> String {
    let text = text_to_binary(text);
    let data = text.as_bytes();
    
    let encrypted = xor_encrypt_decrypt(data, KEY);
    general_purpose::STANDARD.encode(encrypted)
}

pub fn decrypt(encrypted_text: &str) -> String {
    match general_purpose::STANDARD.decode(encrypted_text) {
        Ok(encrypted_data) => {
            let decrypted = xor_encrypt_decrypt(&encrypted_data, KEY);
            match String::from_utf8(decrypted) {
                Ok(binary_text) => binary_to_text(&binary_text),
                Err(_) => String::new(),
            }
        }
        Err(_) => String::new(),
    }
}

pub fn encrypt_file(file_path: &str) -> Result<String> {
    let file = File::open(file_path).map_err(|_| {
        eprintln!("{}", "FILE_NOT_FOUND".on_bright_red());
        std::io::Error::new(std::io::ErrorKind::NotFound, "File not found")
    })?;

    let mut reader = BufReader::new(file);
    let mut text = String::new();

    reader.read_to_string(&mut text)?;

    Ok(encrypt(&text))
}

pub fn decrypt_file(file_path: &str) -> Result<String> {
    let file = File::open(file_path).map_err(|_| {
        eprintln!("{}", "FILE_NOT_FOUND".on_bright_red());
        std::io::Error::new(std::io::ErrorKind::NotFound, "File not found")
    })?;

    let mut reader = BufReader::new(file);
    let mut text = String::new();

    reader.read_to_string(&mut text)?;

    Ok(decrypt(&text))
}