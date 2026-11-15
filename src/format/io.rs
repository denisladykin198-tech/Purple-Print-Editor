//! Низкоуровневые читатель/писатель для MPP. Little-endian.

use crate::constants::{MAX_FRAMES, MAX_OPTIONS, MAX_SOCKETS, MAX_STR, MAX_ZONES};

pub(super) struct W(pub Vec<u8>);

impl W {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f3(&mut self, v: [f32; 3]) {
        for x in v {
            self.f32(x);
        }
    }
    pub fn str(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
    pub fn rle(&mut self, data: &[u8]) {
        let enc = super::rle::rle_encode(data);
        self.u32(enc.len() as u32);
        self.0.extend_from_slice(&enc);
    }
}

pub(super) struct R<'a> {
    pub d: &'a [u8],
    pub p: usize,
}

impl<'a> R<'a> {
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.d.len() {
            return Err("Файл обрезан или повреждён".into());
        }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn f3(&mut self) -> Result<[f32; 3], String> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn str(&mut self) -> Result<String, String> {
        let n = self.u32()? as usize;
        if n > MAX_STR {
            return Err("Слишком длинная строка в файле".into());
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|e| e.to_string())
    }
    pub fn rle(&mut self, expected: usize) -> Result<Vec<u8>, String> {
        let n = self.u32()? as usize;
        let enc = self.take(n)?;
        super::rle::rle_decode(enc, expected)
    }
    pub fn count(&mut self, max: u32, what: &str) -> Result<usize, String> {
        let n = self.u32()?;
        if n > max {
            return Err(format!("Слишком много элементов ({}): {}", what, n));
        }
        Ok(n as usize)
    }
}

// Реэкспорт для read.rs/write.rs.
pub(super) const FRAME_LIMIT: u32 = MAX_FRAMES;
pub(super) const SOCKET_LIMIT: u32 = MAX_SOCKETS;
pub(super) const OPTION_LIMIT: u32 = MAX_OPTIONS;
pub(super) const ZONE_LIMIT: u32 = MAX_ZONES;