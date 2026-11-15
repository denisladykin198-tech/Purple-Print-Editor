//! RLE-кодирование байт (`[value, varint(count)]*`).

pub fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let v = data[i];
        let mut j = i + 1;
        while j < data.len() && data[j] == v {
            j += 1;
        }
        out.push(v);
        write_varint(&mut out, (j - i) as u64);
        i = j;
    }
    out
}

pub fn rle_decode(enc: &[u8], expected: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected);
    let mut p = 0;
    while p < enc.len() {
        let v = enc[p];
        p += 1;
        let (run, np) = read_varint(enc, p)?;
        p = np;
        // Безопасная проверка: run > remaining
        let remaining = expected.saturating_sub(out.len());
        if run > remaining as u64 {
            return Err(format!(
                "RLE: данных больше ожидаемого ({} + {} > {})",
                out.len(), run, expected
            ));
        }
        out.extend(std::iter::repeat(v).take(run as usize));
    }
    if out.len() != expected {
        return Err(format!(
            "RLE: ожидалось {} байт, получено {}",
            expected,
            out.len()
        ));
    }
    Ok(out)
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

fn read_varint(d: &[u8], mut p: usize) -> Result<(u64, usize), String> {
    let mut shift = 0u32;
    let mut v = 0u64;
    loop {
        if p >= d.len() {
            return Err("RLE обрезан".into());
        }
        let b = d[p];
        p += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok((v, p));
        }
        shift += 7;
        if shift >= 64 {
            return Err("RLE: слишком длинное число".into());
        }
    }
}

// ============================================================================
// Тесты round-trip
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) {
        let enc = rle_encode(data);
        let dec = rle_decode(&enc, data.len())
            .unwrap_or_else(|e| panic!("decode failed for len={}: {}", data.len(), e));
        assert_eq!(data, &dec[..], "roundtrip mismatch at len={}", data.len());
    }

    #[test]
    fn roundtrip_edge_lengths() {
        roundtrip(&[]);
        roundtrip(&[0]);
        roundtrip(&[1]);
        roundtrip(&[0; 1]);
        roundtrip(&[7; 127]);
        roundtrip(&[7; 128]);
        roundtrip(&[7; 255]);
        roundtrip(&[7; 256]);
        roundtrip(&[7; 16383]);
        roundtrip(&[7; 16384]);
    }

    #[test]
    fn roundtrip_alternating() {
        roundtrip(&[0, 1, 0, 1, 0, 1, 0, 1]);
        roundtrip(&[5, 5, 0, 0, 5, 5, 0, 0]);
    }

    #[test]
    fn roundtrip_voxel_realistic() {
        // 64³ с «оболочкой» — типичный паттерн RLE хорошо сжимает
        let n = 64usize;
        let mut data = vec![0u8; n * n * n];
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    if x == 0 || y == 0 || z == 0 || x == n - 1 || y == n - 1 || z == n - 1 {
                        data[z * n * n + y * n + x] = 1;
                    }
                }
            }
        }
        roundtrip(&data);
    }

    #[test]
    fn decode_too_short_is_error() {
        let enc = rle_encode(&[1, 1, 1]);
        assert!(rle_decode(&enc, 10).is_err());
    }

    #[test]
    fn decode_too_long_is_error() {
        let enc = rle_encode(&[1; 100]);
        assert!(rle_decode(&enc, 10).is_err());
    }

    #[test]
    fn decode_empty_into_nonzero_is_error() {
        let enc = rle_encode(&[]);
        assert!(rle_decode(&enc, 10).is_err());
    }

    #[test]
    fn varint_extremes() {
        let mut buf = Vec::new();
        for v in [0u64, 1, 127, 128, 16383, 16384, u64::MAX] {
            buf.clear();
            write_varint(&mut buf, v);
            let (back, p) = read_varint(&buf, 0).unwrap();
            assert_eq!(back, v);
            assert_eq!(p, buf.len());
        }
    }
}