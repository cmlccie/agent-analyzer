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
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use widgets::ListPane;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Models,
    Agents,
    Tools,
}

impl Pane {
    fn next(self) -> Self {
        match self {
            Pane::Models => Pane::Agents,
            Pane::Agents => Pane::Tools,
            Pane::Tools => Pane::Models,
        }
    }

    fn prev(self) -> Self {
        match self {
            Pane::Models => Pane::Tools,
            Pane::Agents => Pane::Models,
            Pane::Tools => Pane::Agents,
        }
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

    let mut models_pane = ListPane::new("Models", TargetKind::Model);
    let mut agents_pane = ListPane::new("Agents", TargetKind::Agent);
    let mut tools_pane = ListPane::new("Tools (MCP)", TargetKind::Tool);
    let mut focused = Pane::Models;

    loop {
        let ts = targets.lock().unwrap().clone();

        terminal.draw(|f| {
            draw(
                f,
                &ts,
                &mut models_pane,
                &mut agents_pane,
                &mut tools_pane,
                focused,
            );
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match (key.code, key.modifiers) {
                (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    break;
                }
                (KeyCode::Tab, _) => focused = focused.next(),
                (KeyCode::BackTab, _) => focused = focused.prev(),
                (KeyCode::Down, _) => match focused {
                    Pane::Models => models_pane.select_next(&ts),
                    Pane::Agents => agents_pane.select_next(&ts),
                    Pane::Tools => tools_pane.select_next(&ts),
                },
                (KeyCode::Up, _) => match focused {
                    Pane::Models => models_pane.select_prev(&ts),
                    Pane::Agents => agents_pane.select_prev(&ts),
                    Pane::Tools => tools_pane.select_prev(&ts),
                },
                _ => {}
            }
        }
    }

    Ok(())
}

fn draw(
    f: &mut Frame,
    targets: &[Target],
    models_pane: &mut ListPane,
    agents_pane: &mut ListPane,
    tools_pane: &mut ListPane,
    focused: Pane,
) {
    let size = f.area();

    // Outer split: header | body | websites bar
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // 6 banner lines + bottom border
            Constraint::Min(0),    // main body
            Constraint::Length(3), // websites bar
        ])
        .split(size);

    // Header: logo left, key bindings right, shared bottom border.
    // Render the border on the full area first, then work inside the inner rect.
    let header_block = Block::default().borders(Borders::BOTTOM);
    let header_inner = header_block.inner(outer[0]);
    f.render_widget(header_block, outer[0]);

    let header_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(22)])
        .split(header_inner);

    let logo: Vec<Line> = ascii::BANNER_LINES.iter().map(|&l| Line::from(l)).collect();
    f.render_widget(Paragraph::new(logo), header_cols[0]);

    // Offset keys by 1 blank line so they sit in the middle of the banner height.
    let key_style = Style::default().fg(Color::DarkGray);
    let keys: Vec<Line> = std::iter::once(Line::from(""))
        .chain(
            ascii::KEYS_LINES
                .iter()
                .map(|&l| Line::from(ratatui::text::Span::styled(l, key_style))),
        )
        .collect();
    f.render_widget(Paragraph::new(keys), header_cols[1]);

    // Body split: left panes | detail pane
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(0)])
        .split(outer[1]);

    // Left column: three stacked list panes
    let left_panes = widgets::three_column_left(body[0]);
    models_pane.render(f, left_panes[0], targets, focused == Pane::Models);
    agents_pane.render(f, left_panes[1], targets, focused == Pane::Agents);
    tools_pane.render(f, left_panes[2], targets, focused == Pane::Tools);

    // Detail pane
    let selected = match focused {
        Pane::Models => models_pane.selected_target(targets),
        Pane::Agents => agents_pane.selected_target(targets),
        Pane::Tools => tools_pane.selected_target(targets),
    };
    widgets::render_detail(f, body[1], selected);

    // Websites bar
    widgets::render_websites_bar(f, outer[2], targets);
}
