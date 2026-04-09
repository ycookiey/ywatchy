use owo_colors::OwoColorize;
use unicode_width::UnicodeWidthStr;

pub enum Status {
    Skip,
    Synced(String),
    New(String),
    Linked,
    Removed,
    Orphan,
    Error(String),
}

struct Row {
    icon: &'static str,
    name: String,
    detail: String,
}

pub fn print_section(label: &str, count: usize, items: &[(String, Status)]) {
    let rows: Vec<Row> = items
        .iter()
        .map(|(name, status)| to_row(name, status))
        .collect();

    let name_width = rows.iter().map(|r| display_width(&r.name)).max().unwrap_or(0);

    let row_widths: Vec<usize> = rows
        .iter()
        .map(|r| {
            // "  {icon} {name}   {detail}"
            let base = 2 + display_width(r.icon) + 1 + name_width;
            if r.detail.is_empty() {
                base
            } else {
                base + 3 + display_width(&r.detail)
            }
        })
        .collect();

    let max_row_width = row_widths.iter().copied().max().unwrap_or(0);

    // header: " {label} ── {count} items "
    let header_text = format!(" {} \u{2500}\u{2500} {} items ", label, count);
    let header_width = display_width(&header_text);

    let inner_width = max_row_width.max(header_width);

    // top border: ┌{header_text}{─ × padding}┐
    let header_pad = inner_width - header_width;
    print!("\u{250c}{}", header_text);
    println!("{}\u{2510}", "\u{2500}".repeat(header_pad));

    // rows
    for (row, row_width) in rows.iter().zip(row_widths.iter()) {
        let padded_name = pad_right(&row.name, name_width);
        let content = if row.detail.is_empty() {
            format!("  {} {}", row.icon, padded_name)
        } else {
            format!("  {} {}   {}", row.icon, padded_name, row.detail)
        };
        let pad = inner_width - row_width;
        let line = format!("\u{2502}{}{}\u{2502}", content, " ".repeat(pad));
        println!("{}", colorize_line(&line, row.icon));
    }

    // bottom border: └{─ × inner_width}┘
    println!("\u{2514}{}\u{2518}", "\u{2500}".repeat(inner_width));
}

pub fn print_event(category: &str, name: &str, detail: &str) {
    let msg = if detail.is_empty() {
        format!("[{}] {}", category, name)
    } else {
        format!("[{}] {} ({})", category, name, detail)
    };
    println!("{}", msg.green());
}

pub fn print_event_error(category: &str, name: &str, detail: &str) {
    let msg = format!("[{}] {} ({})", category, name, detail);
    println!("{}", msg.red());
}

pub fn print_watching() {
    println!("\n{}", "watching for changes...".dimmed());
}

fn to_row(name: &str, status: &Status) -> Row {
    match status {
        Status::Skip => Row {
            icon: "\u{2713}",
            name: name.to_string(),
            detail: String::new(),
        },
        Status::Synced(dir) => Row {
            icon: "\u{2605}",
            name: name.to_string(),
            detail: dir.clone(),
        },
        Status::New(dir) => Row {
            icon: "+",
            name: name.to_string(),
            detail: dir.clone(),
        },
        Status::Linked => Row {
            icon: "\u{2605}",
            name: name.to_string(),
            detail: "linked".to_string(),
        },
        Status::Removed => Row {
            icon: "-",
            name: name.to_string(),
            detail: "stale".to_string(),
        },
        Status::Orphan => Row {
            icon: "?",
            name: name.to_string(),
            detail: "store only".to_string(),
        },
        Status::Error(msg) => Row {
            icon: "\u{2717}",
            name: name.to_string(),
            detail: msg.clone(),
        },
    }
}

fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

fn pad_right(s: &str, target_width: usize) -> String {
    let current = display_width(s);
    if current >= target_width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(target_width - current))
    }
}

fn colorize_line(line: &str, icon: &str) -> String {
    match icon {
        "\u{2713}" => format!("{}", line.dimmed()),
        "\u{2605}" => format!("{}", line.green()),
        "+" => format!("{}", line.cyan()),
        "-" => format!("{}", line.yellow()),
        "?" => format!("{}", line.yellow()),
        "\u{2717}" => format!("{}", line.red()),
        _ => line.to_string(),
    }
}
