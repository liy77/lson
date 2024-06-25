use crypto::aes;
use crypto::blockmodes::PkcsPadding;
use crypto::buffer::{BufferResult, ReadBuffer, WriteBuffer};
use std::fs::File;
use std::io::{BufReader, Read, Result};
use colored::Colorize;

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
        let byte_value = u8::from_str_radix(&byte, 2).unwrap();
        result.push(char::from(byte_value));
    }
    result
}

const BIT256_KEY: &[u8; 32] = b"01234567890123456789012345678901";

pub fn encrypt(text: &str) -> String {
    let text = &text_to_binary(text);

    let iv: [u8; 16] = [0u8; 16];

    let mut encryptor = aes::cbc_encryptor(
        aes::KeySize::KeySize256,
        BIT256_KEY,
        &iv,
        PkcsPadding,
    );

    let mut buffer = [0; 4096];
    let mut read_buffer = crypto::buffer::RefReadBuffer::new(text.as_bytes());
    let mut write_buffer = crypto::buffer::RefWriteBuffer::new(&mut buffer);

    let mut ciphertext = Vec::new();

    loop {
        let result = encryptor.encrypt(&mut read_buffer, &mut write_buffer, true).unwrap();
        ciphertext.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));

        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }

    let hex_str: String = ciphertext.iter().map(|b| format!("{:02x}", b)).collect();
    hex_str
}

pub fn decrypt(hex_ciphertext: &str) -> String {
    let iv: [u8; 16] = [0u8; 16];

    let mut decryptor = aes::cbc_decryptor(
        aes::KeySize::KeySize256,
        BIT256_KEY,
        &iv, 
        PkcsPadding
    );

    let mut ciphertext = Vec::new();
    for i in 0..hex_ciphertext.len() / 2 {
        let byte = u8::from_str_radix(&hex_ciphertext[2*i..2*i+2], 16).unwrap();
        ciphertext.push(byte);
    }

    let mut buffer = [0; 4096];
    let mut read_buffer = crypto::buffer::RefReadBuffer::new(&ciphertext);
    let mut write_buffer = crypto::buffer::RefWriteBuffer::new(&mut buffer);

    let mut plaintext = Vec::new();

    loop {
        let result = decryptor.decrypt(&mut read_buffer, &mut write_buffer, true).unwrap();
        plaintext.extend(write_buffer.take_read_buffer().take_remaining().iter().map(|&i| i));

        match result {
            BufferResult::BufferUnderflow => break,
            BufferResult::BufferOverflow => {}
        }
    }

    binary_to_text(&String::from_utf8(plaintext).unwrap())
}

pub fn encrypt_file(file_path: &str) -> Result<String> {
    let file = File::open(file_path).expect("FILE_NOT_FOUND".on_bright_red().to_string().as_str());

    let mut reader = BufReader::new(file);
    let mut text = String::new();

    reader.read_to_string(&mut text)?;

    Ok(encrypt(&text))
}

pub fn decrypt_file(file_path: &str) -> Result<String> {
    let file = File::open(file_path).expect("FILE_NOT_FOUND".on_bright_red().to_string().as_str());

    let mut reader = BufReader::new(file);
    let mut text = String::new();

    reader.read_to_string(&mut text)?;

    Ok(decrypt(&text))
}