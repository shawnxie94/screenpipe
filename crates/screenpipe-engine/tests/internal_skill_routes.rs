// screenpipe — AI that knows everything you've seen, said, or heard
// https://screenpipe.com

//! Every HTTP endpoint cited by the internal knowledge skills must exist in
//! this crate's route table at the prefix it is really served under, with the
//! method it is really served with.
//!
//! The skills are agent-facing instructions: a wrong path there turns into a
//! failed session, and nothing else in the build checks it. This test
//! reconstructs the effective route set statically:
//!
//! * per-file route literals and their methods (`.get("…", h)`,
//!   `.route("…", get(h))`, plus `-> MethodRouter` builders resolved through
//!   their body);
//! * real nest attribution: a literal is prefixed by a `.nest(…)` prefix only
//!   when the router-builder function that defines it is the one referenced
//!   inside that nest expression, matched as `module::fn_name(` (bare names
//!   only when unique across the crate). Literals behind locally built
//!   sub-routers (`nest("/pipes", pipe_routes)`) cannot be attributed and
//!   stay at root — a documented over-approximation, never a cross-router
//!   product.
//!
//! The earlier version of this test combined every nest prefix with every
//! literal and ignored methods, so it accepted `GET /knowledge` (the list
//! actually lives at `/knowledge/knowledge` under the nest), `GET
//! /knowledge/:id`, the invented `/knowledge/activity-intervals`, and any
//! wrong method. The regression test below pins those rejections.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const SKILLS: [&str; 4] = [
    "knowledge-fetch",
    "activity-summary",
    "work-unit",
    "knowledge-distill",
];

const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

fn engine_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn core_skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("screenpipe-core")
        .join("assets")
        .join("skills")
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

// --- lexical helpers (comment- and string-aware, offset preserving) ---

/// Blank out comments with spaces (newlines kept) so the brace/paren
/// matchers below never trip over prose. Strings are skipped so `//` inside
/// URLs survives.
fn blank_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let mut depth = 1usize;
                out[i] = b' ';
                out[i + 1] = b' ';
                i += 2;
                while i < bytes.len() && depth > 0 {
                    if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                        continue;
                    }
                    if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                        continue;
                    }
                    if bytes[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn matching_delimiter(bytes: &[u8], open: usize, open_ch: u8, close_ch: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'\'' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b if b == open_ch => depth += 1,
            b if b == close_ch => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn identifier_at(source: &str, start: usize) -> String {
    let bytes = source.as_bytes();
    let mut end = start;
    if end >= bytes.len() || !(bytes[end] == b'_' || bytes[end].is_ascii_alphabetic()) {
        return String::new();
    }
    while end < bytes.len() && (bytes[end] == b'_' || bytes[end].is_ascii_alphanumeric()) {
        end += 1;
    }
    source[start..end].to_string()
}

/// Remove `#[cfg(test)] mod … { … }` blocks (and bare `#[cfg(test)]` items)
/// so test-only routers never inflate the production route set.
fn strip_test_modules(source: &str) -> String {
    const MARKER: &str = "#[cfg(test)]";
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    while let Some(at) = source[cursor..].find(MARKER) {
        let marker = cursor + at;
        out.push_str(&source[cursor..marker]);
        let after = &source[marker + MARKER.len()..];
        let Some(brace_rel) = after.find('{') else {
            cursor = marker + MARKER.len();
            continue;
        };
        if after.find(';').is_some_and(|semi| semi < brace_rel) {
            // `#[cfg(test)] mod name;` — nothing inline to strip.
            cursor = marker + MARKER.len() + after.find(';').unwrap() + 1;
            continue;
        }
        let open = marker + MARKER.len() + brace_rel;
        let Some(close) = matching_delimiter(bytes, open, b'{', b'}') else {
            cursor = marker + MARKER.len();
            continue;
        };
        out.push('\n');
        cursor = close + 1;
    }
    out.push_str(&source[cursor..]);
    out
}

// --- route extraction ---

struct FileRoute {
    literal: String,
    methods: HashSet<String>,
    /// `fn_name` invoked as the method-router argument of `.route(path, fn())`
    /// when no method keyword is visible at the call site.
    method_router_fn: Option<String>,
}

struct BuilderFn {
    name: String,
    is_method_router: bool,
    /// For `-> MethodRouter` builders: methods found in the body.
    methods: HashSet<String>,
}

struct NestSite {
    prefix: String,
    expr: String,
}

fn method_keywords(expr: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for (needle, method) in [
        ("get(", "GET"),
        ("post(", "POST"),
        ("put(", "PUT"),
        ("patch(", "PATCH"),
        ("delete(", "DELETE"),
    ] {
        let mut cursor = 0usize;
        while let Some(at) = expr[cursor..].find(needle) {
            let at = cursor + at;
            let boundary = at == 0
                || !expr[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
            if boundary {
                out.insert(method.to_string());
            }
            cursor = at + needle.len();
        }
    }
    out
}

/// `…::answer_route()` at the end of a `.route(path, EXPR)` argument.
fn trailing_fn_call(expr: &str) -> Option<String> {
    let trimmed = expr.trim().trim_end_matches(',').trim_end();
    let close = trimmed.rfind(')')?;
    if !trimmed[close + 1..].trim().is_empty() {
        return None;
    }
    let open = trimmed[..close].rfind('(')?;
    let head = trimmed[..open].trim();
    let name = head.rsplit("::").next()?.trim();
    if !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(name.to_string())
    } else {
        None
    }
}

fn extract_file_routes(cleaned: &str) -> Vec<FileRoute> {
    let bytes = cleaned.as_bytes();
    let mut out: Vec<FileRoute> = Vec::new();

    for (marker, method) in [
        (".get(", "GET"),
        (".post(", "POST"),
        (".put(", "PUT"),
        (".patch(", "PATCH"),
        (".delete(", "DELETE"),
    ] {
        let mut cursor = 0usize;
        while let Some(at) = cleaned[cursor..].find(marker) {
            let at = cursor + at;
            let trimmed = cleaned[at + marker.len()..].trim_start();
            if let Some(rest) = trimmed.strip_prefix('"') {
                if let Some(end) = rest.find('"') {
                    let literal = &rest[..end];
                    if literal.starts_with('/') {
                        out.push(FileRoute {
                            literal: literal.to_string(),
                            methods: HashSet::from([method.to_string()]),
                            method_router_fn: None,
                        });
                    }
                }
            }
            cursor = at + marker.len();
        }
    }

    let mut cursor = 0usize;
    while let Some(at) = cleaned[cursor..].find(".route(") {
        let at = cursor + at;
        let open = at + ".route(".len() - 1;
        let close = matching_delimiter(bytes, open, b'(', b')').unwrap_or(open);
        let inner = &cleaned[open + 1..close];
        if let Some(q1) = inner.find('"') {
            if let Some(q2rel) = inner[q1 + 1..].find('"') {
                let literal = &inner[q1 + 1..q1 + 1 + q2rel];
                if literal.starts_with('/') {
                    let expr = &inner[q1 + 1 + q2rel + 1..];
                    let methods = method_keywords(expr);
                    let method_router_fn = if methods.is_empty() {
                        trailing_fn_call(expr)
                    } else {
                        None
                    };
                    out.push(FileRoute {
                        literal: literal.to_string(),
                        methods,
                        method_router_fn,
                    });
                }
            }
        }
        cursor = close + 1;
    }

    out
}

fn extract_builders(cleaned: &str) -> Vec<BuilderFn> {
    let bytes = cleaned.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(at) = cleaned[cursor..].find("fn ") {
        let at = cursor + at;
        let name_start = at + "fn ".len();
        let name = identifier_at(cleaned, name_start);
        if !name.is_empty() {
            if let Some(brace_rel) = cleaned[name_start..].find('{') {
                let brace = name_start + brace_rel;
                let signature = &cleaned[name_start..brace];
                let builds_router = signature.contains("Router");
                let is_method_router = signature.contains("MethodRouter");
                if builds_router {
                    let methods = if is_method_router {
                        matching_delimiter(bytes, brace, b'{', b'}')
                            .map(|close| method_keywords(&cleaned[brace..=close]))
                            .unwrap_or_default()
                    } else {
                        HashSet::new()
                    };
                    out.push(BuilderFn {
                        name,
                        is_method_router,
                        methods,
                    });
                }
            }
        }
        cursor = at + "fn ".len();
    }
    out
}

fn extract_nests(cleaned: &str) -> Vec<NestSite> {
    let bytes = cleaned.as_bytes();
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(at) = cleaned[cursor..].find(".nest(") {
        let at = cursor + at;
        let open = at + ".nest(".len() - 1;
        let Some(close) = matching_delimiter(bytes, open, b'(', b')') else {
            cursor = at + ".nest(".len();
            continue;
        };
        let inner = &cleaned[open + 1..close];
        if let Some(q1) = inner.find('"') {
            if let Some(q2rel) = inner[q1 + 1..].find('"') {
                out.push(NestSite {
                    prefix: inner[q1 + 1..q1 + 1 + q2rel].to_string(),
                    expr: inner[q1 + 1 + q2rel + 1..].to_string(),
                });
            }
        }
        cursor = close + 1;
    }
    out
}

fn join_path(prefix: &str, literal: &str) -> String {
    let mut joined = prefix.trim_end_matches('/').to_string();
    if !literal.starts_with('/') {
        joined.push('/');
    }
    joined.push_str(literal);
    joined
}

fn normalize(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for segment in path.split('/') {
        out.push('/');
        if segment.starts_with(':') {
            out.push(':');
        } else {
            out.push_str(segment);
        }
    }
    out
}

/// path -> set of methods actually routed there.
fn effective_routes() -> HashMap<String, HashSet<String>> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rs_files(&engine_src(), &mut files);
    files.sort();

    let per_file: Vec<(String, String, Vec<FileRoute>, Vec<BuilderFn>)> = files
        .iter()
        .map(|path| {
            let cleaned = strip_test_modules(&blank_comments(
                &std::fs::read_to_string(path).unwrap_or_default(),
            ));
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let routes = extract_file_routes(&cleaned);
            let builders = extract_builders(&cleaned);
            (stem, cleaned, routes, builders)
        })
        .collect();

    let nests: Vec<NestSite> = per_file
        .iter()
        .flat_map(|(_, cleaned, _, _)| extract_nests(cleaned))
        .collect();

    let builders_named = |name: &str| {
        per_file
            .iter()
            .flat_map(|(_, _, _, builders)| builders.iter())
            .filter(|builder| builder.name == name)
            .count()
    };

    // Nest attribution, per builder fn; a file inherits the union of the
    // prefixes its builders are mounted under.
    let mut file_prefixes: Vec<HashSet<String>> = vec![HashSet::new(); per_file.len()];
    for site in &nests {
        for (index, (stem, _, _, builders)) in per_file.iter().enumerate() {
            for builder in builders {
                let qualified = format!("{stem}::{}(", builder.name);
                let bare = format!("{}(", builder.name);
                let attributed = site.expr.contains(&qualified)
                    || (site.expr.contains(&bare) && builders_named(&builder.name) == 1);
                if attributed {
                    file_prefixes[index].insert(site.prefix.clone());
                }
            }
        }
    }

    // Method-router builders (`.route(path, fn())` with no visible method
    // keyword) are resolved by fn name across the whole crate.
    let mut method_routers: HashMap<String, HashSet<String>> = HashMap::new();
    for (_, _, _, builders) in &per_file {
        for builder in builders {
            if builder.is_method_router {
                method_routers
                    .entry(builder.name.clone())
                    .or_default()
                    .extend(builder.methods.iter().cloned());
            }
        }
    }

    let mut routes: HashMap<String, HashSet<String>> = HashMap::new();
    let mut add =
        |routes: &mut HashMap<String, HashSet<String>>, path: String, methods: &HashSet<String>| {
            routes
                .entry(normalize(&path))
                .or_default()
                .extend(methods.iter().cloned());
        };
    for (index, (_, _, file_routes, builders)) in per_file.iter().enumerate() {
        for route in file_routes {
            let mut methods = route.methods.clone();
            if methods.is_empty() {
                if let Some(name) = &route.method_router_fn {
                    if let Some(resolved) = method_routers.get(name) {
                        methods = resolved.clone();
                    }
                }
            }
            if file_prefixes[index].is_empty() {
                add(&mut routes, route.literal.clone(), &methods);
            } else {
                for prefix in &file_prefixes[index] {
                    let mounted = join_path(prefix, &route.literal);
                    add(&mut routes, mounted, &methods);
                }
            }
        }
    }
    routes
}

fn is_served(routes: &HashMap<String, HashSet<String>>, method: &str, path: &str) -> bool {
    routes
        .get(&normalize(path))
        .is_some_and(|methods| methods.contains(method))
}

/// `METHOD /path` pairs cited by a skill body.
fn cited_endpoints(body: &str) -> Vec<(String, String)> {
    let tokens: Vec<&str> = body.split_whitespace().collect();
    let mut out = Vec::new();
    for window in tokens.windows(2) {
        let method = window[0].trim_matches(|c: char| c == '`' || c == '*' || c == '（' || c == '(');
        if !METHODS.contains(&method) {
            continue;
        }
        let raw = window[1]
            .trim_matches(|c: char| matches!(c, '`' | '*' | '"' | '）' | ')' | ',' | '。' | ':' | '；'));
        if !raw.starts_with('/') {
            continue;
        }
        let path = raw.split(['?', '#']).next().unwrap_or(raw).trim_end_matches(['.', ',']);
        if path.len() > 1 {
            out.push((method.to_string(), path.to_string()));
        }
    }
    out
}

#[test]
fn internal_skill_endpoints_exist_in_the_engine_router() {
    let routes = effective_routes();
    assert!(
        routes.len() > 50,
        "route derivation looks wrong: only {} paths found",
        routes.len()
    );

    // Spot-check the model against routes whose real location this test once
    // got wrong: nested knowledge paths, root activity paths, and a
    // MethodRouter-mounted endpoint.
    for (method, path) in [
        ("GET", "/knowledge/knowledge"),
        ("GET", "/knowledge/knowledge/:id"),
        ("GET", "/knowledge/work-units"),
        ("GET", "/knowledge/work-units/:id"),
        ("GET", "/knowledge/status"),
        ("GET", "/activity-intervals"),
        ("GET", "/activity-intervals/:interval_id/evidence"),
        ("POST", "/answer"),
    ] {
        assert!(
            is_served(&routes, method, path),
            "{method} {path} must be derived as served"
        );
    }

    let mut checked = 0usize;
    let mut missing: Vec<String> = Vec::new();
    for skill in SKILLS {
        let path = core_skills_dir().join(skill).join("SKILL.md");
        let body = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for (method, cited) in cited_endpoints(&body) {
            checked += 1;
            if !is_served(&routes, &method, &cited) {
                missing.push(format!("{skill}: {method} {cited}"));
            }
        }
    }

    assert!(checked >= 8, "endpoint scan found too few citations: {checked}");
    assert!(
        missing.is_empty(),
        "skills cite endpoints the engine does not serve:\n  {}",
        missing.join("\n  ")
    );
}

/// Regression: the first version of this test treated every literal as a
/// top-level route and cross-combined every nest prefix with every literal,
/// so invented paths and wrong methods passed. Every entry here must stay
/// rejected.
#[test]
fn internal_skill_route_probes_reject_wrong_prefixes_methods_and_invented_paths() {
    let routes = effective_routes();
    let rejected: [(&str, &str); 8] = [
        // The knowledge list/detail live under the /knowledge nest at
        // /knowledge/knowledge[/:id]; bare forms were never mounted.
        ("GET", "/knowledge"),
        ("GET", "/knowledge/:id"),
        // Invented cross-router combinations: /activity-intervals and /search
        // exist only at the root, never under the /knowledge nest.
        ("GET", "/knowledge/activity-intervals"),
        ("GET", "/knowledge/activity-ledger"),
        ("GET", "/knowledge/search"),
        // Method violations on real paths.
        ("POST", "/activity-intervals"),
        ("POST", "/knowledge/knowledge"),
        ("DELETE", "/frames/:frame_id/text"),
    ];
    for (method, path) in rejected {
        assert!(
            !is_served(&routes, method, path),
            "{method} {path} must not be served but the derived route set accepts it"
        );
    }
}
