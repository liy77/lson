//! Comment stripping that preserves line numbers.
//!
//! Line comments (`#`) and block comments (`/* */`) are replaced with spaces
//! (and newlines kept) so a later line-oriented parser still reports the
//! original line. Strings are not scanned here — the parser handles them —
//! except that a `#` inside a `"` string on the same line is left alone.

/// A `#` starts a line comment unless the next character begins a tag name.
/// `# comment` and a trailing `#` are comments. `#Ok` / `#Off` are tagged unions.
fn is_line_comment(chars: &[char], i: usize) -> bool {
    match chars.get(i + 1) {
        None => true,
        Some(c) if c.is_ascii_alphabetic() || *c == '_' => false,
        Some(_) => true,
    }
}

/// Replace comments with whitespace, keeping newlines.
pub fn strip_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut in_string = false;
    let mut triple = false;
    while i < chars.len() {
        let c = chars[i];
        if !in_string && c == '/' && chars.get(i + 1) == Some(&'*') {
            out.push(' ');
            out.push(' ');
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    break;
                }
                if chars[i] == '\n' {
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                i += 1;
            }
            continue;
        }
        if c == '"' {
            let is_triple = chars.get(i + 1) == Some(&'"') && chars.get(i + 2) == Some(&'"');
            if !in_string && is_triple {
                in_string = true;
                triple = true;
                out.push('"');
                out.push('"');
                out.push('"');
                i += 3;
                continue;
            }
            if in_string && triple && is_triple {
                in_string = false;
                triple = false;
                out.push('"');
                out.push('"');
                out.push('"');
                i += 3;
                continue;
            }
            if !triple {
                in_string = !in_string;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if !in_string && c == '#' && is_line_comment(&chars, i) {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_inside_string_survives() {
        let src = "k = \"a # b\"\n# gone\n";
        let stripped = strip_comments(src);
        assert!(stripped.contains("\"a # b\""));
        assert!(!stripped.contains("gone"));
    }

    #[test]
    fn block_comment_keeps_newlines() {
        let src = "a = 1\n/* one\ntwo */\nb = 2\n";
        let stripped = strip_comments(src);
        assert_eq!(stripped.lines().count(), 4);
        assert!(stripped.contains("b = 2"));
    }
}
