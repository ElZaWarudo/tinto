//! A QA job's visual changes record (`<KEY>.md` in its QA folder), rendered
//! to one self-contained HTML page for the user's local review: screenshots
//! embedded, grouped per change in a desktop browser frame and a phone frame,
//! with tabs for states and a full-screen viewer. A port of the
//! backlog-delivery skill's `visual_record_html.py`, so the page looks the
//! same; it is never committed or sent anywhere.

use std::path::{Path, PathBuf};

use base64::Engine;

const CSS: &str = include_str!("visual_record.css");
const JS: &str = include_str!("visual_record.js");

/// Renders `<KEY>.md` to `<KEY>.html` beside it and returns that path.
/// Fails, naming it, when a screenshot the record shows is missing.
pub fn render_beside(record: &Path) -> Result<PathBuf, String> {
    let page = render(record)?;
    let out = record.with_extension("html");
    std::fs::write(&out, page).map_err(|error| error.to_string())?;
    Ok(out)
}

/// The record Tinto writes for a QA job that ended without one (its time ran
/// out, its process was killed), so its evidence is not lost: a header, the
/// items it was to check, each «not verifiable», and the screenshots it left
/// in `dir`. `items` are the agent's task list with whether it finished
/// each, else what the job was to check again.
pub fn write_stub(
    dir: &Path,
    key: &str,
    branch: &str,
    date: &str,
    why: &str,
    items: &[(String, bool)],
) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut shots: Vec<String> = std::fs::read_dir(dir)?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            [".png", ".jpg", ".jpeg"]
                .iter()
                .any(|ext| lower.ends_with(ext))
        })
        .collect();
    shots.sort();
    let mut lines = vec![
        format!("# {key} — QA record written by Tinto: {why}"),
        String::new(),
        "| Test environment | |".to_string(),
        "| --- | --- |".to_string(),
        format!("| Date | {date} |"),
        format!("| Branch | `{branch}` |"),
        "| Environment | as the job found it; the agent did not write its record |".to_string(),
        String::new(),
        format!("The QA job ended before it wrote its record ({why}). Tinto lists what it was checking and the screenshots it took; nothing here is a verdict."),
        String::new(),
        "## Verdicts".to_string(),
        String::new(),
        "| Item | Verdict |".to_string(),
        "| --- | --- |".to_string(),
    ];
    let fallback = [(format!("QA of {key}"), false)];
    let items = if items.is_empty() {
        &fallback[..]
    } else {
        items
    };
    for (number, (item, done)) in items.iter().enumerate() {
        let item = item.replace('|', "/");
        let verdict = match done {
            true => "not verifiable: out of time (checked, but no verdict was written)",
            false => "not verifiable: out of time",
        };
        lines.push(format!("| {}. {item} | {verdict} |", number + 1));
    }
    if !shots.is_empty() {
        lines.extend([String::new(), "## Screenshots".to_string()]);
        for shot in &shots {
            lines.extend([String::new(), format!("![]({shot})")]);
        }
    }
    lines.extend([
        String::new(),
        "**Problems:** Not recorded: the job ended before it wrote them.".to_string(),
        String::new(),
    ]);
    let path = dir.join(format!("{key}.md"));
    std::fs::write(&path, lines.join("\n"))?;
    Ok(path)
}

/// `YYYY-MM-DD` (UTC) of a time in milliseconds since the epoch.
pub fn date_of(ms: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let z = (ms / 86_400_000) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The record a QA job wrote, and the other candidates beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordFile {
    pub source: PathBuf,
    pub extras: Vec<PathBuf>,
}

/// Finds a QA job's record in its QA folder: `<KEY>.md`, else a
/// `<KEY>.html` the agent wrote itself, else the newest `<KEY>-*.md` or
/// `<KEY>_*.md`. The other `<KEY>-*.md` and `<KEY>_*.md` are its extras.
pub fn find(dir: &Path, key: &str) -> Option<RecordFile> {
    let mut named: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(key)?;
            let meta = entry.metadata().ok()?;
            if !(rest.starts_with(['-', '_']) && rest.ends_with(".md") && meta.is_file()) {
                return None;
            }
            Some((meta.modified().ok()?, entry.path()))
        })
        .collect();
    // Newest first; the name settles a tie.
    named.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut others: Vec<PathBuf> = named.into_iter().map(|(_, path)| path).collect();
    let source = [
        dir.join(format!("{key}.md")),
        dir.join(format!("{key}.html")),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .or_else(|| (!others.is_empty()).then(|| others.remove(0)))?;
    Some(RecordFile {
        source,
        extras: others,
    })
}

/// The page the user opens for a record: an HTML record as it is, a
/// Markdown one rendered beside it.
pub fn page_for(record: &Path) -> Result<PathBuf, String> {
    if record.extension().is_some_and(|ext| ext == "html") {
        return Ok(record.to_path_buf());
    }
    render_beside(record)
}

/// What the QA record guide asks for and a Markdown record lacks: the
/// header table (its date) and the verdict table or matrix (pass / fail /
/// not verifiable per item).
pub fn missing_parts(text: &str) -> Vec<String> {
    let rows: Vec<Vec<String>> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|'))
        .map(|line| {
            line.trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().trim_matches('*').trim().to_lowercase())
                .collect()
        })
        .collect();
    let header = rows.iter().any(|cells| {
        cells
            .first()
            .is_some_and(|cell| cell == "date" || cell == "fecha")
    });
    let verdict = rows.iter().flatten().any(|cell| {
        let word = cell.trim_start_matches(|c: char| !c.is_alphanumeric());
        ["pass", "fail", "not verif", "pasa", "falla", "no verific"]
            .iter()
            .any(|verdict| word.starts_with(verdict))
    });
    let mut missing = Vec::new();
    if !header {
        missing.push(
            "Le falta la tabla de cabecera (fecha, rama, entorno y, si hay interfaz, viewports)"
                .to_string(),
        );
    }
    if !verdict {
        missing.push(
            "Le falta la tabla de veredictos o la matriz (cada punto → pass / fail / not verifiable)"
                .to_string(),
        );
    }
    missing
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#x27;"),
            c => out.push(c),
        }
    }
    out
}

/// Replaces each pair of `mark` with `open`…`close`; an unpaired one stays.
fn pairs(text: &str, mark: &str, open: &str, close: &str) -> String {
    let parts: Vec<&str> = text.split(mark).collect();
    if parts.len() < 3 {
        return text.to_string();
    }
    let paired = (parts.len() - 1) / 2 * 2;
    let mut out = String::from(parts[0]);
    for (index, part) in parts[1..].iter().enumerate() {
        if index < paired {
            out.push_str(if index % 2 == 0 { open } else { close });
        } else {
            out.push_str(mark);
        }
        out.push_str(part);
    }
    out
}

/// `code`, **bold** and *emphasis* in escaped text.
fn inline(text: &str) -> String {
    let text = pairs(&escape(text), "`", "<code>", "</code>");
    let text = pairs(&text, "**", "<strong>", "</strong>");
    pairs(&text, "*", "<em>", "</em>")
}

/// Every `![label](ref)` in the text.
fn images(text: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("![") {
        let after = &rest[start + 2..];
        let Some(label_end) = after.find("](") else {
            break;
        };
        let target = &after[label_end + 2..];
        let Some(ref_end) = target.find(')') else {
            break;
        };
        found.push((
            after[..label_end].to_string(),
            target[..ref_end].trim().to_string(),
        ));
        rest = &target[ref_end + 1..];
    }
    found
}

/// The text is only images (and spaces).
fn image_only(text: &str) -> bool {
    let found = images(text);
    if found.is_empty() {
        return false;
    }
    let mut rest = text.to_string();
    for (label, reference) in &found {
        rest = rest.replacen(&format!("![{label}]({reference})"), "", 1);
    }
    rest.trim().is_empty()
}

#[derive(Debug, Clone)]
struct Shot {
    label: String,
    reference: String,
    mobile: bool,
}

fn stem(reference: &str) -> String {
    Path::new(reference)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name(reference: &str) -> String {
    Path::new(reference)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn shot(label: &str, reference: &str) -> Shot {
    let name = stem(reference);
    let label = match label.trim() {
        "" => name
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches('-')
            .replace('-', " "),
        label => label.to_string(),
    };
    let text = format!("{label} {name}").to_lowercase();
    let mobile = ["mobile", "móvil", "movil"]
        .iter()
        .any(|word| text.contains(word));
    Shot {
        label,
        reference: reference.to_string(),
        mobile,
    }
}

/// "Desktop: menu" / "Mobile — toast" → "menu" / "toast".
fn tab_label(label: &str) -> String {
    let lower = label.trim_start().to_lowercase();
    if lower.starts_with("desktop") || lower.starts_with("mobile") {
        if let Some(at) = label.find([':', '—', '–', '-']) {
            let rest = label[at..].trim_start_matches([':', '—', '–', '-']).trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    label.to_string()
}

/// A caption that only says the viewport ("Desktop", "Mobile 375×812").
fn redundant_caption(label: &str) -> bool {
    let lower = label.trim().to_lowercase();
    let Some(rest) = lower
        .strip_prefix("desktop")
        .or_else(|| lower.strip_prefix("mobile"))
    else {
        return false;
    };
    let rest: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
    if rest.is_empty() {
        return true;
    }
    let mut sides = rest.split(['×', 'x']);
    let numeric = |side: Option<&str>| {
        side.is_some_and(|side| !side.is_empty() && side.chars().all(|c| c.is_ascii_digit()))
    };
    numeric(sides.next()) && numeric(sides.next()) && sides.next().is_none()
}

pub(super) fn data_uri(reference: &str, base: &Path) -> Result<String, String> {
    let source = base.join(reference);
    let bytes = std::fs::read(&source).map_err(|_| format!("falta la captura {reference}"))?;
    let mime = match source
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "image/jpeg",
    };
    Ok(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

fn render_panel(
    title: &str,
    shots: &[&Shot],
    base: &Path,
    section: &str,
    frame: &str,
) -> Result<String, String> {
    let tabs = if shots.len() > 1 {
        let buttons: String = shots
            .iter()
            .enumerate()
            .map(|(index, shot)| {
                // A long label is cut short on screen; the title keeps it whole.
                let label = tab_label(&shot.label);
                format!(
                    r#"<button role="tab" aria-selected="{}" title="{}">{}</button>"#,
                    index == 0,
                    escape(&label),
                    inline(&label)
                )
            })
            .collect();
        format!(r#"<div class="tabs" role="tablist">{buttons}</div>"#)
    } else {
        String::new()
    };
    let mut figures = String::new();
    for (index, shot) in shots.iter().enumerate() {
        figures.push_str(&format!(
            r#"<figure class="shot{}"><img src="{}" alt="{}" data-section="{}" data-file="{}" loading="lazy"><figcaption><code>{}</code></figcaption></figure>"#,
            if index == 0 { " on" } else { "" },
            data_uri(&shot.reference, base)?,
            escape(&shot.label),
            escape(section),
            escape(&shot.reference),
            escape(&file_name(&shot.reference))
        ));
    }
    let bar = if frame == "browser" {
        r#"<div class="bar"><i></i><i></i><i></i></div>"#
    } else {
        ""
    };
    let only = match shots {
        [only] => only.label.as_str(),
        _ => "",
    };
    let caption = if only.is_empty() || redundant_caption(only) {
        String::new()
    } else {
        format!(r#"<div class="caption">{}</div>"#, inline(only))
    };
    Ok(format!(
        r#"<div class="panel"><div class="title">{title}</div>{tabs}{caption}<div class="{frame}">{bar}{figures}</div></div>"#
    ))
}

fn render_shots(shots: &[Shot], base: &Path, section: &str) -> Result<String, String> {
    let desktop: Vec<&Shot> = shots.iter().filter(|shot| !shot.mobile).collect();
    let mobile: Vec<&Shot> = shots.iter().filter(|shot| shot.mobile).collect();
    let mut panels = String::new();
    if !desktop.is_empty() {
        panels.push_str(&render_panel(
            "Desktop", &desktop, base, section, "browser",
        )?);
    }
    if !mobile.is_empty() {
        panels.push_str(&render_panel("Mobile", &mobile, base, section, "phone")?);
    }
    let class = if !desktop.is_empty() && !mobile.is_empty() {
        "shots"
    } else {
        "shots single"
    };
    Ok(format!(r#"<div class="{class}">{panels}</div>"#))
}

/// `**Observations:** body` and the other labelled notes of a change.
fn callout(text: &str) -> Option<(String, String)> {
    let inner = text.strip_prefix("**")?;
    let end = inner.find("**")?;
    let name = inner[..end].strip_suffix(':')?;
    let kind = name.split('(').next().unwrap_or_default().trim();
    let known = [
        "observation",
        "observations",
        "problem",
        "problems",
        "suggested solution",
        "suggested solutions",
        "pending",
        "not visible in the browser",
        "note",
    ];
    known
        .contains(&kind.to_lowercase().as_str())
        .then(|| (kind.to_string(), inner[end + 2..].trim().to_string()))
}

fn render_callout(kind: &str, body: &str) -> String {
    let lower = kind.to_lowercase();
    let class = if lower.starts_with("problem") {
        "prob"
    } else if lower.starts_with("suggested") {
        "sol"
    } else if lower.starts_with("pending") {
        "pend"
    } else {
        "obs"
    };
    let empty = body.trim().trim_end_matches('.').to_lowercase();
    let none = if ["none", "ninguno", "n/a"].contains(&empty.as_str()) {
        " none"
    } else {
        ""
    };
    format!(
        r#"<div class="callout {class}{none}"><div class="h">{}</div>{}</div>"#,
        escape(kind),
        inline(body)
    )
}

enum Block {
    Heading(usize, String),
    Html(String),
    Shots(Vec<Shot>),
    Callout(String, String),
    Facts(Vec<(String, String)>),
    Label(String),
}

fn separator(cell: &str) -> bool {
    let cell = cell.trim_start_matches(':').trim_end_matches(':');
    !cell.is_empty() && cell.chars().all(|c| c == '-')
}

fn list_item(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("- ")
}

fn heading(line: &str) -> Option<(usize, String)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    let rest = line[level..].strip_prefix(' ')?;
    (1..=6)
        .contains(&level)
        .then(|| (level.min(3), rest.trim().to_string()))
}

fn parse(lines: &[&str]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() || line.trim() == "---" {
            i += 1;
            continue;
        }
        if let Some((level, text)) = heading(line) {
            blocks.push(Block::Heading(level, text));
            i += 1;
            continue;
        }
        if line.starts_with('|') {
            let mut rows: Vec<Vec<String>> = Vec::new();
            while i < lines.len() && lines[i].starts_with('|') {
                rows.push(
                    lines[i]
                        .trim()
                        .trim_matches('|')
                        .split('|')
                        .map(|cell| cell.trim().to_string())
                        .collect(),
                );
                i += 1;
            }
            let head = rows[0].clone();
            let body: Vec<Vec<String>> = rows[1..]
                .iter()
                .filter(|row| !row.iter().all(|cell| separator(cell)))
                .cloned()
                .collect();
            if body.iter().flatten().any(|cell| !images(cell).is_empty()) {
                let mut found = Vec::new();
                for row in &body {
                    for (column, cell) in row.iter().enumerate() {
                        for (label, reference) in images(cell) {
                            let label = match head.get(column) {
                                Some(title) if !title.is_empty() => title.clone(),
                                _ => label,
                            };
                            found.push(shot(&label, &reference));
                        }
                    }
                }
                blocks.push(Block::Shots(found));
            } else if head.iter().all(String::is_empty) && body.iter().all(|row| row.len() == 2) {
                blocks.push(Block::Facts(
                    body.into_iter()
                        .map(|row| (row[0].clone(), row[1].clone()))
                        .collect(),
                ));
            } else {
                let mut table = String::from(r#"<table class="plain"><thead><tr>"#);
                for cell in &head {
                    table.push_str(&format!("<th>{}</th>", inline(cell)));
                }
                table.push_str("</tr></thead><tbody>");
                for row in &body {
                    table.push_str("<tr>");
                    for cell in row {
                        table.push_str(&format!("<td>{}</td>", inline(cell)));
                    }
                    table.push_str("</tr>");
                }
                blocks.push(Block::Html(table + "</tbody></table>"));
            }
            continue;
        }
        if list_item(line).is_some() {
            let mut items = String::new();
            while let Some(item) = lines.get(i).and_then(|line| list_item(line)) {
                items.push_str(&format!("<li>{}</li>", inline(item)));
                i += 1;
            }
            blocks.push(Block::Html(format!("<ul>{items}</ul>")));
            continue;
        }
        let mut paragraph = Vec::new();
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && !(lines[i].starts_with('#')
                || lines[i].starts_with('|')
                || list_item(lines[i]).is_some()
                || lines[i].starts_with("---"))
        {
            paragraph.push(lines[i].trim());
            i += 1;
        }
        let text = paragraph.join(" ");
        if image_only(&text) {
            let label = match blocks.last() {
                Some(Block::Label(label)) => {
                    let label = label.clone();
                    blocks.pop();
                    label
                }
                _ => String::new(),
            };
            blocks.push(Block::Shots(
                images(&text)
                    .into_iter()
                    .map(|(alt, reference)| {
                        shot(if label.is_empty() { &alt } else { &label }, &reference)
                    })
                    .collect(),
            ));
        } else if let Some((kind, body)) = callout(&text) {
            blocks.push(Block::Callout(kind, body));
        } else if let Some(bold) = text
            .strip_prefix("**")
            .and_then(|rest| rest.strip_suffix("**"))
            .filter(|inner| !inner.is_empty() && !inner.contains('*'))
        {
            blocks.push(Block::Label(bold.to_string()));
        } else {
            blocks.push(Block::Html(format!("<p>{}</p>", inline(&text))));
        }
    }
    // One change gets one comparison view: consecutive screenshots merge.
    let mut merged: Vec<Block> = Vec::new();
    for block in blocks {
        let block = match block {
            Block::Label(label) => Block::Html(format!("<h3>{}</h3>", inline(&label))),
            block => block,
        };
        match (merged.last_mut(), block) {
            (Some(Block::Shots(previous)), Block::Shots(more)) => previous.extend(more),
            (_, block) => merged.push(block),
        }
    }
    merged
}

fn render(record: &Path) -> Result<String, String> {
    let base = record.parent().unwrap_or(Path::new("."));
    let text = std::fs::read_to_string(record).map_err(|error| error.to_string())?;
    let lines: Vec<&str> = text.lines().collect();
    let mut title = stem(&record.to_string_lossy());
    let key = title
        .split("-review")
        .next()
        .unwrap_or_default()
        .to_string();
    let (mut header, mut nav, mut body, mut callouts) =
        (String::new(), Vec::new(), String::new(), Vec::new());
    let mut section = String::new();
    let mut in_section = false;
    let flush =
        |body: &mut String, header: &mut String, in_section: bool, callouts: &mut Vec<String>| {
            if !callouts.is_empty() {
                let target = if in_section { body } else { header };
                target.push_str(&format!(
                    r#"<div class="callouts">{}</div>"#,
                    callouts.concat()
                ));
                callouts.clear();
            }
        };
    for block in parse(&lines) {
        match block {
            Block::Heading(1, text) => {
                title = text;
                header.push_str(&format!("<h1>{}</h1>", inline(&title)));
            }
            Block::Heading(2, text) => {
                flush(&mut body, &mut header, in_section, &mut callouts);
                if in_section {
                    body.push_str("</section>");
                }
                let number = nav.len() + 1;
                let name = text
                    .trim_start_matches(|c: char| c.is_ascii_digit())
                    .trim_start_matches(['.', ')'])
                    .trim_start()
                    .to_string();
                let name = if name.len() == text.len() || name.is_empty() {
                    text.clone()
                } else {
                    name
                };
                nav.push(format!(
                    "<a href=\"#change-{number}\">{number}. {}</a>",
                    inline(&name)
                ));
                body.push_str(&format!(
                    r#"<section class="change" id="change-{number}"><h2><span class="n">{number}</span>{}</h2>"#,
                    inline(&name)
                ));
                section = name;
                in_section = true;
            }
            Block::Callout(kind, text) => callouts.push(render_callout(&kind, &text)),
            other => {
                flush(&mut body, &mut header, in_section, &mut callouts);
                let html = match other {
                    Block::Heading(_, text) => format!("<h3>{}</h3>", inline(&text)),
                    Block::Html(html) => html,
                    Block::Shots(shots) => render_shots(
                        &shots,
                        base,
                        if section.is_empty() { &title } else { &section },
                    )?,
                    Block::Facts(rows) if in_section => format!(
                        r#"<dl class="facts">{}</dl>"#,
                        rows.iter()
                            .map(|(name, value)| format!(
                                "<dt>{}</dt><dd>{}</dd>",
                                inline(name),
                                inline(value)
                            ))
                            .collect::<String>()
                    ),
                    Block::Facts(rows) => format!(
                        r#"<div class="chips">{}</div>"#,
                        rows.iter()
                            .map(|(name, value)| format!(
                                r#"<span class="chip"><b>{}</b>{}</span>"#,
                                inline(name),
                                inline(value)
                            ))
                            .collect::<String>()
                    ),
                    Block::Callout(..) | Block::Label(_) => String::new(),
                };
                if in_section {
                    body.push_str(&html);
                } else {
                    header.push_str(&html);
                }
            }
        }
    }
    flush(&mut body, &mut header, in_section, &mut callouts);
    if in_section {
        body.push_str("</section>");
    }
    Ok(format!(
        "<!doctype html><html lang='en'><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>{}</title><style>
{CSS}</style></head><body><div class=\"layout\"><nav><div class=\"key\">{}</div>{}</nav><main><header>{header}</header>{body}</main></div><div class=\"lightbox\" role=\"dialog\" aria-label=\"Screenshot viewer\"><button class=\"close\" aria-label=\"Close\">×</button><button class=\"prev\" aria-label=\"Previous\">‹</button><img alt=\"\"><div class=\"cap\"></div><button class=\"next\" aria-label=\"Next\">›</button></div><script>
{JS}</script></body></html>",
        escape(&title),
        escape(&key),
        nav.concat()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_record_with_its_screenshots_embedded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("K-1-1-menu-desktop.png"), b"png").unwrap();
        std::fs::write(dir.path().join("K-1-1-menu-mobile.png"), b"png").unwrap();
        let record = dir.path().join("K-1.md");
        std::fs::write(
            &record,
            "# K-1 — Visual changes: menú nuevo\n\n| Test environment | |\n| --- | --- |\n| Date | 2026-10-16 |\n\n## 1. Menú\n\n| | |\n| --- | --- |\n| Where | Ajustes |\n| Behaviour | Abre el `menú` |\n\n**Desktop — abierto**\n\n![](K-1-1-menu-desktop.png)\n\n**Mobile — abierto**\n\n![](K-1-1-menu-mobile.png)\n\n**Observations:** Bien.\n\n**Problems:** None\n",
        )
        .unwrap();
        let out = render_beside(&record).unwrap();
        let page = std::fs::read_to_string(out).unwrap();
        assert!(page.contains("<h1>K-1 — Visual changes: menú nuevo</h1>"));
        // As in the script: a table with a header renders as a table.
        assert!(page.contains("<th>Test environment</th>"));
        assert!(page.contains("<td>2026-10-16</td>"));
        assert!(page.contains(r#"<dt>Behaviour</dt><dd>Abre el <code>menú</code></dd>"#));
        assert!(page
            .contains(r#"<div class="shots"><div class="panel"><div class="title">Desktop</div>"#));
        assert!(page.contains(r#"<div class="title">Mobile</div>"#));
        assert!(page.contains("data:image/png;base64,"));
        // The file name lets a mention of it in the text open the screenshot.
        assert!(page.contains(r#"data-file="K-1-1-menu-desktop.png""#));
        // And each screenshot names its file below it.
        assert!(page.contains(r#"<figcaption><code>K-1-1-menu-mobile.png</code></figcaption>"#));
        assert!(page.contains(r#"<div class="callout prob none">"#));
        assert!(page.contains(r##"<a href="#change-1">1. Menú</a>"##));
    }

    #[test]
    fn a_missing_screenshot_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("K-2.md");
        std::fs::write(&record, "## 1. A\n\n![](K-2-1-a-desktop.png)\n").unwrap();
        assert!(render_beside(&record)
            .unwrap_err()
            .contains("K-2-1-a-desktop.png"));
    }

    #[test]
    fn a_record_under_another_name_is_found() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(find(dir.path(), "K-3"), None);
        let write = |name: &str, secs: u64| {
            let path = dir.path().join(name);
            std::fs::write(&path, "# x").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
                .unwrap();
            path
        };
        write("K-3-1-menu-desktop.png", 50);
        write("K-30.md", 300);
        let old = write("K-3_old.md", 100);
        let matrix = write("K-3-wp1-matrix.md", 200);
        assert_eq!(
            find(dir.path(), "K-3"),
            Some(RecordFile {
                source: matrix.clone(),
                extras: vec![old.clone()],
            })
        );
        // A page the agent wrote comes before them, and `<KEY>.md` first.
        let html = write("K-3.html", 10);
        assert_eq!(find(dir.path(), "K-3").unwrap().source, html);
        assert_eq!(page_for(&html).unwrap(), html);
        let md = write("K-3.md", 10);
        let found = find(dir.path(), "K-3").unwrap();
        assert_eq!(found.source, md);
        assert_eq!(found.extras, [matrix, old]);
    }

    #[test]
    fn a_stub_record_keeps_the_evidence_and_verdicts_nothing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("K-1-1-menu-desktop.png"), b"png").unwrap();
        let items = [
            ("Check the menu".to_string(), true),
            ("Check the toast".to_string(), false),
        ];
        let path = write_stub(
            dir.path(),
            "K-1",
            "feat/k-1",
            &date_of(1_793_106_614_092),
            "its 60 minutes ran out",
            &items,
        )
        .unwrap();
        let text = std::fs::read_to_string(path).unwrap();
        assert!(missing_parts(&text).is_empty(), "{text}");
        assert!(text.contains("| Date | 2026-10-27 |"), "{text}");
        assert!(
            text.contains("| 2. Check the toast | not verifiable: out of time |"),
            "{text}"
        );
        assert!(text.contains("![](K-1-1-menu-desktop.png)"), "{text}");
        assert!(!text.contains("pass"), "{text}");
    }

    #[test]
    fn a_record_without_its_header_or_verdicts_is_told() {
        let full = "| Test environment | |\n| --- | --- |\n| **Date** | 2026-10-22 |\n\n| Item | Verdict |\n| --- | --- |\n| Menu | ✅ Pass |\n";
        assert!(missing_parts(full).is_empty());
        let matrix =
            "| Fecha | 2026-10-22 |\n\n| AC1 | `cargo test a` | not verified: no database |\n";
        assert!(missing_parts(matrix).is_empty());
        let bare = missing_parts("# K-1\n\nAll good.\n");
        assert_eq!(bare.len(), 2);
        assert!(bare[0].contains("cabecera"));
        assert!(bare[1].contains("veredictos"));
    }

    #[test]
    fn labels_and_inline_marks_follow_the_script() {
        assert_eq!(tab_label("Desktop: menu"), "menu");
        assert_eq!(tab_label("Mobile — toast"), "toast");
        assert!(redundant_caption("Mobile 375×812"));
        assert!(!redundant_caption("Mobile — toast"));
        assert_eq!(
            inline("a `b` **c** *d* <e>"),
            "a <code>b</code> <strong>c</strong> <em>d</em> &lt;e&gt;"
        );
        assert!(shot("", "K-1-2-dialog-mobile.png").mobile);
    }
}
