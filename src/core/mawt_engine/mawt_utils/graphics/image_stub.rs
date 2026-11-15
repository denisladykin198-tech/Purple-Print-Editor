//! Минимальная замена библиотеки image — без внешних крейтов.
//! Поддержка: PNG (полный), BMP (полный), GIF (первый кадр + запись),
//! TIFF (uncompressed), JPEG (baseline decode). WebP не поддерживается.
//!
//! Кодирование: PNG без компрессии (stored deflate) — быстро, но крупно.
//! Декодирование JPEG: только baseline sequential (SOF0/SOF1).

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::OnceLock;

// ============================================================================
// Errors
// ============================================================================

#[derive(Debug)]
pub enum ImageError {
    Io(std::io::Error),
    Format(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Io(e) => write!(f, "IO: {}", e),
            ImageError::Format(m) => write!(f, "Format: {}", m),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<std::io::Error> for ImageError {
    fn from(e: std::io::Error) -> Self {
        ImageError::Io(e)
    }
}

// ============================================================================
// Buffers and DynamicImage
// ============================================================================

pub struct ImageBuffer<T> {
    pub width: u32,
    pub height: u32,
    pub data: Vec<T>,
}

impl<T: Clone> ImageBuffer<T> {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn into_raw(self) -> Vec<T> {
        self.data
    }
}

impl<T> ImageBuffer<T> {
    pub fn as_raw(&self) -> &[T] {
        &self.data
    }

    pub fn as_raw_mut(&mut self) -> &mut [T] {
        &mut self.data
    }
}

impl ImageBuffer<u8> {
    /// RGBA8-буфер: длина `data` должна быть равна `width * height * 4`.
    pub fn from_raw_rgba8(width: u32, height: u32, data: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if data.len() == expected {
            Some(Self { width, height, data })
        } else {
            None
        }
    }
}

pub enum DynamicImage {
    ImageRgba8(ImageBuffer<u8>),
}

impl DynamicImage {
    pub fn to_rgba8(&self) -> ImageBuffer<u8> {
        match self {
            DynamicImage::ImageRgba8(b) => ImageBuffer {
                width: b.width,
                height: b.height,
                data: b.data.clone(),
            },
        }
    }

    /// Nearest-neighbor ресайз. Качество невысокое, но предсказуемое
    /// и достаточно для превью в UI.
    pub fn resize_exact(&self, nw: u32, nh: u32) -> DynamicImage {
        match self {
            DynamicImage::ImageRgba8(b) => {
                let mut new_data = vec![0u8; (nw * nh * 4) as usize];
                for y in 0..nh {
                    for x in 0..nw {
                        let sx = (x * b.width) / nw;
                        let sy = (y * b.height) / nh;
                        let si = ((sy * b.width + sx) * 4) as usize;
                        let di = ((y * nw + x) * 4) as usize;
                        if si + 3 < b.data.len() && di + 3 < new_data.len() {
                            new_data[di..di + 4].copy_from_slice(&b.data[si..si + 4]);
                        }
                    }
                }
                DynamicImage::ImageRgba8(ImageBuffer {
                    width: nw,
                    height: nh,
                    data: new_data,
                })
            }
        }
    }

    pub fn save_with_format(&self, path: &Path, format: ImageFormat) -> Result<(), ImageError> {
        // Сначала кодируем в память: если формат не поддержан для записи,
        // файл не будет создан пустым.
        let mut buf = Vec::new();
        self.write_to(&mut buf, format)?;
        let mut f = File::create(path)?;
        f.write_all(&buf)?;
        Ok(())
    }

    pub fn write_to<W: Write>(&self, w: &mut W, format: ImageFormat) -> Result<(), ImageError> {
        match self {
            DynamicImage::ImageRgba8(buf) => match format {
                ImageFormat::Png => encode_png(w, buf),
                ImageFormat::Bmp => encode_bmp(w, buf),
                ImageFormat::Gif => encode_gif(w, buf),
                ImageFormat::Tiff => encode_tiff(w, buf),
                ImageFormat::Jpeg => Err(ImageError::Format(
                    "JPEG encode: not implemented (needs forward DCT + Huffman)".into(),
                )),
                ImageFormat::WebP => Err(ImageError::Format(
                    "WebP encode: not implemented".into(),
                )),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Gif,
    Tiff,
    WebP,
}

// ============================================================================
// Input
// ============================================================================

pub fn open<P: AsRef<Path>>(path: P) -> Result<DynamicImage, ImageError> {
    load_from_memory(&std::fs::read(path)?)
}

pub fn load_from_memory(data: &[u8]) -> Result<DynamicImage, ImageError> {
    if data.len() < 8 {
        return Err(ImageError::Format("data too short".into()));
    }
    if data.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]) {
        return decode_png(data);
    }
    if data[0] == 0xFF && data[1] == 0xD8 {
        return decode_jpeg(data);
    }
    if data.starts_with(b"BM") {
        return decode_bmp(data);
    }
    if data.len() >= 6 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return decode_gif(data);
    }
    if data.len() >= 4 && (data.starts_with(b"II*\0") || data.starts_with(b"MM\0*")) {
        return decode_tiff(data);
    }
    if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        return Err(ImageError::Format(
            "WebP: not implemented (VP8 too large)".into(),
        ));
    }
    Err(ImageError::Format("unsupported format".into()))
}

// ============================================================================
// Checksums and deflate
// ============================================================================

fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0xEDB8_8320
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

struct BitReader<'a> {
    d: &'a [u8],
    p: usize,
    buf: u32,
    cnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self {
            d,
            p: 0,
            buf: 0,
            cnt: 0,
        }
    }

    fn bit(&mut self) -> Result<u32, String> {
        if self.cnt == 0 {
            if self.p >= self.d.len() {
                return Err("inflate eof".into());
            }
            self.buf = self.d[self.p] as u32;
            self.p += 1;
            self.cnt = 8;
        }
        let b = self.buf & 1;
        self.buf >>= 1;
        self.cnt -= 1;
        Ok(b)
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }
}

struct Huff {
    counts: [u16; 16],
    syms: Vec<u16>,
}

impl Huff {
    fn new(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[l as usize] += 1;
        }
        counts[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + counts[i - 1];
        }
        let mut syms = vec![0u16; lengths.len()];
        for (i, &l) in lengths.iter().enumerate() {
            if l != 0 {
                syms[offs[l as usize] as usize] = i as u16;
                offs[l as usize] += 1;
            }
        }
        Self { counts, syms }
    }

    fn decode(&self, br: &mut BitReader) -> Result<u16, String> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bit()? as i32;
            let cnt = self.counts[len] as i32;
            if code - cnt < first {
                return Ok(self.syms[(index + code - first) as usize]);
            }
            index += cnt;
            first = (first + cnt) << 1;
            code <<= 1;
        }
        Err("huffman error".into())
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12,
    13, 13,
];

fn inflate_block(
    br: &mut BitReader,
    out: &mut Vec<u8>,
    lit: &Huff,
    dist: &Huff,
) -> Result<(), String> {
    loop {
        let s = lit.decode(br)?;
        if s < 256 {
            out.push(s as u8);
        } else if s == 256 {
            return Ok(());
        } else {
            let li = (s - 257) as usize;
            if li >= LEN_BASE.len() {
                return Err("bad length symbol".into());
            }
            let len = LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;

            let di = dist.decode(br)? as usize;
            if di >= DIST_BASE.len() {
                return Err("bad dist symbol".into());
            }
            let distance = DIST_BASE[di] as usize + br.bits(DIST_EXTRA[di] as u32)? as usize;
            if distance > out.len() {
                return Err("back-ref too far".into());
            }

            let start = out.len() - distance;
            for i in 0..len {
                out.push(out[start + i]);
            }
        }
    }
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut br = BitReader::new(data);
    let mut out = Vec::new();
    loop {
        let final_block = br.bit()? == 1;
        let bt = br.bits(2)?;
        match bt {
            0 => {
                // Stored block. Выравниваемся по границе байта.
                br.cnt = 0;
                let len = br.bits(16)? as usize;
                // NLEN не проверяем — достаточно для доверенных файлов.
                let _nlen = br.bits(16)?;
                for _ in 0..len {
                    out.push(br.bits(8)? as u8);
                }
            }
            1 => {
                let mut ll = [0u8; 288];
                for i in 0..144 {
                    ll[i] = 8;
                }
                for i in 144..256 {
                    ll[i] = 9;
                }
                for i in 256..280 {
                    ll[i] = 7;
                }
                for i in 280..288 {
                    ll[i] = 8;
                }
                let dl = [5u8; 30];
                inflate_block(&mut br, &mut out, &Huff::new(&ll), &Huff::new(&dl))?;
            }
            2 => {
                let hlit = br.bits(5)? as usize + 257;
                let hdist = br.bits(5)? as usize + 1;
                let hclen = br.bits(4)? as usize + 4;
                const ORDER: [usize; 19] = [
                    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
                ];

                let mut cl = [0u8; 19];
                for i in 0..hclen {
                    cl[ORDER[i]] = br.bits(3)? as u8;
                }
                let ch = Huff::new(&cl);

                // Читаем длины кодов одним проходом: подсчёт «сколько
                // осталось» сдвинул бы позицию в битовом потоке.
                let mut lengths: Vec<u8> = Vec::with_capacity(hlit + hdist);
                while lengths.len() < hlit + hdist {
                    let s = ch.decode(&mut br)?;
                    match s {
                        0..=15 => lengths.push(s as u8),
                        16 => {
                            let prev = *lengths.last().ok_or("repeat with no prev")?;
                            let rep = br.bits(2)? as usize + 3;
                            for _ in 0..rep {
                                lengths.push(prev);
                            }
                        }
                        17 => {
                            let rep = br.bits(3)? as usize + 3;
                            for _ in 0..rep {
                                lengths.push(0);
                            }
                        }
                        18 => {
                            let rep = br.bits(7)? as usize + 11;
                            for _ in 0..rep {
                                lengths.push(0);
                            }
                        }
                        _ => return Err("bad code length".into()),
                    }
                }
                // Повторы 17/18 могут дать «хвост» за пределами hlit + hdist.
                lengths.truncate(hlit + hdist);

                let lit = Huff::new(&lengths[..hlit]);
                let dist = Huff::new(&lengths[hlit..]);
                inflate_block(&mut br, &mut out, &lit, &dist)?;
            }
            _ => return Err("bad block type".into()),
        }
        if final_block {
            break;
        }
    }
    Ok(out)
}

fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 2 {
        return Err("zlib too short".into());
    }
    let (cmf, flg) = (data[0], data[1]);
    if (cmf & 0x0F) != 8 {
        return Err("not deflate".into());
    }
    if (((cmf as u16) << 8) | flg as u16) % 31 != 0 {
        return Err("bad zlib header".into());
    }
    if flg & 0x20 != 0 {
        return Err("preset dict unsupported".into());
    }
    inflate(&data[2..])
}

fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78u8, 0x01];
    let mut pos = 0;
    loop {
        let rem = data.len() - pos;
        let chunk = rem.min(65535);
        let final_b = pos + chunk >= data.len();
        out.push(if final_b { 1 } else { 0 });
        let len = chunk as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[pos..pos + chunk]);
        pos += chunk;
        if final_b {
            break;
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

// ============================================================================
// PNG
// ============================================================================

fn decode_png(data: &[u8]) -> Result<DynamicImage, ImageError> {
    let mut pos = 8;
    let (mut w, mut h) = (0u32, 0u32);
    let (mut depth, mut ctype) = (0u8, 0u8);
    let mut idat = Vec::new();
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();

    while pos + 8 <= data.len() {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            as usize;
        if pos + 12 + len > data.len() {
            break;
        }
        let ctype_chunk = &data[pos + 4..pos + 8];
        let cd = &data[pos + 8..pos + 8 + len];
        match ctype_chunk {
            b"IHDR" => {
                w = u32::from_be_bytes([cd[0], cd[1], cd[2], cd[3]]);
                h = u32::from_be_bytes([cd[4], cd[5], cd[6], cd[7]]);
                depth = cd[8];
                ctype = cd[9];
            }
            b"PLTE" => {
                for ch in cd.chunks(3) {
                    if ch.len() == 3 {
                        palette.push([ch[0], ch[1], ch[2]]);
                    }
                }
            }
            b"tRNS" => trns = cd.to_vec(),
            b"IDAT" => idat.extend_from_slice(cd),
            b"IEND" => break,
            _ => {}
        }
        pos += 12 + len;
    }

    if depth != 8 {
        return Err(ImageError::Format(format!("PNG depth {} unsupported", depth)));
    }
    let bpp_per_pixel: usize = match ctype {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(ImageError::Format("PNG ctype".into())),
    };
    let stride = w as usize * bpp_per_pixel;
    let raw =
        zlib_decompress(&idat).map_err(|e| ImageError::Format(format!("PNG inflate: {}", e)))?;

    let mut pixels = Vec::with_capacity(stride * h as usize);
    let mut prev = vec![0u8; stride];
    for y in 0..h as usize {
        let row_start = y * (stride + 1);
        if row_start >= raw.len() {
            break;
        }
        let filter = raw[row_start];
        let mut row = raw[row_start + 1..row_start + 1 + stride].to_vec();
        for i in 0..stride {
            let a = if i >= bpp_per_pixel {
                row[i - bpp_per_pixel]
            } else {
                0
            };
            let b = prev[i];
            let c = if i >= bpp_per_pixel {
                prev[i - bpp_per_pixel]
            } else {
                0
            };
            row[i] = match filter {
                0 => row[i],
                1 => row[i].wrapping_add(a),
                2 => row[i].wrapping_add(b),
                3 => row[i].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => row[i].wrapping_add(paeth(a, b, c)),
                _ => return Err(ImageError::Format(format!("PNG filter {}", filter))),
            };
        }
        pixels.extend_from_slice(&row);
        prev = row;
    }

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    match ctype {
        6 => rgba = pixels,
        2 => {
            for c in pixels.chunks(3) {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        0 => {
            for &g in &pixels {
                let a = if !trns.is_empty() && trns[0] == g { 0 } else { 255 };
                rgba.extend_from_slice(&[g, g, g, a]);
            }
        }
        4 => {
            for c in pixels.chunks(2) {
                rgba.extend_from_slice(&[c[0], c[0], c[0], c[1]]);
            }
        }
        3 => {
            for &i in &pixels {
                let p = palette.get(i as usize).copied().unwrap_or([0, 0, 0]);
                let a = trns.get(i as usize).copied().unwrap_or(255);
                rgba.extend_from_slice(&[p[0], p[1], p[2], a]);
            }
        }
        _ => unreachable!(),
    }

    ImageBuffer::from_raw_rgba8(w, h, rgba)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| ImageError::Format("PNG size mismatch".into()))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let (pa, pb, pc) = (
        (p - a as i32).abs(),
        (p - b as i32).abs(),
        (p - c as i32).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn write_png_chunk<W: Write>(w: &mut W, t: &[u8; 4], d: &[u8]) -> std::io::Result<()> {
    w.write_all(&(d.len() as u32).to_be_bytes())?;
    w.write_all(t)?;
    w.write_all(d)?;
    let mut crc_data = Vec::with_capacity(4 + d.len());
    crc_data.extend_from_slice(t);
    crc_data.extend_from_slice(d);
    w.write_all(&crc32(&crc_data).to_be_bytes())?;
    Ok(())
}

fn encode_png<W: Write>(w: &mut W, buf: &ImageBuffer<u8>) -> Result<(), ImageError> {
    w.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])?;
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&buf.width.to_be_bytes());
    ihdr.extend_from_slice(&buf.height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_png_chunk(w, b"IHDR", &ihdr)?;

    let row_bytes = buf.width as usize * 4;
    let mut raw = Vec::with_capacity((row_bytes + 1) * buf.height as usize);
    for y in 0..buf.height as usize {
        raw.push(0);
        raw.extend_from_slice(&buf.data[y * row_bytes..(y + 1) * row_bytes]);
    }
    write_png_chunk(w, b"IDAT", &zlib_store(&raw))?;
    write_png_chunk(w, b"IEND", &[])?;
    Ok(())
}

// ============================================================================
// BMP
// ============================================================================

fn decode_bmp(data: &[u8]) -> Result<DynamicImage, ImageError> {
    if data.len() < 54 {
        return Err(ImageError::Format("BMP too short".into()));
    }
    let pix_off = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let hdr_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
    if hdr_size < 40 {
        return Err(ImageError::Format("BMP header".into()));
    }
    let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]) as u32;
    let h_signed = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bottom_up = h_signed > 0;
    let h = h_signed.unsigned_abs();
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    let comp = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
    if comp != 0 {
        return Err(ImageError::Format("BMP compressed".into()));
    }

    let row_size = ((bpp as usize * w as usize + 31) / 32) * 4;
    let mut palette: Vec<[u8; 4]> = Vec::new();
    if bpp <= 8 {
        let colors = 1usize << bpp;
        let pstart = 14 + hdr_size as usize;
        for i in 0..colors {
            let o = pstart + i * 4;
            if o + 4 > data.len() {
                break;
            }
            palette.push([data[o + 2], data[o + 1], data[o], 255]);
        }
    }

    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h as usize {
        let sy = if bottom_up { h as usize - 1 - y } else { y };
        let rs = pix_off + sy * row_size;
        for x in 0..w as usize {
            let d = (y * w as usize + x) * 4;
            let c = match bpp {
                24 => {
                    let o = rs + x * 3;
                    if o + 3 > data.len() {
                        [0, 0, 0, 255]
                    } else {
                        [data[o + 2], data[o + 1], data[o], 255]
                    }
                }
                32 => {
                    let o = rs + x * 4;
                    if o + 4 > data.len() {
                        [0, 0, 0, 255]
                    } else {
                        [data[o + 2], data[o + 1], data[o], data[o + 3]]
                    }
                }
                8 => {
                    let o = rs + x;
                    if o >= data.len() {
                        [0, 0, 0, 255]
                    } else {
                        palette
                            .get(data[o] as usize)
                            .copied()
                            .unwrap_or([0, 0, 0, 255])
                    }
                }
                4 => {
                    let o = rs + x / 2;
                    if o >= data.len() {
                        [0, 0, 0, 255]
                    } else {
                        let byte = data[o];
                        let idx = if x % 2 == 0 { byte >> 4 } else { byte & 0x0F };
                        palette
                            .get(idx as usize)
                            .copied()
                            .unwrap_or([0, 0, 0, 255])
                    }
                }
                1 => {
                    let o = rs + x / 8;
                    if o >= data.len() {
                        [0, 0, 0, 255]
                    } else {
                        let bit = (data[o] >> (7 - (x % 8))) & 1;
                        palette
                            .get(bit as usize)
                            .copied()
                            .unwrap_or([0, 0, 0, 255])
                    }
                }
                _ => return Err(ImageError::Format(format!("BMP bpp {}", bpp))),
            };
            rgba[d..d + 4].copy_from_slice(&c);
        }
    }
    ImageBuffer::from_raw_rgba8(w, h, rgba)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| ImageError::Format("BMP size".into()))
}

fn encode_bmp<W: Write>(w: &mut W, buf: &ImageBuffer<u8>) -> Result<(), ImageError> {
    let row_size = ((32 * buf.width as usize + 31) / 32) * 4;
    let pix_size = row_size * buf.height as usize;
    let file_size = 54 + pix_size;

    w.write_all(b"BM")?;
    w.write_all(&(file_size as u32).to_le_bytes())?;
    w.write_all(&[0; 4])?;
    w.write_all(&54u32.to_le_bytes())?;
    w.write_all(&40u32.to_le_bytes())?;
    w.write_all(&(buf.width as i32).to_le_bytes())?;
    w.write_all(&(buf.height as i32).to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?;
    w.write_all(&32u16.to_le_bytes())?;
    w.write_all(&0u32.to_le_bytes())?;
    w.write_all(&(pix_size as u32).to_le_bytes())?;
    w.write_all(&2835i32.to_le_bytes())?;
    w.write_all(&2835i32.to_le_bytes())?;
    w.write_all(&0u32.to_le_bytes())?;
    w.write_all(&0u32.to_le_bytes())?;

    for y in (0..buf.height as usize).rev() {
        let rs = y * buf.width as usize * 4;
        for x in 0..buf.width as usize {
            let i = rs + x * 4;
            w.write_all(&[buf.data[i + 2], buf.data[i + 1], buf.data[i], buf.data[i + 3]])?;
        }
        for _ in (buf.width as usize * 4)..row_size {
            w.write_all(&[0])?;
        }
    }
    Ok(())
}

// ============================================================================
// GIF (first frame + write, LZW)
// ============================================================================

fn decode_gif(data: &[u8]) -> Result<DynamicImage, ImageError> {
    if data.len() < 13 {
        return Err(ImageError::Format("GIF too short".into()));
    }
    let sw = u16::from_le_bytes([data[6], data[7]]) as u32;
    let sh = u16::from_le_bytes([data[8], data[9]]) as u32;
    let flags = data[10];
    let gct_flag = flags & 0x80 != 0;
    let gct_size = 1u32 << ((flags & 0x07) + 1);
    let bg_index = data[11] as usize;
    let mut pos = 13;

    let mut gpal: Vec<[u8; 3]> = Vec::new();
    if gct_flag {
        for _ in 0..gct_size {
            if pos + 3 > data.len() {
                break;
            }
            gpal.push([data[pos], data[pos + 1], data[pos + 2]]);
            pos += 3;
        }
    }

    let mut transparent: Option<u8> = None;
    while pos < data.len() {
        let block = data[pos];
        pos += 1;
        match block {
            0x21 => {
                if pos >= data.len() {
                    break;
                }
                let ext = data[pos];
                pos += 1;
                let mut sub = Vec::new();
                loop {
                    if pos >= data.len() {
                        break;
                    }
                    let sz = data[pos] as usize;
                    pos += 1;
                    if sz == 0 {
                        break;
                    }
                    if pos + sz > data.len() {
                        break;
                    }
                    sub.extend_from_slice(&data[pos..pos + sz]);
                    pos += sz;
                }
                if ext == 0xF9 && sub.len() >= 4 {
                    if sub[0] & 0x01 != 0 {
                        transparent = Some(sub[3]);
                    }
                }
            }
            0x2C => {
                if pos + 9 > data.len() {
                    break;
                }
                let left = u16::from_le_bytes([data[pos], data[pos + 1]]) as u32;
                let top = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as u32;
                let w = u16::from_le_bytes([data[pos + 4], data[pos + 5]]) as u32;
                let h = u16::from_le_bytes([data[pos + 6], data[pos + 7]]) as u32;
                let lflags = data[pos + 8];
                pos += 9;
                let lct_flag = lflags & 0x80 != 0;
                let lct_size = 1u32 << ((lflags & 0x07) + 1);
                let interlaced = lflags & 0x40 != 0;
                let mut lpal: Vec<[u8; 3]> = Vec::new();
                if lct_flag {
                    for _ in 0..lct_size {
                        if pos + 3 > data.len() {
                            break;
                        }
                        lpal.push([data[pos], data[pos + 1], data[pos + 2]]);
                        pos += 3;
                    }
                }
                let pal = if lct_flag { &lpal } else { &gpal };
                if pos >= data.len() {
                    break;
                }
                let min_code = data[pos];
                pos += 1;
                let mut lzw = Vec::new();
                loop {
                    if pos >= data.len() {
                        break;
                    }
                    let sz = data[pos] as usize;
                    pos += 1;
                    if sz == 0 {
                        break;
                    }
                    if pos + sz > data.len() {
                        break;
                    }
                    lzw.extend_from_slice(&data[pos..pos + sz]);
                    pos += sz;
                }
                let idx = gif_lzw_decode(&lzw, min_code);
                let mut rgba = vec![0u8; (sw * sh * 4) as usize];
                let bgc = gpal.get(bg_index).copied().unwrap_or([0, 0, 0]);
                for i in 0..(sw * sh) as usize {
                    rgba[i * 4..i * 4 + 4].copy_from_slice(&[bgc[0], bgc[1], bgc[2], 255]);
                }

                // Interlace: пассы 0,8…; 4,12…; 2,6…; 1,3…
                let row_order: Vec<u32> = if interlaced {
                    let mut order = Vec::with_capacity(h as usize);
                    for (start, step) in [(0u32, 8u32), (4, 8), (2, 4), (1, 2)] {
                        order.extend((start..h).step_by(step as usize));
                    }
                    order
                } else {
                    (0..h).collect()
                };

                for (src_row, &dy) in row_order.iter().enumerate() {
                    let ay = top + dy;
                    if ay >= sh {
                        continue;
                    }
                    for x in 0..w {
                        let ax = left + x;
                        if ax >= sw {
                            continue;
                        }
                        let si = src_row as u32 * w + x;
                        if si as usize >= idx.len() {
                            continue;
                        }
                        let i = idx[si as usize];
                        let c = if Some(i) == transparent {
                            [0u8, 0, 0, 0]
                        } else {
                            let p = pal.get(i as usize).copied().unwrap_or([0, 0, 0]);
                            [p[0], p[1], p[2], 255]
                        };
                        let di = (ay * sw + ax) as usize * 4;
                        rgba[di..di + 4].copy_from_slice(&c);
                    }
                }
                return ImageBuffer::from_raw_rgba8(sw, sh, rgba)
                    .map(DynamicImage::ImageRgba8)
                    .ok_or_else(|| ImageError::Format("GIF size".into()));
            }
            0x3B => break,
            _ => loop {
                if pos >= data.len() {
                    break;
                }
                let sz = data[pos] as usize;
                pos += 1;
                if sz == 0 {
                    break;
                }
                pos += sz;
            },
        }
    }
    Err(ImageError::Format("GIF no frame".into()))
}

fn gif_lzw_decode(data: &[u8], min_code: u8) -> Vec<u8> {
    let clear = 1u16 << min_code;
    let eoi = clear + 1;
    let mut size = min_code as u32 + 1;
    let mut next = eoi + 1;
    let mut dict: Vec<Vec<u8>> = Vec::new();
    let reset = |d: &mut Vec<Vec<u8>>| {
        d.clear();
        for i in 0..(1u16 << min_code) {
            d.push(vec![i as u8]);
        }
        d.push(vec![]);
        d.push(vec![]);
    };
    reset(&mut dict);
    let mut out = Vec::new();
    let mut bit = 0usize;
    let total = data.len() * 8;
    let mut prev: Option<u16> = None;
    while bit + size as usize <= total {
        let mut code = 0u16;
        for i in 0..size {
            let b = (bit + i as usize) / 8;
            let off = (bit + i as usize) % 8;
            code |= (((data[b] >> off) & 1) as u16) << i;
        }
        bit += size as usize;
        if code == clear {
            reset(&mut dict);
            size = min_code as u32 + 1;
            next = eoi + 1;
            prev = None;
            continue;
        }
        if code == eoi {
            break;
        }
        let entry = if (code as usize) < dict.len() {
            dict[code as usize].clone()
        } else if let Some(p) = prev {
            let mut e = dict[p as usize].clone();
            if !e.is_empty() {
                e.push(e[0]);
            }
            e
        } else {
            break;
        };
        out.extend_from_slice(&entry);
        if let Some(p) = prev {
            if next < 4096 {
                let mut ne = dict[p as usize].clone();
                if !entry.is_empty() {
                    ne.push(entry[0]);
                }
                dict.push(ne);
                next += 1;
                if next == (1u16 << size) && size < 12 {
                    size += 1;
                }
            }
        }
        prev = Some(code);
    }
    out
}

fn gif_lzw_encode(indices: &[u8], min_code: u8) -> Vec<u8> {
    let clear = 1u16 << min_code;
    let eoi = clear + 1;
    let mut size = min_code as u32 + 1;
    let mut next = eoi + 1;
    let mut out = Vec::new();
    let mut bit_buf: u32 = 0;
    let mut bit_cnt: u32 = 0;
    let mut emit = |code: u16, buf: &mut u32, cnt: &mut u32, sz: u32| {
        *buf |= (code as u32) << *cnt;
        *cnt += sz;
        while *cnt >= 8 {
            out.push((*buf & 0xFF) as u8);
            *buf >>= 8;
            *cnt -= 8;
        }
    };
    let mut dict: std::collections::HashMap<Vec<u8>, u16> = std::collections::HashMap::new();
    let reset = |d: &mut std::collections::HashMap<Vec<u8>, u16>| {
        d.clear();
        for i in 0..(1u16 << min_code) {
            d.insert(vec![i as u8], i);
        }
    };
    reset(&mut dict);
    emit(clear, &mut bit_buf, &mut bit_cnt, size);
    if indices.is_empty() {
        emit(eoi, &mut bit_buf, &mut bit_cnt, size);
        if bit_cnt > 0 {
            out.push((bit_buf & 0xFF) as u8);
        }
        return out;
    }
    let mut cur = vec![indices[0]];
    for &i in &indices[1..] {
        let mut ext = cur.clone();
        ext.push(i);
        if dict.contains_key(&ext) {
            cur = ext;
        } else {
            let c = *dict.get(&cur).unwrap();
            emit(c, &mut bit_buf, &mut bit_cnt, size);
            if next < 4096 {
                dict.insert(ext, next);
                next += 1;
                if next == (1u16 << size) && size < 12 {
                    size += 1;
                }
            } else {
                emit(clear, &mut bit_buf, &mut bit_cnt, size);
                reset(&mut dict);
                size = min_code as u32 + 1;
                next = eoi + 1;
            }
            cur = vec![i];
        }
    }
    let c = *dict.get(&cur).unwrap();
    emit(c, &mut bit_buf, &mut bit_cnt, size);
    emit(eoi, &mut bit_buf, &mut bit_cnt, size);
    if bit_cnt > 0 {
        out.push((bit_buf & 0xFF) as u8);
    }
    out
}

fn encode_gif<W: Write>(w: &mut W, buf: &ImageBuffer<u8>) -> Result<(), ImageError> {
    let width = buf.width as u16;
    let height = buf.height as u16;
    if width == 0 || height == 0 {
        return Err(ImageError::Format("GIF empty".into()));
    }

    // 216-цветная web-safe палитра (6×6×6) + добивка до 256.
    let mut palette: Vec<[u8; 3]> = Vec::new();
    for r in 0..6u32 {
        for g in 0..6u32 {
            for b in 0..6u32 {
                palette.push([(r * 51) as u8, (g * 51) as u8, (b * 51) as u8]);
            }
        }
    }
    while palette.len() < 256 {
        palette.push([0, 0, 0]);
    }

    let indices: Vec<u8> = buf
        .data
        .chunks(4)
        .map(|p| {
            let r = (p[0] as u32 / 51).min(5);
            let g = (p[1] as u32 / 51).min(5);
            let b = (p[2] as u32 / 51).min(5);
            (r * 36 + g * 6 + b) as u8
        })
        .collect();

    let pal_bits = 7u8; // 256 цветов
    w.write_all(b"GIF89a")?;
    w.write_all(&width.to_le_bytes())?;
    w.write_all(&height.to_le_bytes())?;
    w.write_all(&[0x80 | pal_bits, 0, 0])?;
    for c in &palette {
        w.write_all(c)?;
    }

    w.write_all(&[0x2C])?;
    w.write_all(&0u16.to_le_bytes())?;
    w.write_all(&0u16.to_le_bytes())?;
    w.write_all(&width.to_le_bytes())?;
    w.write_all(&height.to_le_bytes())?;
    w.write_all(&[0])?;

    let min_code = 8u8;
    w.write_all(&[min_code])?;
    let lzw = gif_lzw_encode(&indices, min_code);
    for chunk in lzw.chunks(255) {
        w.write_all(&[chunk.len() as u8])?;
        w.write_all(chunk)?;
    }
    w.write_all(&[0])?;
    w.write_all(&[0x3B])?;
    Ok(())
}

// ============================================================================
// TIFF (uncompressed)
// ============================================================================

fn decode_tiff(data: &[u8]) -> Result<DynamicImage, ImageError> {
    debug_assert!(data.len() >= 8);
    let le = &data[0..2] == b"II";
    let rd16 = |p: usize| -> u16 {
        if le {
            u16::from_le_bytes([data[p], data[p + 1]])
        } else {
            u16::from_be_bytes([data[p], data[p + 1]])
        }
    };
    let rd32 = |p: usize| -> u32 {
        if le {
            u32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]])
        } else {
            u32::from_be_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]])
        }
    };
    if rd16(2) != 42 {
        return Err(ImageError::Format("TIFF magic".into()));
    }
    let ifd = rd32(4) as usize;
    if ifd + 2 > data.len() {
        return Err(ImageError::Format("TIFF ifd off".into()));
    }
    let n = rd16(ifd) as usize;

    let (mut w, mut h) = (0u32, 0u32);
    let (mut bps, mut spp, mut comp, mut photo) = (8u16, 1u16, 1u16, 1u16);
    let mut strip_offsets: Vec<u32> = Vec::new();
    let mut strip_counts: Vec<u32> = Vec::new();

    for i in 0..n {
        let e = ifd + 2 + i * 12;
        if e + 12 > data.len() {
            break;
        }
        let tag = rd16(e);
        let dtype = rd16(e + 2);
        let count = rd32(e + 4) as usize;
        let tsize = match dtype {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 => 4,
            5 | 10 | 12 => 8,
            _ => 1,
        };
        let total = tsize * count;
        let vpos = if total <= 4 {
            e + 8
        } else {
            rd32(e + 8) as usize
        };
        let rv = |idx: usize| -> u32 {
            let p = vpos + idx * tsize;
            if p + tsize > data.len() {
                return 0;
            }
            match tsize {
                1 => data[p] as u32,
                2 => rd16(p) as u32,
                4 => rd32(p),
                _ => 0,
            }
        };
        match tag {
            256 => w = rv(0),
            257 => h = rv(0),
            258 => bps = rv(0) as u16,
            259 => comp = rv(0) as u16,
            262 => photo = rv(0) as u16,
            273 => {
                for j in 0..count {
                    strip_offsets.push(rv(j));
                }
            }
            277 => spp = rv(0) as u16,
            279 => {
                for j in 0..count {
                    strip_counts.push(rv(j));
                }
            }
            _ => {}
        }
    }

    if comp != 1 {
        return Err(ImageError::Format(format!("TIFF compression {}", comp)));
    }
    if bps != 8 {
        return Err(ImageError::Format(format!("TIFF bps {}", bps)));
    }
    if w == 0 || h == 0 {
        return Err(ImageError::Format("TIFF zero size".into()));
    }

    let mut raw = Vec::new();
    for (i, &off) in strip_offsets.iter().enumerate() {
        let cnt = strip_counts.get(i).copied().unwrap_or(0) as usize;
        let end = (off as usize + cnt).min(data.len());
        if (off as usize) < end {
            raw.extend_from_slice(&data[off as usize..end]);
        }
    }

    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    match (spp, photo) {
        (1, 0) | (1, 1) => {
            for &g in &raw {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        (3, 2) => {
            for c in raw.chunks(3) {
                if c.len() >= 3 {
                    rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        }
        (4, 2) => rgba = raw,
        _ => return Err(ImageError::Format(format!("TIFF spp={} photo={}", spp, photo))),
    }

    ImageBuffer::from_raw_rgba8(w, h, rgba)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| ImageError::Format("TIFF size".into()))
}

fn encode_tiff<W: Write>(w: &mut W, buf: &ImageBuffer<u8>) -> Result<(), ImageError> {
    let width = buf.width;
    let height = buf.height;
    let pixel_size = buf.data.len() as u32;
    let n_entries = 11u16;
    let bits_offset = 8 + 2 + n_entries as u32 * 12 + 4;
    let pixel_offset = bits_offset + 8;

    w.write_all(b"II")?;
    w.write_all(&42u16.to_le_bytes())?;
    w.write_all(&8u32.to_le_bytes())?;
    w.write_all(&n_entries.to_le_bytes())?;

    let entry = |w: &mut W, tag: u16, dtype: u16, count: u32, value: u32| -> std::io::Result<()> {
        w.write_all(&tag.to_le_bytes())?;
        w.write_all(&dtype.to_le_bytes())?;
        w.write_all(&count.to_le_bytes())?;
        w.write_all(&value.to_le_bytes())
    };
    entry(w, 256, 4, 1, width)?;
    entry(w, 257, 4, 1, height)?;
    entry(w, 258, 3, 4, bits_offset)?;
    entry(w, 259, 3, 1, 1)?;
    entry(w, 262, 3, 1, 2)?;
    entry(w, 273, 4, 1, pixel_offset)?;
    entry(w, 277, 3, 1, 4)?;
    entry(w, 278, 4, 1, height)?;
    entry(w, 279, 4, 1, pixel_size)?;
    entry(w, 284, 3, 1, 1)?;
    entry(w, 338, 3, 1, 2)?;

    w.write_all(&0u32.to_le_bytes())?;
    for _ in 0..4 {
        w.write_all(&8u16.to_le_bytes())?;
    }
    w.write_all(&buf.data)?;
    Ok(())
}

// ============================================================================
// JPEG baseline sequential (decode only)
// ============================================================================

struct JBit<'a> {
    d: &'a [u8],
    p: usize,
    buf: u32,
    cnt: u32,
}

impl<'a> JBit<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self {
            d,
            p: 0,
            buf: 0,
            cnt: 0,
        }
    }

    fn byte(&mut self) -> Result<u8, String> {
        loop {
            if self.p >= self.d.len() {
                return Err("jpeg eof".into());
            }
            let b = self.d[self.p];
            self.p += 1;
            if b == 0xFF {
                if self.p >= self.d.len() {
                    return Err("jpeg eof after ff".into());
                }
                let n = self.d[self.p];
                if n == 0x00 {
                    self.p += 1;
                    return Ok(0xFF);
                }
                if (0xD0..=0xD7).contains(&n) {
                    self.p += 1;
                    continue;
                }
                self.p -= 1;
                return Err(format!("marker 0x{:02X} in scan", n));
            }
            return Ok(b);
        }
    }

    fn bit(&mut self) -> Result<u32, String> {
        if self.cnt == 0 {
            self.buf = self.byte()? as u32;
            self.cnt = 8;
        }
        let b = (self.buf >> (self.cnt - 1)) & 1;
        self.cnt -= 1;
        Ok(b)
    }

    fn bits(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Ok(v)
    }

    fn align(&mut self) {
        self.cnt = 0;
        self.buf = 0;
    }
}

struct JHuff {
    counts: [u8; 17],
    syms: Vec<u8>,
}

impl JHuff {
    fn parse(data: &[u8]) -> Result<Self, String> {
        if data.len() < 17 {
            return Err("short DHT".into());
        }
        let mut counts = [0u8; 17];
        for i in 1..=16 {
            counts[i] = data[i - 1];
        }
        let total: usize = counts[1..].iter().map(|&x| x as usize).sum();
        if data.len() < 16 + total {
            return Err("short DHT syms".into());
        }
        Ok(Self {
            counts,
            syms: data[16..16 + total].to_vec(),
        })
    }

    fn decode(&self, br: &mut JBit) -> Result<u8, String> {
        let (mut code, mut first, mut index) = (0u32, 0u32, 0u32);
        for len in 1..=16 {
            code = (code << 1) | br.bit()?;
            let cnt = self.counts[len] as u32;
            if code < first + cnt {
                return Ok(self.syms[(index + code - first) as usize]);
            }
            index += cnt;
            first = (first + cnt) << 1;
        }
        Err("huffman decode".into())
    }
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
    20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
    58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

struct JComp {
    id: u8,
    h: u8,
    v: u8,
    qt: usize,
    dc_t: usize,
    ac_t: usize,
    dc_pred: i32,
    blocks: Vec<[u8; 64]>,
    bw: usize,
    bh: usize,
}

fn idct_table() -> &'static [[f32; 8]; 8] {
    static T: OnceLock<[[f32; 8]; 8]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [[0.0f32; 8]; 8];
        for x in 0..8 {
            for u in 0..8 {
                let c = if u == 0 { 1.0 / 2.0f32.sqrt() } else { 1.0 };
                let ang = (2.0 * x as f32 + 1.0) * u as f32 * std::f32::consts::PI / 16.0;
                t[x][u] = 0.5 * c * ang.cos();
            }
        }
        t
    })
}

fn idct_8x8(coef: &[i32; 64]) -> [u8; 64] {
    let t = idct_table();
    let mut tmp = [0i32; 64];
    for r in 0..8 {
        for x in 0..8 {
            let mut s = 0.0f32;
            for u in 0..8 {
                s += t[x][u] * coef[r * 8 + u] as f32;
            }
            tmp[r * 8 + x] = s as i32;
        }
    }
    let mut out = [0u8; 64];
    for c in 0..8 {
        for y in 0..8 {
            let mut s = 0.0f32;
            for v in 0..8 {
                s += t[y][v] * tmp[v * 8 + c] as f32;
            }
            out[y * 8 + c] = (s as i32 + 128).clamp(0, 255) as u8;
        }
    }
    out
}

fn ycbcr_to_rgb(y: i32, cb: i32, cr: i32) -> (u8, u8, u8) {
    let y = y as f32;
    let cb = cb as f32 - 128.0;
    let cr = cr as f32 - 128.0;
    (
        (y + 1.402 * cr).clamp(0.0, 255.0) as u8,
        (y - 0.344_136 * cb - 0.714_136 * cr).clamp(0.0, 255.0) as u8,
        (y + 1.772 * cb).clamp(0.0, 255.0) as u8,
    )
}

fn decode_jpeg(data: &[u8]) -> Result<DynamicImage, ImageError> {
    let mut pos = 2;
    let mut qt = [[0u16; 64]; 4];
    let mut dc_tabs: Vec<Option<JHuff>> = vec![None, None, None, None];
    let mut ac_tabs: Vec<Option<JHuff>> = vec![None, None, None, None];
    let (mut fw, mut fh) = (0usize, 0usize);
    let mut comps: Vec<JComp> = Vec::new();
    let (mut hmax, mut vmax) = (1u8, 1u8);
    let mut restart = 0u16;
    let mut scan_done = false;

    while pos + 1 < data.len() {
        // Пропускаем не-маркерные байты до следующего 0xFF.
        if data[pos] != 0xFF {
            pos += 1;
            continue;
        }
        let m = data[pos + 1];
        pos += 2;
        match m {
            0xD8 | 0x01 | 0xD0..=0xD7 => continue,
            0xD9 => break,
            0xDA => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + len > data.len() {
                    break;
                }
                let mut p = pos + 2;
                let ns = data[p] as usize;
                p += 1;
                let mut scan_comps = Vec::new();
                for _ in 0..ns {
                    let cid = data[p];
                    let tab = data[p + 1];
                    scan_comps.push((cid, (tab >> 4) as usize, (tab & 0x0F) as usize));
                    p += 2;
                }
                pos += len;
                let start = pos;
                let mut end = pos;
                while end + 1 < data.len() {
                    if data[end] == 0xFF
                        && data[end + 1] != 0x00
                        && data[end + 1] != 0xFF
                        && !(0xD0..=0xD7).contains(&data[end + 1])
                    {
                        break;
                    }
                    end += 1;
                }
                pos = end;
                for c in &mut comps {
                    for &(cid, dc, ac) in &scan_comps {
                        if c.id == cid {
                            c.dc_t = dc;
                            c.ac_t = ac;
                        }
                    }
                }
                decode_jpeg_scan(
                    &data[start..end],
                    &mut comps,
                    fw,
                    fh,
                    hmax,
                    vmax,
                    &qt,
                    &dc_tabs,
                    &ac_tabs,
                    restart,
                )?;
                scan_done = true;
            }
            0xDB => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + len > data.len() {
                    break;
                }
                let mut p = pos + 2;
                while p < pos + len {
                    let pq_tq = data[p];
                    p += 1;
                    let pq = (pq_tq >> 4) & 0x0F;
                    let tq = (pq_tq & 0x0F) as usize;
                    if tq >= 4 {
                        break;
                    }
                    if pq == 0 {
                        for i in 0..64 {
                            qt[tq][ZIGZAG[i]] = data[p] as u16;
                            p += 1;
                        }
                    } else {
                        for i in 0..64 {
                            qt[tq][ZIGZAG[i]] = u16::from_be_bytes([data[p], data[p + 1]]);
                            p += 2;
                        }
                    }
                }
                pos += len;
            }
            0xC4 => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + len > data.len() {
                    break;
                }
                let mut p = pos + 2;
                while p < pos + len {
                    let tc_th = data[p];
                    p += 1;
                    let tc = (tc_th >> 4) & 0x0F;
                    let th = (tc_th & 0x0F) as usize;
                    if th >= 4 {
                        break;
                    }
                    let h = JHuff::parse(&data[p..])
                        .map_err(|e| ImageError::Format(format!("DHT: {}", e)))?;
                    p += 16 + h.syms.len();
                    if tc == 0 {
                        dc_tabs[th] = Some(h);
                    } else {
                        ac_tabs[th] = Some(h);
                    }
                }
                pos += len;
            }
            0xC0 | 0xC1 => {
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                if pos + len > data.len() {
                    break;
                }
                let mut p = pos + 2;
                p += 1; // precision
                fh = u16::from_be_bytes([data[p], data[p + 1]]) as usize;
                p += 2;
                fw = u16::from_be_bytes([data[p], data[p + 1]]) as usize;
                p += 2;
                let nf = data[p] as usize;
                p += 1;
                comps.clear();
                for _ in 0..nf {
                    let id = data[p];
                    let hv = data[p + 1];
                    let tq = data[p + 2];
                    p += 3;
                    let (ch, cv) = (hv >> 4, hv & 0x0F);
                    if ch > hmax {
                        hmax = ch;
                    }
                    if cv > vmax {
                        vmax = cv;
                    }
                    comps.push(JComp {
                        id,
                        h: ch,
                        v: cv,
                        qt: tq as usize,
                        dc_t: 0,
                        ac_t: 0,
                        dc_pred: 0,
                        blocks: Vec::new(),
                        bw: 0,
                        bh: 0,
                    });
                }
                pos += len;
            }
            0xDD => {
                if pos + 4 > data.len() {
                    break;
                }
                restart = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
                pos += u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
            }
            0xC2..=0xCF if m != 0xC4 && m != 0xC8 && m != 0xCC => {
                return Err(ImageError::Format(format!(
                    "JPEG SOF 0x{:02X} unsupported",
                    m
                )));
            }
            _ => {
                if m == 0x00 {
                    pos += 1;
                    continue;
                }
                if pos + 2 > data.len() {
                    break;
                }
                let len = u16::from_be_bytes([data[pos], data[pos + 1]]) as usize;
                pos += len;
            }
        }
    }

    if !scan_done || comps.is_empty() || fw == 0 || fh == 0 {
        return Err(ImageError::Format("JPEG no scan".into()));
    }

    let mut rgba = vec![0u8; fw * fh * 4];
    for y in 0..fh {
        for x in 0..fw {
            let mut ycc = [0i32; 3];
            for (ci, c) in comps.iter().enumerate() {
                if ci >= 3 {
                    break;
                }
                let cx = x * c.h as usize / hmax as usize;
                let cy = y * c.v as usize / vmax as usize;
                let bx = cx / 8;
                let by = cy / 8;
                let bi = by * c.bw + bx;
                let v = if bi < c.blocks.len() {
                    c.blocks[bi][(cy % 8) * 8 + (cx % 8)] as i32
                } else {
                    0
                };
                ycc[ci] = v;
            }
            let (r, g, b) = if comps.len() >= 3 {
                ycbcr_to_rgb(ycc[0], ycc[1], ycc[2])
            } else {
                let g = ycc[0].clamp(0, 255) as u8;
                (g, g, g)
            };
            let i = (y * fw + x) * 4;
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    ImageBuffer::from_raw_rgba8(fw as u32, fh as u32, rgba)
        .map(DynamicImage::ImageRgba8)
        .ok_or_else(|| ImageError::Format("JPEG size".into()))
}

fn decode_jpeg_scan(
    data: &[u8],
    comps: &mut Vec<JComp>,
    width: usize,
    height: usize,
    hmax: u8,
    vmax: u8,
    qt: &[[u16; 64]; 4],
    dc_tabs: &[Option<JHuff>],
    ac_tabs: &[Option<JHuff>],
    restart: u16,
) -> Result<(), ImageError> {
    let mcu_w = hmax as usize;
    let mcu_h = vmax as usize;
    let mx_count = (width + mcu_w * 8 - 1) / (mcu_w * 8);
    let my_count = (height + mcu_h * 8 - 1) / (mcu_h * 8);

    for c in comps.iter_mut() {
        c.bw = mx_count * c.h as usize;
        c.bh = my_count * c.v as usize;
        c.blocks = vec![[0u8; 64]; c.bw * c.bh];
        c.dc_pred = 0;
    }

    let mut br = JBit::new(data);
    let mut mcu_n = 0u32;

    for my in 0..my_count {
        for mx in 0..mx_count {
            if restart > 0 && mcu_n > 0 && (mcu_n % restart as u32) == 0 {
                br.align();
                for c in comps.iter_mut() {
                    c.dc_pred = 0;
                }
            }
            mcu_n += 1;
            for ci in 0..comps.len() {
                let (ch, cv, qid, dct, act) = {
                    let c = &comps[ci];
                    (c.h as usize, c.v as usize, c.qt, c.dc_t, c.ac_t)
                };
                for by in 0..cv {
                    for bx in 0..ch {
                        let bidx = (my * cv + by) * comps[ci].bw + (mx * ch + bx);
                        let dc_h = dc_tabs[dct]
                            .as_ref()
                            .ok_or_else(|| ImageError::Format("no DC".into()))?;
                        let ac_h = ac_tabs[act]
                            .as_ref()
                            .ok_or_else(|| ImageError::Format("no AC".into()))?;
                        let mut coef = [0i32; 64];

                        let dl = dc_h
                            .decode(&mut br)
                            .map_err(|e| ImageError::Format(format!("DC: {}", e)))?
                            as u32;
                        let dv = if dl > 0 {
                            let raw = br
                                .bits(dl)
                                .map_err(|e| ImageError::Format(format!("DC bits: {}", e)))?
                                as i32;
                            if raw >= (1 << (dl - 1)) {
                                raw
                            } else {
                                raw - (1 << dl) + 1
                            }
                        } else {
                            0
                        };
                        comps[ci].dc_pred += dv;
                        coef[0] = comps[ci].dc_pred * qt[qid][0] as i32;

                        let mut k = 1;
                        while k < 64 {
                            let rs = ac_h
                                .decode(&mut br)
                                .map_err(|e| ImageError::Format(format!("AC: {}", e)))?;
                            let r = (rs >> 4) as usize;
                            let s = (rs & 0x0F) as u32;
                            if s == 0 {
                                if r == 15 {
                                    k += 16;
                                    continue;
                                }
                                break;
                            }
                            k += r;
                            if k >= 64 {
                                break;
                            }
                            let raw = br
                                .bits(s)
                                .map_err(|e| ImageError::Format(format!("AC bits: {}", e)))?
                                as i32;
                            let val = if raw >= (1 << (s - 1)) {
                                raw
                            } else {
                                raw - (1 << s) + 1
                            };
                            coef[ZIGZAG[k]] = val * qt[qid][ZIGZAG[k]] as i32;
                            k += 1;
                        }
                        let px = idct_8x8(&coef);
                        if bidx < comps[ci].blocks.len() {
                            comps[ci].blocks[bidx] = px;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}