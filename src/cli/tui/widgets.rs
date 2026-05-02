use crate::core::target::{Status, Target, TargetKind};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

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

    pub fn render(&mut self, f: &mut Frame, area: Rect, targets: &[Target], focused: bool) {
        let border_style = if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default()
        };

        let items: Vec<ListItem> = targets
            .iter()
            .filter(|t| t.kind == self.kind)
            .map(|t| {
                let (icon, color) = status_icon(&t.status);
                let source_tag = match &t.source {
                    crate::core::target::Source::Discovered { .. } => "(k8s)",
                    crate::core::target::Source::Manual => "(cfg)",
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{icon} "), Style::default().fg(color)),
                    Span::raw(format!("{} {source_tag}", t.name)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(self.title)
                    .borders(Borders::ALL)
                    .border_style(border_style),
            )
            .highlight_style(
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::White),
            );

        f.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn selected_target<'a>(&self, targets: &'a [Target]) -> Option<&'a Target> {
        let i = self.state.selected()?;
        targets.iter().filter(|t| t.kind == self.kind).nth(i)
    }

    pub fn select_next(&mut self, targets: &[Target]) {
        let count = targets.iter().filter(|t| t.kind == self.kind).count();
        if count == 0 {
            return;
        }
        let i = self.state.selected().map(|i| (i + 1) % count).unwrap_or(0);
        self.state.select(Some(i));
    }

    pub fn select_prev(&mut self, targets: &[Target]) {
        let count = targets.iter().filter(|t| t.kind == self.kind).count();
        if count == 0 {
            return;
        }
        let i = self
            .state
            .selected()
            .map(|i| i.checked_sub(1).unwrap_or(count - 1))
            .unwrap_or(0);
        self.state.select(Some(i));
    }
}

pub fn render_detail(f: &mut Frame, area: Rect, target: Option<&Target>) {
    let block = Block::default().title("Detail").borders(Borders::ALL);

    let text: Vec<Line> = match target {
        None => vec![Line::from("Select a target to view details.")],
        Some(t) => {
            let (icon, color) = status_icon(&t.status);
            let mut lines = vec![
                Line::from(vec![
                    Span::raw("Name:    "),
                    Span::styled(
                        t.name.clone(),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(format!("Kind:    {}", t.kind)),
                Line::from(format!("URL:     {}", t.url)),
                Line::from(format!("Source:  {}", t.source)),
                Line::from(vec![
                    Span::raw("Status:  "),
                    Span::styled(icon.to_string(), Style::default().fg(color)),
                ]),
            ];

            if let Some(ts) = t.status.checked_at() {
                lines.push(Line::from(format!(
                    "Checked: {}",
                    ts.format("%Y-%m-%dT%H:%M:%SZ")
                )));
            }

            match &t.status {
                Status::Ok { details, .. } => {
                    lines.push(Line::from(""));
                    lines.push(Line::from("Details:"));
                    let pretty = serde_json::to_string_pretty(details).unwrap_or_default();
                    for l in pretty.lines() {
                        lines.push(Line::from(format!("  {l}")));
                    }
                }
                Status::Failed { error, .. } => {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::raw("Error: "),
                        Span::styled(error.clone(), Style::default().fg(Color::Red)),
                    ]));
                }
                Status::Unknown => {}
            }

            if !t.metadata.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from("Labels:"));
                for (k, v) in &t.metadata {
                    lines.push(Line::from(format!("  {k}={v}")));
                }
            }

            lines
        }
    };

    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

pub fn render_websites_bar(f: &mut Frame, area: Rect, targets: &[Target]) {
    let websites: Vec<&Target> = targets
        .iter()
        .filter(|t| t.kind == TargetKind::Website)
        .collect();

    let spans: Vec<Span> = websites
        .iter()
        .flat_map(|t| {
            let (icon, color) = status_icon(&t.status);
            vec![
                Span::styled(format!("{icon} "), Style::default().fg(color)),
                Span::raw(format!("{}  ", t.name)),
            ]
        })
        .collect();

    let block = Block::default().title("Websites").borders(Borders::ALL);
    let para = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(para, area);
}

pub fn three_column_left(area: Rect) -> [Rect; 3] {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);
    [chunks[0], chunks[1], chunks[2]]
}

fn status_icon(status: &Status) -> (&'static str, Color) {
    match status {
        Status::Ok { .. } => ("✓", Color::Green),
        Status::Failed { .. } => ("✗", Color::Red),
        Status::Unknown => ("?", Color::DarkGray),
    }
}
