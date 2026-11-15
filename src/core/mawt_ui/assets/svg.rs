use bevy::ecs::system::EntityCommands;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Корень ассетов проекта. Пути к SVG-файлам строятся относительно него.
const ASSETS_ROOT: &str = "assets";

#[inline]
pub fn asset_path(relative: &str) -> String {
    if relative.starts_with(ASSETS_ROOT) {
        relative.to_string()
    } else {
        format!("{}/{}", ASSETS_ROOT, relative.trim_start_matches('/'))
    }
}

// ---- Minimal SVG engine ----

pub mod svg_engine {
    use std::collections::HashMap;
    use std::f32::consts::PI;

    // ---- XML ----

    #[derive(Debug, Clone, Default)]
    pub struct XmlNode {
        pub name: String,
        pub attrs: HashMap<String, String>,
        pub children: Vec<XmlNode>,
        pub text: String,
    }

    struct XP<'a> {
        b: &'a [u8],
        i: usize,
    }

    impl<'a> XP<'a> {
        fn eof(&self) -> bool {
            self.i >= self.b.len()
        }

        fn cur(&self) -> Option<u8> {
            self.b.get(self.i).copied()
        }

        fn adv(&mut self) {
            self.i += 1;
        }

        fn ws(&mut self) {
            while matches!(self.cur(), Some(c) if c.is_ascii_whitespace()) {
                self.adv();
            }
        }

        fn sw(&self, s: &[u8]) -> bool {
            self.b[self.i..].starts_with(s)
        }

        fn until(&mut self, s: &[u8]) -> Result<(), String> {
            while self.i + s.len() <= self.b.len() {
                if self.b[self.i..].starts_with(s) {
                    self.i += s.len();
                    return Ok(());
                }
                self.i += 1;
            }
            Err(format!("не найдено {:?}", String::from_utf8_lossy(s)))
        }

        fn misc(&mut self) -> Result<(), String> {
            loop {
                self.ws();
                if self.eof() {
                    return Ok(());
                }
                if self.sw(b"<?") {
                    self.until(b"?>")?;
                } else if self.sw(b"<!--") {
                    self.until(b"-->")?;
                } else if self.sw(b"<!DOCTYPE") || self.sw(b"<!doctype") {
                    self.until(b">")?;
                } else {
                    return Ok(());
                }
            }
        }

        fn name(&mut self) -> String {
            let s = self.i;
            while matches!(self.cur(), Some(c) if c.is_ascii_alphanumeric()
                || c == b':' || c == b'-' || c == b'_' || c == b'.')
            {
                self.adv();
            }
            String::from_utf8_lossy(&self.b[s..self.i]).to_string()
        }

        fn elem(&mut self) -> Result<XmlNode, String> {
            self.misc()?;
            if self.cur() != Some(b'<') {
                return Err("ожидался '<'".into());
            }
            self.adv();
            let name = self.name();
            let mut attrs = HashMap::new();
            loop {
                self.ws();
                match self.cur() {
                    Some(b'/') => {
                        self.adv();
                        self.ws();
                        if self.cur() != Some(b'>') {
                            return Err("ожидался '>'".into());
                        }
                        self.adv();
                        return Ok(XmlNode {
                            name,
                            attrs,
                            children: vec![],
                            text: String::new(),
                        });
                    }
                    Some(b'>') => {
                        self.adv();
                        break;
                    }
                    Some(_) => {
                        let k = self.name();
                        if k.is_empty() {
                            return Err("имя атрибута".into());
                        }
                        self.ws();
                        if self.cur() != Some(b'=') {
                            return Err("ожидался '='".into());
                        }
                        self.adv();
                        self.ws();
                        let q = match self.cur() {
                            Some(q @ (b'"' | b'\'')) => {
                                self.adv();
                                q
                            }
                            _ => return Err("кавычка".into()),
                        };
                        let s = self.i;
                        while let Some(c) = self.cur() {
                            if c == q {
                                break;
                            }
                            self.adv();
                        }
                        let v = String::from_utf8_lossy(&self.b[s..self.i]).to_string();
                        if self.cur() != Some(q) {
                            return Err("не закрыта кавычка".into());
                        }
                        self.adv();
                        attrs.insert(k, v);
                    }
                    None => return Err("EOF в теге".into()),
                }
            }
            let mut children = Vec::new();
            let mut text = String::new();
            loop {
                if self.eof() {
                    return Err(format!("не закрыт <{}>", name));
                }
                if self.sw(b"</") {
                    self.i += 2;
                    let cn = self.name();
                    if cn != name {
                        return Err(format!("</{}> vs <{}>", cn, name));
                    }
                    self.ws();
                    if self.cur() != Some(b'>') {
                        return Err(">".into());
                    }
                    self.adv();
                    break;
                } else if self.sw(b"<!--") {
                    self.until(b"-->")?;
                } else if self.sw(b"<![CDATA[") {
                    self.i += 9;
                    let s = self.i;
                    self.until(b"]]>")?;
                    text.push_str(&String::from_utf8_lossy(&self.b[s..self.i - 3]));
                } else if self.cur() == Some(b'<') {
                    children.push(self.elem()?);
                } else {
                    let s = self.i;
                    while let Some(c) = self.cur() {
                        if c == b'<' {
                            break;
                        }
                        self.adv();
                    }
                    text.push_str(&String::from_utf8_lossy(&self.b[s..self.i]));
                }
            }
            Ok(XmlNode {
                name,
                attrs,
                children,
                text,
            })
        }
    }

    fn parse_xml(src: &str) -> Result<XmlNode, String> {
        XP {
            b: src.as_bytes(),
            i: 0,
        }
        .elem()
    }

    // ---- Matrix ----

    #[derive(Clone, Copy, Debug)]
    pub struct Mat(pub [f32; 6]);

    impl Mat {
        pub const ID: Mat = Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

        pub fn mul(self, o: Mat) -> Mat {
            let a = self.0;
            let b = o.0;
            Mat([
                a[0] * b[0] + a[2] * b[1],
                a[1] * b[0] + a[3] * b[1],
                a[0] * b[2] + a[2] * b[3],
                a[1] * b[2] + a[3] * b[3],
                a[0] * b[4] + a[2] * b[5] + a[4],
                a[1] * b[4] + a[3] * b[5] + a[5],
            ])
        }

        pub fn app(self, x: f32, y: f32) -> (f32, f32) {
            (
                self.0[0] * x + self.0[2] * y + self.0[4],
                self.0[1] * x + self.0[3] * y + self.0[5],
            )
        }

        pub fn scale(self) -> f32 {
            ((self.0[0] * self.0[3] - self.0[1] * self.0[2]).abs()).sqrt()
        }

        pub fn tr(tx: f32, ty: f32) -> Mat {
            Mat([1.0, 0.0, 0.0, 1.0, tx, ty])
        }

        pub fn sc(sx: f32, sy: f32) -> Mat {
            Mat([sx, 0.0, 0.0, sy, 0.0, 0.0])
        }

        pub fn rot(a: f32, cx: f32, cy: f32) -> Mat {
            let (s, c) = a.to_radians().sin_cos();
            Mat::tr(cx, cy)
                .mul(Mat([c, s, -s, c, 0.0, 0.0]))
                .mul(Mat::tr(-cx, -cy))
        }

        pub fn skew_x(a: f32) -> Mat {
            Mat([1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0])
        }

        pub fn skew_y(a: f32) -> Mat {
            Mat([1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0])
        }
    }

    pub fn parse_transform(s: &str) -> Mat {
        let mut m = Mat::ID;
        let b = s.as_bytes();
        let n = b.len();
        let mut i = 0;
        while i < n {
            while i < n && (b[i].is_ascii_whitespace() || b[i] == b',') {
                i += 1;
            }
            let s0 = i;
            while i < n && b[i].is_ascii_alphabetic() {
                i += 1;
            }
            if s0 == i {
                break;
            }
            let name = std::str::from_utf8(&b[s0..i]).unwrap_or("");
            while i < n && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= n || b[i] != b'(' {
                break;
            }
            i += 1;
            let s1 = i;
            while i < n && b[i] != b')' {
                i += 1;
            }
            let args: Vec<f32> = std::str::from_utf8(&b[s1..i])
                .unwrap_or("")
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse().ok())
                .collect();
            if i < n {
                i += 1;
            }
            let t = match name {
                "matrix" if args.len() == 6 => {
                    Mat([args[0], args[1], args[2], args[3], args[4], args[5]])
                }
                "translate" if args.len() == 1 => Mat::tr(args[0], 0.0),
                "translate" if args.len() == 2 => Mat::tr(args[0], args[1]),
                "scale" if args.len() == 1 => Mat::sc(args[0], args[0]),
                "scale" if args.len() == 2 => Mat::sc(args[0], args[1]),
                "rotate" if args.len() == 1 => Mat::rot(args[0], 0.0, 0.0),
                "rotate" if args.len() == 3 => Mat::rot(args[0], args[1], args[2]),
                "skewX" if args.len() == 1 => Mat::skew_x(args[0]),
                "skewY" if args.len() == 1 => Mat::skew_y(args[0]),
                _ => Mat::ID,
            };
            m = m.mul(t);
        }
        m
    }

    // ---- Color ----

    #[derive(Clone, Copy, Debug)]
    pub struct Rgba(pub [f32; 4]);

    pub fn parse_color(s: &str) -> Option<Rgba> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") {
            return None;
        }

        if let Some(hex) = s.strip_prefix('#') {
            let h = hex.as_bytes();
            let hx = |v: &[u8]| -> f32 {
                u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap_or(0) as f32 / 255.0
            };
            return match h.len() {
                3 => Some(Rgba([hx(&[h[0], h[0]]), hx(&[h[1], h[1]]), hx(&[h[2], h[2]]), 1.0])),
                4 => Some(Rgba([
                    hx(&[h[0], h[0]]),
                    hx(&[h[1], h[1]]),
                    hx(&[h[2], h[2]]),
                    hx(&[h[3], h[3]]),
                ])),
                6 => Some(Rgba([hx(&h[0..2]), hx(&h[2..4]), hx(&h[4..6]), 1.0])),
                8 => Some(Rgba([
                    hx(&h[0..2]),
                    hx(&h[2..4]),
                    hx(&h[4..6]),
                    hx(&h[6..8]),
                ])),
                _ => None,
            };
        }

        if s.starts_with("rgb") {
            let inside = s.split('(').nth(1)?.split(')').next()?;
            let p: Vec<f32> = inside
                .split(|c: char| c == ',' || c == '/')
                .flat_map(|t| t.split_whitespace())
                .filter(|t| !t.is_empty())
                .map(|t| {
                    if let Some(v) = t.strip_suffix('%') {
                        v.parse::<f32>().map(|x| x / 100.0).unwrap_or(0.0)
                    } else {
                        t.parse::<f32>().map(|x| x / 255.0).unwrap_or(0.0)
                    }
                })
                .collect();
            return Some(Rgba([
                p.get(0).copied().unwrap_or(0.0),
                p.get(1).copied().unwrap_or(0.0),
                p.get(2).copied().unwrap_or(0.0),
                p.get(3).copied().unwrap_or(1.0),
            ]));
        }

        let c = match s.to_ascii_lowercase().as_str() {
            "black" => [0.0, 0.0, 0.0, 1.0],
            "white" => [1.0, 1.0, 1.0, 1.0],
            "red" => [1.0, 0.0, 0.0, 1.0],
            "green" => [0.0, 0.5, 0.0, 1.0],
            "blue" => [0.0, 0.0, 1.0, 1.0],
            "yellow" => [1.0, 1.0, 0.0, 1.0],
            "cyan" | "aqua" => [0.0, 1.0, 1.0, 1.0],
            "magenta" | "fuchsia" => [1.0, 0.0, 1.0, 1.0],
            "gray" | "grey" => [0.5, 0.5, 0.5, 1.0],
            "silver" => [0.75, 0.75, 0.75, 1.0],
            "orange" => [1.0, 0.647, 0.0, 1.0],
            "purple" => [0.5, 0.0, 0.5, 1.0],
            "navy" => [0.0, 0.0, 0.5, 1.0],
            "teal" => [0.0, 0.5, 0.5, 1.0],
            "lime" => [0.0, 1.0, 0.0, 1.0],
            "olive" => [0.5, 0.5, 0.0, 1.0],
            "maroon" => [0.5, 0.0, 0.0, 1.0],
            "pink" => [1.0, 0.753, 0.796, 1.0],
            "brown" => [0.647, 0.165, 0.165, 1.0],
            "gold" => [1.0, 0.843, 0.0, 1.0],
            "indigo" => [0.294, 0.0, 0.510, 1.0],
            "violet" => [0.933, 0.510, 0.933, 1.0],
            _ => return None,
        };
        Some(Rgba(c))
    }

    // ---- Path ----

    pub struct Path {
        pub subpaths: Vec<Vec<(f32, f32)>>,
    }

    fn tok_path(d: &str) -> Vec<String> {
        let mut out = Vec::new();
        let b = d.as_bytes();
        let n = b.len();
        let mut i = 0;
        while i < n {
            let c = b[i];
            if c.is_ascii_alphabetic() {
                out.push((c as char).to_string());
                i += 1;
            } else if c.is_ascii_whitespace() || c == b',' {
                i += 1;
            } else if c == b'-' || c == b'+' || c == b'.' || c.is_ascii_digit() {
                let s = i;
                if c == b'-' || c == b'+' {
                    i += 1;
                }
                while i < n && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i < n && b[i] == b'.' {
                    i += 1;
                    while i < n && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < n && (b[i] == b'e' || b[i] == b'E') {
                    i += 1;
                    if i < n && (b[i] == b'-' || b[i] == b'+') {
                        i += 1;
                    }
                    while i < n && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                out.push(std::str::from_utf8(&b[s..i]).unwrap().to_string());
            } else {
                i += 1;
            }
        }
        out
    }

    /// Следующее число в токенах path. `None` — токены кончились или
    /// попался не-числовой токен (что для пути означает конец команды).
    fn next_num(toks: &[String], i: &mut usize) -> Option<f32> {
        let v = toks.get(*i)?.parse::<f32>().ok()?;
        *i += 1;
        Some(v)
    }

    fn flat_quad(
        out: &mut Vec<(f32, f32)>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    ) {
        let steps = 16;
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let m = 1.0 - t;
            out.push((
                m * m * x0 + 2.0 * m * t * x1 + t * t * x2,
                m * m * y0 + 2.0 * m * t * y1 + t * t * y2,
            ));
        }
    }

    fn flat_cubic(
        out: &mut Vec<(f32, f32)>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
    ) {
        let steps = 24;
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let m = 1.0 - t;
            out.push((
                m * m * m * x0 + 3.0 * m * m * t * x1 + 3.0 * m * t * t * x2 + t * t * t * x3,
                m * m * m * y0 + 3.0 * m * m * t * y1 + 3.0 * m * t * t * y2 + t * t * t * y3,
            ));
        }
    }

    fn flat_arc(
        out: &mut Vec<(f32, f32)>,
        x0: f32,
        y0: f32,
        mut rx: f32,
        mut ry: f32,
        xrot_deg: f32,
        large: bool,
        sweep: bool,
        x1: f32,
        y1: f32,
    ) {
        if rx == 0.0 || ry == 0.0 {
            out.push((x1, y1));
            return;
        }
        let phi = xrot_deg.to_radians();
        let (sp, cp) = phi.sin_cos();
        let dx = (x0 - x1) / 2.0;
        let dy = (y0 - y1) / 2.0;
        let x1p = cp * dx + sp * dy;
        let y1p = -sp * dx + cp * dy;
        rx = rx.abs();
        ry = ry.abs();
        let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
        if lam > 1.0 {
            let s = lam.sqrt();
            rx *= s;
            ry *= s;
        }
        let sign = if large != sweep { 1.0 } else { -1.0 };
        let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
        let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
        let co = if den <= 0.0 { 0.0 } else { sign * (num / den).sqrt() };
        let cxp = co * (rx * y1p / ry);
        let cyp = co * -(ry * x1p / rx);
        let cx = cp * cxp - sp * cyp + (x0 + x1) / 2.0;
        let cy = sp * cxp + cp * cyp + (y0 + y1) / 2.0;
        let ang = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
            let d = ux * vx + uy * vy;
            let l = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
            let mut a = if l <= 0.0 {
                0.0
            } else {
                (d / l).clamp(-1.0, 1.0).acos()
            };
            if ux * vy - uy * vx < 0.0 {
                a = -a;
            }
            a
        };
        let th1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
        let mut dth = ang(
            (x1p - cxp) / rx,
            (y1p - cyp) / ry,
            (-x1p - cxp) / rx,
            (-y1p - cyp) / ry,
        );
        if !sweep && dth > 0.0 {
            dth -= 2.0 * PI;
        }
        if sweep && dth < 0.0 {
            dth += 2.0 * PI;
        }
        // Шаг ~11° на сегмент, минимум 8 шагов на дугу.
        let steps = ((dth.abs() / (PI / 16.0)).ceil() as usize).max(8);
        for k in 1..=steps {
            let t = th1 + dth * (k as f32 / steps as f32);
            let (st, ct) = t.sin_cos();
            out.push((cp * rx * ct - sp * ry * st + cx, sp * rx * ct + cp * ry * st + cy));
        }
    }

    pub fn parse_path(d: &str) -> Path {
        let toks = tok_path(d);
        let mut subpaths: Vec<Vec<(f32, f32)>> = Vec::new();
        let mut cur: Vec<(f32, f32)> = Vec::new();
        let (mut cx, mut cy) = (0.0f32, 0.0f32);
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        let mut last_ctrl: Option<(f32, f32)> = None;
        let mut i = 0;
        let mut cmd: char = ' ';

        while i < toks.len() {
            let t = &toks[i];
            if t.len() == 1 && t.chars().next().unwrap().is_ascii_alphabetic() {
                cmd = t.chars().next().unwrap();
                i += 1;
            }
            let rel = cmd.is_ascii_lowercase();
            match cmd.to_ascii_uppercase() {
                'M' => {
                    let (Some(x), Some(y)) = (next_num(&toks, &mut i), next_num(&toks, &mut i))
                    else {
                        break;
                    };
                    let (nx, ny) = if rel { (cx + x, cy + y) } else { (x, y) };
                    if !cur.is_empty() {
                        subpaths.push(std::mem::take(&mut cur));
                    }
                    cur.push((nx, ny));
                    cx = nx;
                    cy = ny;
                    sx = nx;
                    sy = ny;
                    last_ctrl = None;
                    cmd = if rel { 'l' } else { 'L' };
                }
                'L' => {
                    let (Some(x), Some(y)) = (next_num(&toks, &mut i), next_num(&toks, &mut i))
                    else {
                        break;
                    };
                    let (nx, ny) = if rel { (cx + x, cy + y) } else { (x, y) };
                    cur.push((nx, ny));
                    cx = nx;
                    cy = ny;
                    last_ctrl = None;
                }
                'H' => {
                    let Some(x) = next_num(&toks, &mut i) else {
                        break;
                    };
                    let nx = if rel { cx + x } else { x };
                    cur.push((nx, cy));
                    cx = nx;
                    last_ctrl = None;
                }
                'V' => {
                    let Some(y) = next_num(&toks, &mut i) else {
                        break;
                    };
                    let ny = if rel { cy + y } else { y };
                    cur.push((cx, ny));
                    cy = ny;
                    last_ctrl = None;
                }
                'C' => {
                    let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                    ) else {
                        break;
                    };
                    let (x1, y1) = if rel { (cx + x1, cy + y1) } else { (x1, y1) };
                    let (x2, y2) = if rel { (cx + x2, cy + y2) } else { (x2, y2) };
                    let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                    flat_cubic(&mut cur, cx, cy, x1, y1, x2, y2, x, y);
                    last_ctrl = Some((x2, y2));
                    cx = x;
                    cy = y;
                }
                'S' => {
                    let (Some(x2), Some(y2), Some(x), Some(y)) = (
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                    ) else {
                        break;
                    };
                    let (x2, y2) = if rel { (cx + x2, cy + y2) } else { (x2, y2) };
                    let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                    let (x1, y1) = match last_ctrl {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    flat_cubic(&mut cur, cx, cy, x1, y1, x2, y2, x, y);
                    last_ctrl = Some((x2, y2));
                    cx = x;
                    cy = y;
                }
                'Q' => {
                    let (Some(x1), Some(y1), Some(x), Some(y)) = (
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                    ) else {
                        break;
                    };
                    let (x1, y1) = if rel { (cx + x1, cy + y1) } else { (x1, y1) };
                    let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                    flat_quad(&mut cur, cx, cy, x1, y1, x, y);
                    last_ctrl = Some((x1, y1));
                    cx = x;
                    cy = y;
                }
                'T' => {
                    let (Some(x), Some(y)) = (next_num(&toks, &mut i), next_num(&toks, &mut i))
                    else {
                        break;
                    };
                    let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                    let (x1, y1) = match last_ctrl {
                        Some((lx, ly)) => (2.0 * cx - lx, 2.0 * cy - ly),
                        None => (cx, cy),
                    };
                    flat_quad(&mut cur, cx, cy, x1, y1, x, y);
                    last_ctrl = Some((x1, y1));
                    cx = x;
                    cy = y;
                }
                'A' => {
                    let (Some(rx), Some(ry), Some(xr), Some(laf), Some(sf), Some(x), Some(y)) = (
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                        next_num(&toks, &mut i),
                    ) else {
                        break;
                    };
                    let (x, y) = if rel { (cx + x, cy + y) } else { (x, y) };
                    flat_arc(&mut cur, cx, cy, rx, ry, xr, laf != 0.0, sf != 0.0, x, y);
                    cx = x;
                    cy = y;
                    last_ctrl = None;
                }
                'Z' => {
                    if !cur.is_empty() {
                        cur.push((sx, sy));
                        subpaths.push(std::mem::take(&mut cur));
                    }
                    cx = sx;
                    cy = sy;
                    last_ctrl = None;
                }
                _ => break,
            }
        }
        if !cur.is_empty() {
            subpaths.push(cur);
        }
        Path { subpaths }
    }

    // ---- Raster ----

    pub struct Canvas {
        pub w: usize,
        pub h: usize,
        pub data: Vec<[f32; 4]>,
    }

    impl Canvas {
        fn new(w: usize, h: usize) -> Self {
            Self {
                w,
                h,
                data: vec![[0.0; 4]; w * h],
            }
        }

        fn blend(&mut self, x: usize, y: usize, c: [f32; 4]) {
            debug_assert!(x < self.w && y < self.h);
            let d = &mut self.data[y * self.w + x];
            let (sa, da) = (c[3], d[3]);
            let oa = sa + da * (1.0 - sa);
            if oa <= 0.0 {
                return;
            }
            d[0] = (c[0] * sa + d[0] * da * (1.0 - sa)) / oa;
            d[1] = (c[1] * sa + d[1] * da * (1.0 - sa)) / oa;
            d[2] = (c[2] * sa + d[2] * da * (1.0 - sa)) / oa;
            d[3] = oa;
        }
    }

    fn fill_polygon(c: &mut Canvas, poly: &[(f32, f32)], color: [f32; 4], even_odd: bool) {
        if poly.len() < 3 {
            return;
        }
        let n = poly.len();
        let mut ymin = f32::INFINITY;
        let mut ymax = f32::NEG_INFINITY;
        for &(_, y) in poly {
            if y < ymin {
                ymin = y;
            }
            if y > ymax {
                ymax = y;
            }
        }
        let y0 = ymin.floor().max(0.0) as i32;
        let y1 = (ymax.ceil() as i32).min(c.h as i32);
        let mut cr: Vec<(f32, i32)> = Vec::new();
        for y in y0..y1 {
            let sy = y as f32 + 0.5;
            cr.clear();
            for k in 0..n {
                let (xa, ya) = poly[k];
                let (xb, yb) = poly[(k + 1) % n];
                if (ya <= sy && yb > sy) || (yb <= sy && ya > sy) {
                    let t = (sy - ya) / (yb - ya);
                    cr.push((xa + t * (xb - xa), if yb > ya { 1 } else { -1 }));
                }
            }
            cr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            let mut wind = 0;
            let mut start = 0.0;
            for (x, dir) in cr.iter() {
                if even_odd {
                    if wind == 0 {
                        start = *x;
                        wind = 1;
                    } else {
                        fill_span(c, y as usize, start, *x, color);
                        wind = 0;
                    }
                } else {
                    if wind == 0 {
                        start = *x;
                    }
                    wind += dir;
                    if wind == 0 {
                        fill_span(c, y as usize, start, *x, color);
                    }
                }
            }
        }
    }

    fn fill_span(c: &mut Canvas, y: usize, xa: f32, xb: f32, color: [f32; 4]) {
        if xb <= xa {
            return;
        }
        let xs = xa.max(0.0);
        let xe = xb.min(c.w as f32);
        if xe <= xs {
            return;
        }
        for px in (xs.floor() as i32)..(xe.ceil() as i32) {
            if px < 0 || px >= c.w as i32 {
                continue;
            }
            let p0 = px as f32;
            let ov = (xe.min(p0 + 1.0) - xs.max(p0)).max(0.0);
            if ov > 0.0 {
                let a = color[3] * ov;
                c.blend(px as usize, y, [color[0], color[1], color[2], a]);
            }
        }
    }

    fn circle_poly(cx: f32, cy: f32, r: f32, seg: usize) -> Vec<(f32, f32)> {
        let mut v = Vec::with_capacity(seg);
        for k in 0..seg {
            let a = 2.0 * PI * (k as f32) / (seg as f32);
            v.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        v
    }

    fn stroke_path(c: &mut Canvas, sp: &[(f32, f32)], width: f32, color: [f32; 4]) {
        if sp.len() < 2 {
            return;
        }
        let hw = width / 2.0;
        for i in 0..sp.len() - 1 {
            let (x0, y0) = sp[i];
            let (x1, y1) = sp[i + 1];
            let dx = x1 - x0;
            let dy = y1 - y0;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                continue;
            }
            let nx = -dy / len * hw;
            let ny = dx / len * hw;
            let quad = [
                (x0 + nx, y0 + ny),
                (x1 + nx, y1 + ny),
                (x1 - nx, y1 - ny),
                (x0 - nx, y0 - ny),
            ];
            fill_polygon(c, &quad, color, false);
        }
        for &(x, y) in sp.iter() {
            fill_polygon(c, &circle_poly(x, y, hw, 12), color, false);
        }
    }

    // ---- SVG AST ----

    #[derive(Clone)]
    pub enum Paint {
        Solid(Rgba),
        Linear {
            x1: f32,
            y1: f32,
            x2: f32,
            y2: f32,
            stops: Vec<(f32, Rgba)>,
        },
        Radial {
            cx: f32,
            cy: f32,
            r: f32,
            stops: Vec<(f32, Rgba)>,
        },
    }

    #[derive(Clone)]
    pub struct Style {
        pub fill: Option<Paint>,
        pub stroke: Option<Paint>,
        pub stroke_width: f32,
        pub fill_opacity: f32,
        pub stroke_opacity: f32,
        pub opacity: f32,
    }

    impl Default for Style {
        fn default() -> Self {
            Self {
                fill: Some(Paint::Solid(Rgba([0.0, 0.0, 0.0, 1.0]))),
                stroke: None,
                stroke_width: 1.0,
                fill_opacity: 1.0,
                stroke_opacity: 1.0,
                opacity: 1.0,
            }
        }
    }

    #[derive(Clone)]
    pub struct Item {
        pub path: Vec<Vec<(f32, f32)>>,
        pub style: Style,
    }

    pub struct Tree {
        pub view_box: Option<(f32, f32, f32, f32)>,
        pub items: Vec<Item>,
    }

    #[derive(Clone)]
    struct GradDef {
        linear: bool,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        cx: f32,
        cy: f32,
        r: f32,
        stops: Vec<(f32, Rgba)>,
    }

    fn parse_len(s: &str) -> Option<f32> {
        let s = s.trim();
        let s = s
            .trim_end_matches("px")
            .trim_end_matches("pt")
            .trim_end_matches("mm")
            .trim_end_matches("cm")
            .trim_end_matches("in")
            .trim_end_matches("em")
            .trim_end_matches("ex");
        s.parse::<f32>().ok()
    }

    fn nattr(a: &HashMap<String, String>, k: &str, d: f32) -> f32 {
        a.get(k).and_then(|v| parse_len(v)).unwrap_or(d)
    }

    fn parse_points(s: &str) -> Vec<(f32, f32)> {
        let nums: Vec<f32> = s
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse().ok())
            .collect();
        let mut v = Vec::new();
        let mut i = 0;
        while i + 1 < nums.len() {
            v.push((nums[i], nums[i + 1]));
            i += 2;
        }
        v
    }

    fn collect_gradients(node: &XmlNode, out: &mut HashMap<String, GradDef>) {
        if node.name == "linearGradient" || node.name == "radialGradient" {
            if let Some(id) = node.attrs.get("id") {
                let linear = node.name == "linearGradient";
                let mut stops: Vec<(f32, Rgba)> = Vec::new();
                for c in &node.children {
                    if c.name == "stop" {
                        let off = c
                            .attrs
                            .get("offset")
                            .map(|v| {
                                if let Some(p) = v.trim().strip_suffix('%') {
                                    p.parse::<f32>().map(|x| x / 100.0).unwrap_or(0.0)
                                } else {
                                    parse_len(v).unwrap_or(0.0)
                                }
                            })
                            .unwrap_or(0.0);
                        let mut col = c
                            .attrs
                            .get("stop-color")
                            .and_then(|v| parse_color(v))
                            .unwrap_or(Rgba([0.0, 0.0, 0.0, 1.0]));
                        if let Some(op) = c
                            .attrs
                            .get("stop-opacity")
                            .and_then(|v| v.parse::<f32>().ok())
                        {
                            col.0[3] = op;
                        }
                        stops.push((off, col));
                    }
                }
                stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                out.insert(
                    id.clone(),
                    GradDef {
                        linear,
                        x1: nattr(&node.attrs, "x1", 0.0),
                        y1: nattr(&node.attrs, "y1", 0.0),
                        x2: nattr(&node.attrs, "x2", 1.0),
                        y2: nattr(&node.attrs, "y2", 0.0),
                        cx: nattr(&node.attrs, "cx", 0.5),
                        cy: nattr(&node.attrs, "cy", 0.5),
                        r: nattr(&node.attrs, "r", 0.5),
                        stops,
                    },
                );
            }
        }
        for c in &node.children {
            collect_gradients(c, out);
        }
    }

    fn parse_paint(s: &str, g: &HashMap<String, GradDef>) -> Option<Paint> {
        let s = s.trim();
        if let Some(rest) = s.strip_prefix("url(") {
            let id = rest.trim_end_matches(')').trim().trim_start_matches('#');
            if let Some(g) = g.get(id) {
                return Some(if g.linear {
                    Paint::Linear {
                        x1: g.x1,
                        y1: g.y1,
                        x2: g.x2,
                        y2: g.y2,
                        stops: g.stops.clone(),
                    }
                } else {
                    Paint::Radial {
                        cx: g.cx,
                        cy: g.cy,
                        r: g.r,
                        stops: g.stops.clone(),
                    }
                });
            }
            return None;
        }
        parse_color(s).map(Paint::Solid)
    }

    fn get_style(
        a: &HashMap<String, String>,
        parent: &Style,
        g: &HashMap<String, GradDef>,
    ) -> Style {
        let mut s = parent.clone();
        let mut m: HashMap<String, String> = a.clone();
        if let Some(st) = a.get("style") {
            for decl in st.split(';') {
                if let Some((k, v)) = decl.split_once(':') {
                    m.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
        if let Some(v) = m.get("fill") {
            s.fill = parse_paint(v, g);
        }
        if let Some(v) = m.get("fill-opacity") {
            s.fill_opacity = v.parse().unwrap_or(1.0);
        }
        if let Some(v) = m.get("stroke") {
            s.stroke = parse_paint(v, g);
        }
        if let Some(v) = m.get("stroke-opacity") {
            s.stroke_opacity = v.parse().unwrap_or(1.0);
        }
        if let Some(v) = m.get("stroke-width") {
            s.stroke_width = parse_len(v).unwrap_or(1.0);
        }
        if let Some(v) = m.get("opacity") {
            s.opacity = v.parse().unwrap_or(1.0);
        }
        s
    }

    fn shape_polys(node: &XmlNode) -> Vec<Vec<(f32, f32)>> {
        let a = &node.attrs;
        match node.name.as_str() {
            "rect" => {
                let x = nattr(a, "x", 0.0);
                let y = nattr(a, "y", 0.0);
                let w = nattr(a, "width", 0.0);
                let h = nattr(a, "height", 0.0);
                if w <= 0.0 || h <= 0.0 {
                    return vec![];
                }
                vec![vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]]
            }
            "circle" => {
                let cx = nattr(a, "cx", 0.0);
                let cy = nattr(a, "cy", 0.0);
                let r = nattr(a, "r", 0.0);
                if r <= 0.0 {
                    return vec![];
                }
                vec![circle_poly(cx, cy, r, 48)]
            }
            "ellipse" => {
                let cx = nattr(a, "cx", 0.0);
                let cy = nattr(a, "cy", 0.0);
                let rx = nattr(a, "rx", 0.0);
                let ry = nattr(a, "ry", 0.0);
                if rx <= 0.0 || ry <= 0.0 {
                    return vec![];
                }
                let seg = 48;
                let mut v = Vec::with_capacity(seg);
                for k in 0..seg {
                    let t = 2.0 * PI * (k as f32) / (seg as f32);
                    v.push((cx + rx * t.cos(), cy + ry * t.sin()));
                }
                vec![v]
            }
            "line" => {
                vec![vec![
                    (nattr(a, "x1", 0.0), nattr(a, "y1", 0.0)),
                    (nattr(a, "x2", 0.0), nattr(a, "y2", 0.0)),
                ]]
            }
            "polyline" | "polygon" => {
                let p = parse_points(a.get("points").map(|s| s.as_str()).unwrap_or(""));
                if p.len() < 2 {
                    return vec![];
                }
                let mut p = p;
                if node.name == "polygon" {
                    p.push(p[0]);
                }
                vec![p]
            }
            "path" => {
                let d = a.get("d").cloned().unwrap_or_default();
                parse_path(&d).subpaths
            }
            _ => vec![],
        }
    }

    fn walk(
        node: &XmlNode,
        parent_m: Mat,
        parent_s: &Style,
        g: &HashMap<String, GradDef>,
        items: &mut Vec<Item>,
    ) {
        if node.attrs.get("display").map(|s| s == "none").unwrap_or(false) {
            return;
        }
        let s = get_style(&node.attrs, parent_s, g);
        let m = parent_m.mul(
            node.attrs
                .get("transform")
                .map(|t| parse_transform(t))
                .unwrap_or(Mat::ID),
        );

        let polys = shape_polys(node);
        if !polys.is_empty() {
            let transformed: Vec<Vec<(f32, f32)>> = polys
                .into_iter()
                .map(|sp| sp.into_iter().map(|(x, y)| m.app(x, y)).collect())
                .collect();
            let mut s2 = s.clone();
            s2.stroke_width *= m.scale();
            items.push(Item {
                path: transformed,
                style: s2,
            });
        }
        for c in &node.children {
            walk(c, m, &s, g, items);
        }
    }

    pub fn parse(src: &str) -> Result<Tree, String> {
        let root = parse_xml(src)?;
        let mut grads = HashMap::new();
        collect_gradients(&root, &mut grads);

        let view_box = root.attrs.get("viewBox").and_then(|s| {
            let p: Vec<f32> = s
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter_map(|t| t.parse().ok())
                .collect();
            if p.len() == 4 {
                Some((p[0], p[1], p[2], p[3]))
            } else {
                None
            }
        });

        let mut items = Vec::new();
        let base = Style::default();
        walk(&root, Mat::ID, &base, &grads, &mut items);
        Ok(Tree { view_box, items })
    }

    fn resolve_color(paint: &Paint, alpha: f32) -> [f32; 4] {
        match paint {
            Paint::Solid(c) => [c.0[0], c.0[1], c.0[2], c.0[3] * alpha],
            // Градиенты сведены к среднему цвету стопов: полноценная
            // реализация потребует интерполяции по t внутри fill_span.
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
                if stops.is_empty() {
                    return [0.0; 4];
                }
                let (mut r, mut g, mut b, mut a) = (0.0, 0.0, 0.0, 0.0);
                for (_, c) in stops {
                    r += c.0[0];
                    g += c.0[1];
                    b += c.0[2];
                    a += c.0[3];
                }
                let n = stops.len() as f32;
                [r / n, g / n, b / n, a / n * alpha]
            }
        }
    }

    fn render_item(c: &mut Canvas, sps: &[Vec<(f32, f32)>], style: &Style) {
        if let Some(p) = &style.fill {
            let col = resolve_color(p, style.fill_opacity * style.opacity);
            for sp in sps {
                if sp.len() >= 3 {
                    fill_polygon(c, sp, col, false);
                }
            }
        }
        if let Some(p) = &style.stroke {
            if style.stroke_width > 0.0 {
                let col = resolve_color(p, style.stroke_opacity * style.opacity);
                for sp in sps {
                    stroke_path(c, sp, style.stroke_width, col);
                }
            }
        }
    }

    pub struct Pixmap {
        pub width: u32,
        pub height: u32,
        pub data: Vec<u8>,
    }

    pub fn render(tree: &Tree, out_w: u32, out_h: u32) -> Result<Pixmap, String> {
        const SS: usize = 4;
        let w = out_w as usize * SS;
        let h = out_h as usize * SS;
        let mut canvas = Canvas::new(w, h);

        let fit = if let Some((vx, vy, vw, vh)) = tree.view_box {
            let s = (w as f32 / vw).min(h as f32 / vh);
            let tx = (w as f32 - vw * s) / 2.0 - vx * s;
            let ty = (h as f32 - vh * s) / 2.0 - vy * s;
            Mat([s, 0.0, 0.0, s, tx, ty])
        } else {
            Mat::ID
        };

        for item in &tree.items {
            let sps: Vec<Vec<(f32, f32)>> = item
                .path
                .iter()
                .map(|sp| sp.iter().map(|&(x, y)| fit.app(x, y)).collect())
                .collect();
            let mut style = item.style.clone();
            // Штрих задан в SVG-единицах; масштабируем под фактический viewBox,
            // иначе тонкие штрихи иконок при малом размере исчезают.
            style.stroke_width *= fit.scale();
            render_item(&mut canvas, &sps, &style);
        }

        let mut data = vec![0u8; (out_w * out_h * 4) as usize];
        for oy in 0..out_h as usize {
            for ox in 0..out_w as usize {
                let mut r = 0.0;
                let mut g = 0.0;
                let mut b = 0.0;
                let mut a = 0.0;
                for dy in 0..SS {
                    for dx in 0..SS {
                        let p = canvas.data[(oy * SS + dy) * w + (ox * SS + dx)];
                        r += p[0] * p[3];
                        g += p[1] * p[3];
                        b += p[2] * p[3];
                        a += p[3];
                    }
                }
                let n = (SS * SS) as f32;
                let av = a / n;
                let (rr, gg, bb) = if av > 0.0 {
                    (r / (n * av), g / (n * av), b / (n * av))
                } else {
                    (0.0, 0.0, 0.0)
                };
                let idx = (oy * out_w as usize + ox) * 4;
                data[idx] = (rr.clamp(0.0, 1.0) * 255.0) as u8;
                data[idx + 1] = (gg.clamp(0.0, 1.0) * 255.0) as u8;
                data[idx + 2] = (bb.clamp(0.0, 1.0) * 255.0) as u8;
                data[idx + 3] = (av.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
        Ok(Pixmap {
            width: out_w,
            height: out_h,
            data,
        })
    }
}

// ---- Components ----

#[derive(Component)]
pub struct SvgAsset {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}

impl SvgAsset {
    pub fn new(path: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            path: path.into(),
            width,
            height,
            scale: 1.0,
        }
    }

    pub fn with_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }
}

#[derive(Component)]
pub struct SvgTextureReady {
    pub texture: Handle<Image>,
}

#[derive(Resource, Default)]
pub struct SvgTextureCache {
    pub cache: std::collections::HashMap<String, Handle<Image>>,
}

#[derive(Resource)]
pub struct MawtSvgSettings {
    pub default_width: u32,
    pub default_height: u32,
    pub enable_caching: bool,
}

impl Default for MawtSvgSettings {
    fn default() -> Self {
        Self {
            default_width: 128,
            default_height: 128,
            enable_caching: true,
        }
    }
}

// ---- Plugin ----

pub struct MawtSvgPlugin;

impl Plugin for MawtSvgPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SvgTextureCache>()
            .init_resource::<MawtSvgSettings>()
            .add_systems(Update, load_svg_assets_system);
    }
}

// ---- System ----

fn load_svg_assets_system(
    mut commands: Commands,
    mut query: Query<(
        Entity,
        &SvgAsset,
        Option<&SvgTextureReady>,
        Option<&mut Sprite>,
        Option<&mut UiImage>,
    )>,
    mut images: ResMut<Assets<Image>>,
    mut cache: ResMut<SvgTextureCache>,
    settings: Res<MawtSvgSettings>,
) {
    for (entity, svg_asset, ready, mut sprite, mut ui) in query.iter_mut() {
        if ready.is_some() {
            continue;
        }

        let key = format!(
            "{}_{}_{}_{}",
            svg_asset.path, svg_asset.width, svg_asset.height, svg_asset.scale
        );

        if settings.enable_caching {
            if let Some(h) = cache.cache.get(&key) {
                let handle = h.clone();
                commands.entity(entity).insert(SvgTextureReady {
                    texture: handle.clone(),
                });
                apply_texture(
                    &mut commands,
                    entity,
                    handle,
                    sprite.as_deref_mut(),
                    ui.as_deref_mut(),
                );
                continue;
            }
        }

        let data = match std::fs::read_to_string(&svg_asset.path) {
            Ok(d) => d,
            Err(e) => {
                error!("[MawtSVG] read {}: {}", svg_asset.path, e);
                continue;
            }
        };

        let handle = match render_svg_to_texture(
            &data,
            svg_asset.width,
            svg_asset.height,
            svg_asset.scale,
            &mut images,
        ) {
            Ok(h) => h,
            Err(e) => {
                error!("[MawtSVG] render {}: {}", svg_asset.path, e);
                continue;
            }
        };

        if settings.enable_caching {
            cache.cache.insert(key, handle.clone());
        }

        commands.entity(entity).insert(SvgTextureReady {
            texture: handle.clone(),
        });
        apply_texture(
            &mut commands,
            entity,
            handle,
            sprite.as_deref_mut(),
            ui.as_deref_mut(),
        );
        info!("[MawtSVG] rendered: {}", svg_asset.path);
    }
}

fn apply_texture(
    commands: &mut Commands,
    entity: Entity,
    texture: Handle<Image>,
    sprite: Option<&mut Sprite>,
    ui: Option<&mut UiImage>,
) {
    if sprite.is_some() {
        commands.entity(entity).insert(texture);
        return;
    }
    if let Some(u) = ui {
        u.texture = texture;
        return;
    }
    commands.entity(entity).insert(UiImage::new(texture));
}

// ---- Render ----

fn render_svg_to_texture(
    svg_data: &str,
    width: u32,
    height: u32,
    scale: f32,
    images: &mut Assets<Image>,
) -> Result<Handle<Image>, Box<dyn std::error::Error + Send + Sync>> {
    let sw = ((width as f32) * scale).max(1.0) as u32;
    let sh = ((height as f32) * scale).max(1.0) as u32;

    let tree = svg_engine::parse(svg_data)?;
    let pixmap = svg_engine::render(&tree, sw, sh)?;

    let image = Image::new(
        Extent3d {
            width: pixmap.width,
            height: pixmap.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixmap.data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    );
    Ok(images.add(image))
}

// ---- UI helpers ----

pub fn spawn_svg_icon(parent: &mut ChildBuilder, path: &str, size: f32) -> Entity {
    parent
        .spawn((
            ImageBundle {
                style: Style {
                    width: Val::Px(size),
                    height: Val::Px(size),
                    ..default()
                },
                ..default()
            },
            SvgAsset::new(asset_path(path), size as u32, size as u32),
        ))
        .id()
}

pub fn spawn_svg_background(
    parent: &mut ChildBuilder,
    path: &str,
    width: u32,
    height: u32,
) -> Entity {
    info!("[MawtSVG] loading background: {}", path);
    parent
        .spawn((
            ImageBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(0.0),
                    top: Val::Px(0.0),
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ..default()
            },
            SvgAsset::new(asset_path(path), width, height),
        ))
        .id()
}

pub fn spawn_svg_sprite(commands: &mut Commands, path: &str, width: u32, height: u32) -> Entity {
    commands
        .spawn((
            SvgAsset::new(asset_path(path), width, height),
            Sprite {
                custom_size: Some(Vec2::new(width as f32, height as f32)),
                ..default()
            },
        ))
        .id()
}

pub fn spawn_svg_ui_image<'a>(
    commands: &'a mut Commands,
    path: &str,
    width: f32,
    height: f32,
) -> EntityCommands<'a> {
    commands.spawn((
        NodeBundle {
            style: Style {
                width: Val::Px(width),
                height: Val::Px(height),
                ..default()
            },
            background_color: BackgroundColor(Color::NONE),
            ..default()
        },
        SvgAsset::new(asset_path(path), width as u32, height as u32),
    ))
}