//! Простой токенайзер Rhai для подсветки синтаксиса (без полного парсинга).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Keyword,
    Str,
    Number,
    Comment,
    Call,
    Const,
}

/// `matches!` компилируется в switch, без линейного поиска по массиву.
fn is_keyword(w: &str) -> bool {
    matches!(
        w,
        "fn" | "let"
            | "const"
            | "if"
            | "else"
            | "for"
            | "in"
            | "while"
            | "loop"
            | "do"
            | "until"
            | "return"
            | "break"
            | "continue"
            | "switch"
            | "throw"
            | "try"
            | "catch"
            | "import"
            | "export"
            | "as"
            | "private"
            | "this"
    )
}

/// Непересекающиеся диапазоны (начало, конец, тип) в байтах;
/// всё, что не попало в диапазон, — обычный текст.
pub fn tokenize_rhai(src: &str) -> Vec<(usize, usize, Tok)> {
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0;
    let mut out = Vec::new();

    while i < n {
        let c = b[i];

        // Комментарии: // ... и /* ... */.
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            let s = i;
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            out.push((s, i, Tok::Comment));
        } else if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let s = i;
            i += 2;
            while i + 1 < n && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(n);
            out.push((s, i, Tok::Comment));
        }
        // Строки и символьные литералы: "...", '...', `...` (raw).
        else if c == b'"' || c == b'`' || c == b'\'' {
            let q = c;
            let s = i;
            i += 1;
            while i < n {
                if b[i] == b'\\' && q != b'`' {
                    i += 2;
                    continue;
                }
                if b[i] == q {
                    i += 1;
                    break;
                }
                if b[i] == b'\n' && q != b'`' {
                    break;
                }
                i += 1;
            }
            let e = i.min(n);
            out.push((s, e, Tok::Str));
            i = e;
        }
        // Числа: 42, 1.5, 1.5e10, 0xFF, 0b1010, 1_000.
        else if c.is_ascii_digit() {
            let s = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            // Десятичная точка — только если за ней цифра (не `..`).
            while i + 1 < n && b[i] == b'.' && b[i + 1].is_ascii_digit() {
                i += 2;
                while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
            }
            out.push((s, i, Tok::Number));
        }
        // Идентификаторы и ключевые слова.
        else if c.is_ascii_alphabetic() || c == b'_' {
            let s = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let w = &src[s..i];
            if is_keyword(w) {
                out.push((s, i, Tok::Keyword));
            } else if w == "true" || w == "false" {
                out.push((s, i, Tok::Const));
            } else {
                // Проверка на вызов: пробелы/табы (не переводы строк) перед `(`.
                let mut j = i;
                while j < n && (b[j] == b' ' || b[j] == b'\t') {
                    j += 1;
                }
                if j < n && b[j] == b'(' {
                    out.push((s, i, Tok::Call));
                }
            }
        } else {
            i += 1;
        }
    }

    out
}