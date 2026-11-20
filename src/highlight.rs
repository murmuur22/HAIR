//! Passive, bounded lexical colouring for reader code blocks. One linear scan
//! over already-extracted text: nothing is parsed as a program, evaluated,
//! fetched or executed. Unknown languages stay plain.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}

/// Conservative family for a `class`/`data-lang` attribute value, e.g.
/// `language-rust`, `lang-py`, `highlight-source-js`, `brush: bash`.
pub fn language(attribute: &str) -> Option<&'static str> {
    attribute
        .split(|c: char| c.is_whitespace() || c == ';' || c == ':')
        .filter(|token| !token.is_empty())
        .find_map(|token| {
            let token = token.to_ascii_lowercase();
            let name = [
                "language-",
                "lang-",
                "highlight-source-",
                "highlight-",
                "source-",
            ]
            .iter()
            .find_map(|p| token.strip_prefix(p))
            .unwrap_or(&token);
            family(name)
        })
}
fn family(name: &str) -> Option<&'static str> {
    Some(match name {
        "python" | "py" | "python3" => "python",
        "rust" | "rs" => "rust",
        "javascript" | "js" | "jsx" | "mjs" | "typescript" | "ts" | "tsx" => "javascript",
        "c" | "h" | "cpp" | "c++" | "cxx" | "cc" | "hpp" => "c",
        "bash" | "sh" | "shell" | "zsh" | "console" | "shell-session" => "shell",
        "json" | "jsonc" => "json",
        "html" | "xml" | "xhtml" | "svg" => "html",
        _ => return None,
    })
}
fn keyword(language: &str, word: &str) -> bool {
    let list: &[&str] = match language {
        "python" => &[
            "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
            "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
            "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield",
        ],
        "rust" => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        "javascript" => &[
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "else",
            "export",
            "extends",
            "false",
            "finally",
            "for",
            "function",
            "if",
            "import",
            "in",
            "instanceof",
            "interface",
            "let",
            "new",
            "null",
            "return",
            "static",
            "switch",
            "this",
            "throw",
            "true",
            "try",
            "type",
            "typeof",
            "undefined",
            "var",
            "void",
            "while",
            "yield",
        ],
        "c" => &[
            "auto",
            "bool",
            "break",
            "case",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "double",
            "else",
            "enum",
            "extern",
            "false",
            "float",
            "for",
            "if",
            "include",
            "define",
            "inline",
            "int",
            "long",
            "namespace",
            "new",
            "nullptr",
            "private",
            "public",
            "return",
            "short",
            "signed",
            "sizeof",
            "static",
            "struct",
            "switch",
            "template",
            "this",
            "true",
            "typedef",
            "union",
            "unsigned",
            "using",
            "void",
            "while",
        ],
        "shell" => &[
            "case", "cd", "do", "done", "echo", "elif", "else", "esac", "exit", "export", "fi",
            "for", "function", "if", "in", "local", "return", "then", "until", "while",
        ],
        "json" => &["true", "false", "null"],
        _ => &[],
    };
    list.contains(&word)
}

/// One class per `char` of `code`. Linear in input length; never allocates more
/// than the output vector.
pub fn classify(language: Option<&str>, code: &str) -> Vec<Class> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = vec![Class::Plain; chars.len()];
    let Some(language) = language.and_then(family) else {
        return out;
    };
    let at = |i: usize, s: &str| {
        s.chars()
            .enumerate()
            .all(|(k, c)| chars.get(i + k) == Some(&c))
    };
    let line_comment: &[&str] = match language {
        "python" | "shell" => &["#"],
        "rust" | "javascript" | "c" | "json" => &["//"],
        _ => &[],
    };
    let block_comment = match language {
        "rust" | "javascript" | "c" | "json" => Some(("/*", "*/")),
        "html" => Some(("<!--", "-->")),
        _ => None,
    };
    let quotes: &[char] = match language {
        "javascript" => &['"', '\'', '`'],
        "python" | "c" | "shell" => &['"', '\''],
        _ => &['"'],
    };
    let mut in_tag = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some((open, close)) = block_comment
            && at(i, open)
        {
            let mut end = i + open.chars().count();
            while end < chars.len() && !at(end, close) {
                end += 1;
            }
            end = (end + close.chars().count()).min(chars.len());
            out[i..end].fill(Class::Comment);
            i = end;
            continue;
        }
        if language == "html" {
            if c == '<' {
                in_tag = true;
                let mut end = i + 1;
                if chars.get(end) == Some(&'/') {
                    end += 1;
                }
                let name = end;
                while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '-')
                {
                    end += 1;
                }
                out[name..end].fill(Class::Keyword);
                i = end;
                continue;
            }
            if c == '>' {
                in_tag = false;
            }
            if in_tag && (c == '"' || c == '\'') {
                i = string(&chars, &mut out, i, c, false);
                continue;
            }
            i += 1;
            continue;
        }
        if line_comment.iter().any(|m| at(i, m))
            && (language != "shell" || i == 0 || chars[i - 1].is_whitespace())
        {
            let mut end = i;
            while end < chars.len() && chars[end] != '\n' {
                end += 1;
            }
            out[i..end].fill(Class::Comment);
            i = end;
            continue;
        }
        if quotes.contains(&c) {
            // Python triple quotes span lines; others stop at an unescaped quote.
            let triple = language == "python" && at(i, &c.to_string().repeat(3));
            i = string(&chars, &mut out, i, c, triple);
            continue;
        }
        let word_start = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        if c.is_ascii_digit() && word_start {
            let mut end = i;
            while end < chars.len()
                && (chars[end].is_ascii_alphanumeric() || chars[end] == '.' || chars[end] == '_')
            {
                end += 1;
            }
            out[i..end].fill(Class::Number);
            i = end;
            continue;
        }
        if (c.is_alphabetic() || c == '_' || (c == '#' && language == "c")) && word_start {
            let mut end = i + 1;
            while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            let word: String = chars[i..end].iter().collect();
            if keyword(language, word.trim_start_matches('#')) {
                out[i..end].fill(Class::Keyword);
            }
            i = end;
            continue;
        }
        i += 1;
    }
    out
}
fn string(chars: &[char], out: &mut [Class], start: usize, quote: char, triple: bool) -> usize {
    let width = if triple { 3 } else { 1 };
    let mut end = start + width;
    while end < chars.len() {
        if chars[end] == '\\' {
            end += 2;
            continue;
        }
        if !triple && chars[end] == '\n' && quote != '`' {
            break;
        }
        if chars[end] == quote && (!triple || chars[end..].iter().take(3).all(|c| *c == quote)) {
            end += width;
            break;
        }
        end += 1;
    }
    let end = end.min(chars.len());
    out[start..end].fill(Class::String);
    end
}
