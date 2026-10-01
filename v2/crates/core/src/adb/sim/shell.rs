//! A small POSIX-ish shell interpreter for the simulated device.
//!
//! The app never sends a bare command: reads are batched with `;`, `( … )`
//! subshells, `2>/dev/null`, `printf … $?` status markers and a trailing
//! `true` or `:`. Answering those strings by substring would let the
//! simulator agree with whatever the app happened to send, so instead the
//! string is parsed and every simple command is run against the device model
//! in order, with `$?` and the final exit status tracked the way `sh` does:
//! the *last* command's status is the status of the whole invocation.

/// One parsed word. Kept as parts so `$?` expands at run time, after the
/// command before it has finished.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Part {
    Lit(String),
    Status,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Word {
    parts: Vec<Part>,
    /// An unquoted `*` was present, so the word is a glob pattern.
    pub glob: bool,
}

impl Word {
    pub fn expand(&self, status: i32) -> String {
        self.parts
            .iter()
            .map(|p| match p {
                Part::Lit(s) => s.clone(),
                Part::Status => status.to_string(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(Word),
    /// `;`, `&&`, `||`, `|`, `(`, `)`, newline (as `;`)
    Op(&'static str),
    /// fd, target (`/dev/null`, `&1`)
    Redir(u8, String),
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Redirs {
    pub drop_stdout: bool,
    pub drop_stderr: bool,
    pub stderr_to_stdout: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Command {
    Simple(Vec<Word>, Redirs),
    Subshell(Box<List>, Redirs),
}

#[derive(Debug, Clone)]
pub(crate) struct Pipeline(pub Vec<Command>);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Joiner {
    Seq,
    And,
    Or,
}

#[derive(Debug, Clone)]
pub(crate) struct List(pub Vec<(Joiner, Pipeline)>);

fn tokenize(input: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {
                i += 1;
            }
            '\n' | ';' => {
                out.push(Token::Op(";"));
                i += 1;
            }
            '(' => {
                out.push(Token::Op("("));
                i += 1;
            }
            ')' => {
                out.push(Token::Op(")"));
                i += 1;
            }
            '&' if chars.get(i + 1) == Some(&'&') => {
                out.push(Token::Op("&&"));
                i += 2;
            }
            '|' if chars.get(i + 1) == Some(&'|') => {
                out.push(Token::Op("||"));
                i += 2;
            }
            '|' => {
                out.push(Token::Op("|"));
                i += 1;
            }
            _ => {
                // Redirection: optional fd digit then `>`.
                let (fd, skip) = if c == '>' {
                    (1u8, 1)
                } else if c.is_ascii_digit() && chars.get(i + 1) == Some(&'>') {
                    (c.to_digit(10).unwrap() as u8, 2)
                } else {
                    (0, 0)
                };
                if skip > 0 {
                    i += skip;
                    let mut target = String::new();
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    while i < chars.len() && !" \t\n;|&()".contains(chars[i]) {
                        target.push(chars[i]);
                        i += 1;
                    }
                    // `2>&1`: the `&` was consumed as part of the target.
                    if target.is_empty() && chars.get(i) == Some(&'&') {
                        target.push('&');
                        i += 1;
                        while i < chars.len() && chars[i].is_ascii_digit() {
                            target.push(chars[i]);
                            i += 1;
                        }
                    }
                    out.push(Token::Redir(fd, target));
                    continue;
                }
                let mut parts: Vec<Part> = Vec::new();
                let mut lit = String::new();
                let mut glob = false;
                while i < chars.len() && !" \t\n;|&()".contains(chars[i]) {
                    let ch = chars[i];
                    match ch {
                        '\'' => {
                            i += 1;
                            while i < chars.len() && chars[i] != '\'' {
                                lit.push(chars[i]);
                                i += 1;
                            }
                            if i >= chars.len() {
                                return Err("unterminated single quote".into());
                            }
                            i += 1;
                        }
                        '"' => {
                            i += 1;
                            while i < chars.len() && chars[i] != '"' {
                                if chars[i] == '\\'
                                    && i + 1 < chars.len()
                                    && "\"\\$`".contains(chars[i + 1])
                                {
                                    lit.push(chars[i + 1]);
                                    i += 2;
                                    continue;
                                }
                                if chars[i] == '$' && chars.get(i + 1) == Some(&'?') {
                                    if !lit.is_empty() {
                                        parts.push(Part::Lit(std::mem::take(&mut lit)));
                                    }
                                    parts.push(Part::Status);
                                    i += 2;
                                    continue;
                                }
                                lit.push(chars[i]);
                                i += 1;
                            }
                            if i >= chars.len() {
                                return Err("unterminated double quote".into());
                            }
                            i += 1;
                        }
                        '\\' => {
                            if let Some(next) = chars.get(i + 1) {
                                lit.push(*next);
                            }
                            i += 2;
                        }
                        '$' if chars.get(i + 1) == Some(&'?') => {
                            if !lit.is_empty() {
                                parts.push(Part::Lit(std::mem::take(&mut lit)));
                            }
                            parts.push(Part::Status);
                            i += 2;
                        }
                        '*' => {
                            glob = true;
                            lit.push('*');
                            i += 1;
                        }
                        _ => {
                            lit.push(ch);
                            i += 1;
                        }
                    }
                }
                if !lit.is_empty() || parts.is_empty() {
                    parts.push(Part::Lit(lit));
                }
                out.push(Token::Word(Word { parts, glob }));
            }
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn list(&mut self, nested: bool) -> Result<List, String> {
        let mut items = Vec::new();
        let mut joiner = Joiner::Seq;
        loop {
            match self.peek() {
                None => break,
                Some(Token::Op(")")) if nested => break,
                Some(Token::Op(";")) => {
                    self.pos += 1;
                    joiner = Joiner::Seq;
                    continue;
                }
                _ => {}
            }
            let pipeline = self.pipeline()?;
            items.push((joiner, pipeline));
            match self.peek() {
                Some(Token::Op("&&")) => {
                    self.pos += 1;
                    joiner = Joiner::And;
                }
                Some(Token::Op("||")) => {
                    self.pos += 1;
                    joiner = Joiner::Or;
                }
                Some(Token::Op(";")) => {
                    self.pos += 1;
                    joiner = Joiner::Seq;
                }
                Some(Token::Op(")")) if nested => break,
                None => break,
                Some(other) => return Err(format!("unexpected token {other:?}")),
            }
        }
        Ok(List(items))
    }

    fn pipeline(&mut self) -> Result<Pipeline, String> {
        let mut cmds = vec![self.command()?];
        while let Some(Token::Op("|")) = self.peek() {
            self.pos += 1;
            cmds.push(self.command()?);
        }
        Ok(Pipeline(cmds))
    }

    fn redirs(&mut self, r: &mut Redirs) {
        while let Some(Token::Redir(fd, target)) = self.peek().cloned() {
            self.pos += 1;
            apply_redir(r, fd, &target);
        }
    }

    fn command(&mut self) -> Result<Command, String> {
        if let Some(Token::Op("(")) = self.peek() {
            self.pos += 1;
            let inner = self.list(true)?;
            match self.peek() {
                Some(Token::Op(")")) => self.pos += 1,
                _ => return Err("unterminated subshell".into()),
            }
            let mut r = Redirs::default();
            self.redirs(&mut r);
            return Ok(Command::Subshell(Box::new(inner), r));
        }
        let mut words = Vec::new();
        let mut r = Redirs::default();
        loop {
            match self.peek().cloned() {
                Some(Token::Word(w)) => {
                    words.push(w);
                    self.pos += 1;
                }
                Some(Token::Redir(fd, target)) => {
                    self.pos += 1;
                    apply_redir(&mut r, fd, &target);
                }
                _ => break,
            }
        }
        if words.is_empty() {
            return Err("empty command".into());
        }
        Ok(Command::Simple(words, r))
    }
}

fn apply_redir(r: &mut Redirs, fd: u8, target: &str) {
    match (fd, target) {
        (2, "&1") => r.stderr_to_stdout = true,
        (2, _) => r.drop_stderr = true,
        (_, _) => r.drop_stdout = true,
    }
}

pub(crate) fn parse(input: &str) -> Result<List, String> {
    let tokens = tokenize(input)?;
    let mut p = Parser { tokens, pos: 0 };
    let list = p.list(false)?;
    if p.pos != p.tokens.len() {
        return Err("trailing tokens".into());
    }
    Ok(list)
}

/// Output of one command or of a whole invocation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Out {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Out {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: String::new(),
            code: 0,
        }
    }
    pub fn err(stderr: impl Into<String>, code: i32) -> Self {
        Self {
            stdout: String::new(),
            stderr: stderr.into(),
            code,
        }
    }
    pub fn with_code(stdout: impl Into<String>, code: i32) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: String::new(),
            code,
        }
    }
}

/// What runs one simple command. Implemented by the device model.
pub(crate) trait Exec {
    fn exec(&mut self, argv: &[String], globbed: &[bool], stdin: Option<&str>) -> Out;
    /// Stop executing (a timeout fault fired).
    fn aborted(&self) -> bool {
        false
    }
}

struct Run {
    status: i32,
    exited: bool,
}

pub(crate) fn run(input: &str, exec: &mut dyn Exec) -> Out {
    let list = match parse(input) {
        Ok(l) => l,
        Err(e) => return Out::err(format!("/system/bin/sh: syntax error: {e}\n"), 2),
    };
    let mut state = Run {
        status: 0,
        exited: false,
    };
    let mut out = Out::default();
    run_list(&list, exec, &mut state, &mut out);
    out.code = state.status;
    out
}

fn run_list(list: &List, exec: &mut dyn Exec, state: &mut Run, out: &mut Out) {
    for (joiner, pipeline) in &list.0 {
        if state.exited || exec.aborted() {
            return;
        }
        match joiner {
            Joiner::And if state.status != 0 => continue,
            Joiner::Or if state.status == 0 => continue,
            _ => {}
        }
        let mut stdin: Option<String> = None;
        let last = pipeline.0.len() - 1;
        for (i, cmd) in pipeline.0.iter().enumerate() {
            let piped = i < last;
            let result = run_command(cmd, exec, state, stdin.as_deref());
            if piped {
                out.stderr.push_str(&result.stderr);
                stdin = Some(result.stdout);
            } else {
                out.stdout.push_str(&result.stdout);
                out.stderr.push_str(&result.stderr);
                state.status = result.code;
            }
            if state.exited {
                return;
            }
        }
    }
}

fn run_command(cmd: &Command, exec: &mut dyn Exec, state: &mut Run, stdin: Option<&str>) -> Out {
    let (mut result, redirs) = match cmd {
        Command::Subshell(list, r) => {
            let mut inner = Run {
                status: state.status,
                exited: false,
            };
            let mut o = Out::default();
            run_list(list, exec, &mut inner, &mut o);
            o.code = inner.status;
            (o, r)
        }
        Command::Simple(words, r) => {
            let argv: Vec<String> = words.iter().map(|w| w.expand(state.status)).collect();
            let globbed: Vec<bool> = words.iter().map(|w| w.glob).collect();
            let o = if argv[0] == "exit" {
                state.exited = true;
                Out::with_code("", argv.get(1).and_then(|c| c.parse().ok()).unwrap_or(0))
            } else {
                exec.exec(&argv, &globbed, stdin)
            };
            (o, r)
        }
    };
    if redirs.stderr_to_stdout {
        let err = std::mem::take(&mut result.stderr);
        result.stdout.push_str(&err);
    }
    if redirs.drop_stderr {
        result.stderr.clear();
    }
    if redirs.drop_stdout {
        result.stdout.clear();
    }
    result
}

/// `printf FORMAT ARGS…` with `%s`, `%d` and the usual backslash escapes.
pub(crate) fn printf(format: &str, args: &[String]) -> String {
    let mut out = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;
    let mut arg = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() => {
                match chars[i + 1] {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
                i += 2;
            }
            '%' if i + 1 < chars.len() => {
                match chars[i + 1] {
                    's' | 'd' => {
                        out.push_str(args.get(arg).map(String::as_str).unwrap_or(""));
                        arg += 1;
                    }
                    '%' => out.push('%'),
                    other => {
                        out.push('%');
                        out.push(other);
                    }
                }
                i += 2;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Shell-style glob match supporting `*` only.
pub(crate) fn glob_match(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let mut rest = text;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else {
            match rest.find(part) {
                Some(at) => rest = &rest[at + part.len()..],
                None => return false,
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo(Vec<Vec<String>>);

    impl Exec for Echo {
        fn exec(&mut self, argv: &[String], _: &[bool], stdin: Option<&str>) -> Out {
            self.0.push(argv.to_vec());
            match argv[0].as_str() {
                "echo" => Out::ok(format!("{}\n", argv[1..].join(" "))),
                "printf" => Out::ok(printf(&argv[1], &argv[2..])),
                "true" | ":" => Out::ok(""),
                "false" => Out::with_code("", 1),
                "fail" => Out::err("boom\n", 3),
                "wc" => Out::ok(format!("{}\n", stdin.unwrap_or("").lines().count())),
                _ => Out::err("not found\n", 127),
            }
        }
    }

    #[test]
    fn last_command_status_wins_and_status_markers_expand() {
        let mut e = Echo(vec![]);
        let out = run(
            "(fail) 2>/dev/null; printf '\\n__S__%s\\n' $?; echo __SEP__; false",
            &mut e,
        );
        assert_eq!(out.stdout, "\n__S__3\n__SEP__\n");
        assert_eq!(out.stderr, "");
        assert_eq!(out.code, 1);
        let out = run("false; true", &mut e);
        assert_eq!(out.code, 0);
    }

    #[test]
    fn quotes_and_and_or() {
        let mut e = Echo(vec![]);
        let out = run("echo 'a b' && fail || echo \"c $?\"", &mut e);
        assert_eq!(out.stdout, "a b\nc 3\n");
        assert_eq!(out.stderr, "boom\n");
        let out = run("echo one | wc", &mut e);
        assert_eq!(out.stdout, "1\n");
    }

    #[test]
    fn globs() {
        assert!(glob_match("/vendor/etc/media_codecs*.xml", "/vendor/etc/media_codecs.xml"));
        assert!(!glob_match("/odm/etc/media_codecs*.xml", "/vendor/etc/media_codecs.xml"));
        assert!(glob_match("a*b*c", "aXXbYYc"));
    }
}
