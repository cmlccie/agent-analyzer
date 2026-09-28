use crate::core::target::{Source, Status, Target, TargetKind};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

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
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::styled(t.name.clone(), Style::default().fg(TEXT)),
                    Span::styled(tag, Style::default().fg(SUBTLE)),
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
        Some(t) => {
            let (icon, color) = status_icon(&t.status);
            let mut lines = vec![
                field(
                    "Name",
                    t.name.clone(),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
                field("Kind", t.kind.to_string(), Style::default().fg(TEXT)),
                field("URL", t.url.to_string(), Style::default().fg(TEXT)),
                field("Source", t.source.to_string(), Style::default().fg(TEXT)),
                field(
                    "Status",
                    format!("{icon} {}", status_label(&t.status)),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ];

            if let Some(ts) = t.status.checked_at() {
                lines.push(field(
                    "Checked",
                    ts.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
                    Style::default().fg(TEXT),
                ));
            }

            match &t.status {
                Status::Ok { details, .. } => {
                    lines.push(Line::from(""));
                    lines.push(Line::styled("Details", Style::default().fg(LABEL)));
                    let pretty = serde_json::to_string_pretty(details).unwrap_or_default();
                    lines.extend(
                        pretty
                            .lines()
                            .map(|l| Line::styled(format!("  {l}"), Style::default().fg(TEXT))),
                    );
                }
                Status::Failed { error, .. } => {
                    lines.push(field("Error", error.clone(), Style::default().fg(RED)));
                }
                Status::Unknown => {}
            }

            if !t.metadata.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::styled("Labels", Style::default().fg(LABEL)));
                lines.extend(t.metadata.iter().map(|(k, v)| {
                    Line::from(vec![
                        Span::styled(format!("  {k}="), Style::default().fg(LABEL)),
                        Span::styled(v.clone(), Style::default().fg(TEXT)),
                    ])
                }));
            }

            lines
        }
    };

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn field(label: &str, value: String, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<9}"), Style::default().fg(LABEL)),
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

fn status_label(status: &Status) -> &'static str {
    match status {
        Status::Ok { .. } => "reachable",
        Status::Failed { .. } => "failed",
        Status::Unknown => "not yet probed",
    }
}
