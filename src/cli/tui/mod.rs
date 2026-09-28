pub mod ascii;
pub mod widgets;

use crate::cli::args::TuiArgs;
use crate::core::client::ApiClient;
use crate::core::target::{Target, TargetKind};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use widgets::ListPane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Models,
    Agents,
    Tools,
    Websites,
}

impl Pane {
    const ALL: [Pane; 4] = [Pane::Models, Pane::Agents, Pane::Tools, Pane::Websites];

    fn index(self) -> usize {
        self as usize
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

pub fn run(args: TuiArgs) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, args);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop<B>(terminal: &mut Terminal<B>, args: TuiArgs) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: Send + Sync + 'static,
{
    let client = ApiClient::new(args.url)?;
    let targets: Arc<Mutex<Vec<Target>>> = Arc::new(Mutex::new(Vec::new()));

    // Background polling task.
    let poll_targets = Arc::clone(&targets);
    let poll_rt = tokio::runtime::Handle::try_current();
    if let Ok(handle) = poll_rt {
        let poll_client = ApiClient::new(client.base_url().clone())?;
        handle.spawn(async move {
            loop {
                match poll_client.state().await {
                    Ok(ts) => {
                        *poll_targets.lock().unwrap() = ts;
                    }
                    Err(e) => tracing::warn!("poll error: {e}"),
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }

    // Indexed by `Pane`.
    let mut panes = [
        ListPane::new("Models", TargetKind::Model),
        ListPane::new("Agents (A2A)", TargetKind::Agent),
        ListPane::new("Tools (MCP)", TargetKind::Tool),
        ListPane::new("External Hosts", TargetKind::Website),
    ];
    let mut focused = Pane::Models;
    let mut refreshed_at: Option<Instant> = None;

    loop {
        let ts = targets.lock().unwrap().clone();

        terminal.draw(|f| {
            let refreshing = refreshed_at.is_some_and(|t| t.elapsed() < REFRESH_FLASH);
            draw(f, &ts, &mut panes, focused, refreshing);
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    break;
                }
                (KeyCode::Char('r'), _) => {
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        let client = ApiClient::new(client.base_url().clone())?;
                        handle.spawn(async move {
                            if let Err(e) = client.refresh().await {
                                tracing::warn!("refresh request failed: {e}");
                            }
                        });
                        refreshed_at = Some(Instant::now());
                    }
                }
                (KeyCode::Tab, _) => focused = focused.next(),
                (KeyCode::BackTab, _) => focused = focused.prev(),
                (KeyCode::Right, _) => focused = focused.next(),
                (KeyCode::Left, _) => focused = focused.prev(),
                (KeyCode::Down | KeyCode::Char('j'), _) => panes[focused.index()].select_next(&ts),
                (KeyCode::Up | KeyCode::Char('k'), _) => panes[focused.index()].select_prev(&ts),
                _ => {}
            }
        }
    }

    Ok(())
}

/// How long the header acknowledges an `r` press.
const REFRESH_FLASH: Duration = Duration::from_secs(2);

fn draw(
    f: &mut Frame,
    targets: &[Target],
    panes: &mut [ListPane; 4],
    focused: Pane,
    refreshing: bool,
) {
    // Header | inventory | detail. The inventory and detail split the body 8:5 (consecutive
    // Fibonacci numbers, ≈ the golden ratio) so the inventory panes carry the most weight.
    let [header, inventory, detail] = Layout::vertical([
        Constraint::Length(7), // 6 banner lines + bottom border
        Constraint::Fill(8),
        Constraint::Fill(5),
    ])
    .areas(f.area());

    // Header: logo left, key bindings right, shared bottom border.
    // Render the border on the full area first, then work inside the inner rect.
    let header_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(widgets::BORDER));
    let header_inner = header_block.inner(header);
    f.render_widget(header_block, header);

    let [logo_area, keys_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(24)]).areas(header_inner);

    let logo: Vec<Line> = ascii::BANNER_LINES
        .iter()
        .map(|&l| Line::styled(l, Style::default().fg(widgets::TEXT)))
        .collect();
    f.render_widget(Paragraph::new(logo), logo_area);

    // Offset keys by 1 blank line so they sit in the middle of the banner height.
    let keys: Vec<Line> = std::iter::once(Line::from(""))
        .chain(
            ascii::KEYS_LINES
                .iter()
                .map(|&l| Line::styled(l, Style::default().fg(widgets::LABEL))),
        )
        .chain(
            refreshing
                .then(|| Line::styled("  ↻ refreshing…", Style::default().fg(widgets::ACCENT))),
        )
        .collect();
    f.render_widget(Paragraph::new(keys), keys_area);

    for ((pane, area), p) in panes
        .iter_mut()
        .zip(widgets::inventory_layout(inventory))
        .zip(Pane::ALL)
    {
        pane.render(f, area, targets, p == focused);
    }

    widgets::render_detail(f, detail, panes[focused.index()].selected_target(targets));
}
