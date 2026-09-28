use crate::core::target::{Source, Status, Target, TargetKind};
use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use serde_json::Value;

// GitHub dark palette. Text colors are picked for legibility on a pure black terminal
// background: `TEXT` for primary content, `LABEL` for secondary text such as field names and
// inactive pane titles, `SUBTLE` only for decoration that should recede.
pub const TEXT: Color = Color::Rgb(230, 237, 243); // #e6edf3
pub const LABEL: Color = Color::Rgb(173, 186, 199); // #adbac7
pub const SUBTLE: Color = Color::Rgb(125, 133, 144); // #7d8590
pub const BORDER: Color = Color::Rgb(72, 79, 88); // #484f58
pub const ACCENT: Color = Color::Rgb(88, 166, 255); // #58a6ff
const GREEN: Color = Color::Rgb(63, 185, 80); // #3fb950
const RED: Color = Color::Rgb(248, 81, 73); // #f85149
const DIM_GREEN: Color = Color::Rgb(86, 211, 100); // #56d364
const DIM_RED: Color = Color::Rgb(255, 123, 114); // #ff7b72

/// Terminal width at which the four inventory panes sit side by side instead of in a 2×2 grid.
const WIDE_LAYOUT_MIN_WIDTH: u16 = 120;

pub struct ListPane {
    pub title: &'static str,
    pub kind: TargetKind,
    pub state: ListState,
}

impl ListPane {
    pub fn new(title: &'static str, kind: TargetKind) -> Self {
        ListPane {
            title,
            kind,
            state: ListState::default(),
        }
    }

    fn items<'a>(&self, targets: &'a [Target]) -> impl Iterator<Item = &'a Target> {
        let kind = self.kind;
        targets.iter().filter(move |t| t.kind == kind)
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect, targets: &[Target], focused: bool) {
        let items: Vec<&Target> = self.items(targets).collect();

        // Keep a valid selection so the detail pane always has something to show.
        match self.state.selected() {
            _ if items.is_empty() => self.state.select(None),
            Some(i) if i < items.len() => {}
            _ => self.state.select(Some(0)),
        }

        let block = Block::default()
            .title(pane_title(self.title, &items, focused))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if focused { ACCENT } else { BORDER }));

        let list_items: Vec<ListItem> = items
            .iter()
            .map(|t| {
                let (icon, color) = status_icon(&t.status);
                let tag = match t.source {
                    Source::Discovered { .. } => " k8s",
                    Source::Manual => " cfg",
                };
                let outcome = match &t.status {
                    Status::Failed { kind, .. } => {
                        Span::styled(format!("  {}", kind.label()), Style::default().fg(DIM_RED))
                    }
                    Status::Ok { details, .. } => Span::styled(
                        inventory_summary(details)
                            .map(|s| format!("  {s}"))
                            .unwrap_or_default(),
                        Style::default().fg(DIM_GREEN),
                    ),
                    Status::Unknown => Span::raw(""),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled(t.name.clone(), Style::default().fg(TEXT)),
                    Span::styled(tag, Style::default().fg(SUBTLE)),
                    outcome,
                ]))
            })
            .collect();

        let highlight = if focused {
            Style::default()
                .fg(ACCENT)
                .bg(Color::Rgb(22, 27, 34)) // #161b22
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };

        let list = List::new(list_items)
            .block(block)
            .highlight_style(highlight)
            .highlight_symbol(if focused { "▌" } else { " " });

        f.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn selected_target<'a>(&self, targets: &'a [Target]) -> Option<&'a Target> {
        self.items(targets).nth(self.state.selected()?)
    }

    pub fn select_next(&mut self, targets: &[Target]) {
        let count = self.items(targets).count();
        if count > 0 {
            let i = self.state.selected().map(|i| (i + 1) % count).unwrap_or(0);
            self.state.select(Some(i));
        }
    }

    pub fn select_prev(&mut self, targets: &[Target]) {
        let count = self.items(targets).count();
        if count > 0 {
            let i = self
                .state
                .selected()
                .map(|i| i.checked_sub(1).unwrap_or(count - 1))
                .unwrap_or(0);
            self.state.select(Some(i));
        }
    }
}

/// Pane title with a per-status tally, e.g. ` Agents  ✓ 2  ✗ 4 `.
fn pane_title(title: &str, items: &[&Target], focused: bool) -> Line<'static> {
    let title_style = if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
    };

    let count = |f: fn(&Status) -> bool| items.iter().filter(|t| f(&t.status)).count();
    let tallies = [
        (count(|s| matches!(s, Status::Ok { .. })), GREEN, "✓"),
        (count(|s| matches!(s, Status::Failed { .. })), RED, "✗"),
        (count(|s| matches!(s, Status::Unknown)), SUBTLE, "?"),
    ];

    std::iter::once(Span::styled(format!(" {title} "), title_style))
        .chain(
            tallies
                .into_iter()
                .filter(|(n, ..)| *n > 0)
                .map(|(n, color, icon)| {
                    Span::styled(format!(" {icon} {n} "), Style::default().fg(color))
                }),
        )
        .collect()
}

pub fn render_detail(f: &mut Frame, area: Rect, target: Option<&Target>) {
    let block = Block::default()
        .title(Span::styled(
            " Detail ",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER));

    let text: Vec<Line> = match target {
        None => vec![Line::styled(
            "Select a target to view details.",
            Style::default().fg(LABEL),
        )],
        Some(t) => detail_lines(t),
    };

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn detail_lines(t: &Target) -> Vec<Line<'static>> {
    let text = Style::default().fg(TEXT);
    let (icon, color) = status_icon(&t.status);

    let state = match &t.status {
        Status::Ok { .. } => "reachable".to_string(),
        Status::Failed { kind, .. } => kind.label(),
        Status::Unknown => "not yet probed".to_string(),
    };
    let duration = t
        .since
        .map(|since| format!("  for {}", human_duration(Utc::now() - since)))
        .unwrap_or_default();

    let mut lines = vec![
        field("Name", t.name.clone(), text.add_modifier(Modifier::BOLD)),
        field("Kind", t.kind.to_string(), text),
        field("URL", t.url.to_string(), text),
        field("Source", t.source.to_string(), text),
        Line::from(vec![
            Span::styled(
                format!("{:<LABEL_WIDTH$}", "Status"),
                Style::default().fg(LABEL),
            ),
            Span::styled(
                format!("{icon} {state}"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(duration, Style::default().fg(LABEL)),
        ]),
    ];

    if let Status::Failed { kind, error, .. } = &t.status {
        lines.push(field(
            "",
            kind.explanation().to_string(),
            Style::default().fg(LABEL),
        ));
        lines.push(field("Error", error.clone(), Style::default().fg(RED)));
    }

    if let Some(ts) = t.status.checked_at() {
        lines.push(field(
            "Checked",
            ts.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            text,
        ));
    }

    if !t.history.is_empty() {
        lines.push(Line::from(
            std::iter::once(Span::styled(
                format!("{:<LABEL_WIDTH$}", "History"),
                Style::default().fg(LABEL),
            ))
            .chain(
                t.history.iter().map(|&ok| {
                    Span::styled("▮", Style::default().fg(if ok { GREEN } else { RED }))
                }),
            )
            .collect::<Vec<_>>(),
        ));
    }

    if let Status::Ok { details, .. } = &t.status {
        lines.extend(details_lines(details));
    }

    if !t.metadata.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::styled("Labels", Style::default().fg(LABEL)));
        lines.extend(t.metadata.iter().map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("  {k}="), Style::default().fg(LABEL)),
                Span::styled(v.clone(), text),
            ])
        }));
    }

    lines
}

/// Probe details as fields and lists: scalars become `Key value` rows, arrays of strings become
/// a titled, counted list, and anything else is shown as compact JSON.
fn details_lines(details: &Value) -> Vec<Line<'static>> {
    let text = Style::default().fg(TEXT);
    let Some(map) = details.as_object() else {
        return vec![];
    };

    let (lists, scalars): (Vec<_>, Vec<_>) = map
        .iter()
        .filter(|(_, v)| !v.is_null())
        .partition(|(_, v)| v.is_array());

    let scalar_lines = scalars.into_iter().map(|(k, v)| {
        let value = v
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| v.to_string());
        let style = if k.ends_with("error") {
            Style::default().fg(RED)
        } else {
            text
        };
        field(&title_case(k), value, style)
    });

    let list_lines = lists.into_iter().flat_map(|(k, v)| {
        let items = v.as_array().cloned().unwrap_or_default();
        std::iter::once(Line::from(""))
            .chain(std::iter::once(Line::styled(
                format!("{} ({})", title_case(k), items.len()),
                Style::default().fg(LABEL),
            )))
            .chain(items.into_iter().map(move |i| {
                let item = i
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| i.to_string());
                Line::styled(format!("  • {item}"), text)
            }))
    });

    std::iter::once(Line::from(""))
        .chain(scalar_lines)
        .chain(list_lines)
        .collect()
}

/// A short inventory summary for a list row, e.g. `5 tools`, from the first list in the details.
fn inventory_summary(details: &Value) -> Option<String> {
    details
        .as_object()?
        .iter()
        .find_map(|(k, v)| v.as_array().map(|a| format!("{} {k}", a.len())))
}

fn title_case(key: &str) -> String {
    let words = key.replace('_', " ");
    let mut chars = words.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Compact duration: `42s`, `3m 12s`, `2h 5m`, `3d 4h`.
pub fn human_duration(d: chrono::Duration) -> String {
    let s = d.num_seconds().max(0);
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {}s", s / 60, s % 60),
        3600..86400 => format!("{}h {}m", s / 3600, s % 3600 / 60),
        _ => format!("{}d {}h", s / 86400, s % 86400 / 3600),
    }
}

const LABEL_WIDTH: usize = 13;

fn field(label: &str, value: String, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<LABEL_WIDTH$}"), Style::default().fg(LABEL)),
        Span::styled(value, value_style),
    ])
}

/// Inventory pane areas in pane order (Models, Agents, Tools, Websites): a single row of four
/// equal columns on wide terminals, a 2×2 grid otherwise so names are not truncated.
pub fn inventory_layout(area: Rect) -> [Rect; 4] {
    if area.width >= WIDE_LAYOUT_MIN_WIDTH {
        Layout::horizontal([Constraint::Fill(1); 4]).areas(area)
    } else {
        let [top, bottom] = Layout::vertical([Constraint::Fill(1); 2]).areas(area);
        let [a, b] = Layout::horizontal([Constraint::Fill(1); 2]).areas(top);
        let [c, d] = Layout::horizontal([Constraint::Fill(1); 2]).areas(bottom);
        [a, b, c, d]
    }
}

fn status_icon(status: &Status) -> (&'static str, Color) {
    match status {
        Status::Ok { .. } => ("✓", GREEN),
        Status::Failed { .. } => ("✗", RED),
        Status::Unknown => ("?", SUBTLE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn human_duration_scales_units() {
        let secs = chrono::Duration::seconds;
        assert_eq!(human_duration(secs(42)), "42s");
        assert_eq!(human_duration(secs(192)), "3m 12s");
        assert_eq!(human_duration(secs(7500)), "2h 5m");
        assert_eq!(human_duration(secs(-5)), "0s");
    }

    #[test]
    fn inventory_summary_counts_first_list() {
        let details = json!({"protocol": "2025-06-18", "tools": ["a", "b", "c"]});
        assert_eq!(inventory_summary(&details).as_deref(), Some("3 tools"));
        assert_eq!(inventory_summary(&json!({"status_code": 200})), None);
    }

    #[test]
    fn details_lines_render_lists_with_counts() {
        let details = json!({"server": "weather 1.0", "tools": ["forecast", "alerts"]});
        let rendered: Vec<String> = details_lines(&details)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert!(
            rendered
                .iter()
                .any(|l| l.starts_with("Server") && l.ends_with("weather 1.0"))
        );
        assert!(rendered.contains(&"Tools (2)".to_string()));
        assert!(rendered.contains(&"  • alerts".to_string()));
    }
}
