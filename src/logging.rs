#![allow(dead_code)]

pub fn info(message: impl AsRef<str>) {
    eprintln!("[moon] {}", message.as_ref());
}
