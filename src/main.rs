mod api;

use anyhow::Result;
use api::{AnimeClient, Anime, Episode};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{io::{self, Stdout}, process::Command};

enum CurrentScreen {
    Search,
    SearchResults,
    EpisodeList,
}

struct App {
    client: AnimeClient,
    current_screen: CurrentScreen,
    search_query: String,
    
    // Search Results
    search_results: Vec<Anime>,
    search_list_state: ListState,
    
    // Selected Anime Details
    selected_anime_session: Option<String>,
    episode_list: Vec<Episode>,
    episode_list_state: ListState,

    // Status
    status_message: String,
    
    // Config
    use_terminal_player: bool,
}

impl App {
    fn new() -> Result<Self> {
        Ok(Self {
            client: AnimeClient::new()?,
            current_screen: CurrentScreen::Search,
            search_query: String::new(),
            search_results: Vec::new(),
            search_list_state: ListState::default(),
            selected_anime_session: None,
            episode_list: Vec::new(),
            episode_list_state: ListState::default(),
            status_message: String::from("Type to search, Enter to confirm, Esc to quit"),
            use_terminal_player: false,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse args
    let args: Vec<String> = std::env::args().collect();
    let use_terminal = args.iter().any(|a| a == "--terminal" || a == "-t");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;
    app.use_terminal_player = use_terminal;
    let res = run_app(&mut terminal, app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match app.current_screen {
                CurrentScreen::Search => match key.code {
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                    }
                    KeyCode::Enter => {
                        if !app.search_query.is_empty() {
                            app.status_message = "Searching...".to_string();
                            terminal.draw(|f| ui(f, &mut app))?; // Force redraw to show status
                            
                            match app.client.search(&app.search_query).await {
                                Ok(res) => {
                                    app.search_results = res.data;
                                    app.current_screen = CurrentScreen::SearchResults;
                                    app.search_list_state.select(Some(0));
                                    app.status_message = format!("Found {} results. Select with Up/Down, Enter to view episodes.", app.search_results.len());
                                }
                                Err(e) => {
                                    app.status_message = format!("Error: {}", e);
                                }
                            }
                        }
                    }
                    KeyCode::Esc => return Ok(()),
                    _ => {}
                },
                CurrentScreen::SearchResults => match key.code {
                    KeyCode::Up => {
                        let i = match app.search_list_state.selected() {
                            Some(i) => if i == 0 { app.search_results.len() - 1 } else { i - 1 },
                            None => 0,
                        };
                        app.search_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                         let i = match app.search_list_state.selected() {
                            Some(i) => if i >= app.search_results.len() - 1 { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.search_list_state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.search_list_state.selected() {
                            if let Some(vol) = app.search_results.get(i) {
                                let session = vol.session.clone();
                                app.selected_anime_session = Some(session.clone());
                                app.status_message = "Fetching episodes...".to_string();
                                terminal.draw(|f| ui(f, &mut app))?;

                                match app.client.get_episodes(&session, 1).await {
                                    Ok(res) => {
                                        app.episode_list = res.episodes; // Note: Pagination handled poorly here for now (only page 1)
                                        app.current_screen = CurrentScreen::EpisodeList;
                                        app.episode_list_state.select(Some(0));
                                        app.status_message = format!("Showing {} episodes. Select to play.", app.episode_list.len());
                                    }
                                    Err(e) => {
                                        app.status_message = format!("Error fetching episodes: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        app.current_screen = CurrentScreen::Search;
                        app.search_query.clear();
                        app.status_message = "Moved back to Search.".to_string();
                    }
                    _ => {}
                },
                CurrentScreen::EpisodeList => match key.code {
                    KeyCode::Up => {
                        let i = match app.episode_list_state.selected() {
                            Some(i) => if i == 0 { app.episode_list.len() - 1 } else { i - 1 },
                            None => 0,
                        };
                        app.episode_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                         let i = match app.episode_list_state.selected() {
                            Some(i) => if i >= app.episode_list.len() - 1 { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.episode_list_state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        let selected_ep_data = if let Some(i) = app.episode_list_state.selected() {
                            app.episode_list.get(i).map(|ep| (ep.session.clone(), ep.episode.clone()))
                        } else {
                            None
                        };

                        if let Some((ep_session, ep_num)) = selected_ep_data {
                            if let Some(series_session) = app.selected_anime_session.clone() {
                                app.status_message = format!("Fetching streams for Ep {}...", ep_num);
                                terminal.draw(|f| ui(f, &mut app))?;
                                
                                match app.client.get_stream(&series_session, &ep_session).await {
                                    Ok(streams) => {
                                        // Priority: 1080p -> 720p -> first
                                        let best_stream = streams.iter()
                                            .find(|s| s.name.contains("1080p"))
                                            .or_else(|| streams.iter().find(|s| s.name.contains("720p")))
                                            .or_else(|| streams.first());

                                        if let Some(link_item) = best_stream {
                                            app.status_message = format!("Extracting stream URL ({})...", link_item.name);
                                            terminal.draw(|f| ui(f, &mut app))?;
                                            
                                            match app.client.extract_stream_url(&link_item.link).await {
                                                Ok(direct_url) => {
                                                    // Suspend TUI
                                                    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
                                                    disable_raw_mode()?;
                                                    terminal.show_cursor()?;

                                                    // Run MPV with the direct stream URL
                                                    // Note: We still use referrer for the stream itself as some CDNs require it
                                                    // Run MPV with the direct stream URL
                                                    // Note: We still use referrer for the stream itself as some CDNs require it
                                                    let mut mpv_cmd = Command::new("mpv");
                                                    mpv_cmd.arg("--referrer=https://kwik.cx/");
                                                    
                                                    if app.use_terminal_player {
                                                        // User requested to force kitty backend for all terminals
                                                        // This allows terminals like WezTerm/Ghostty/etc that support the protocol 
                                                        // to work even if not detected as 'kitty' via env vars.
                                                        mpv_cmd.arg("--vo=kitty");
                                                        
                                                        // Explicitly quiet mpv output so it doesn't mess up the terminal too much
                                                        mpv_cmd.arg("--quiet");
                                                    }
                                                    
                                                    match mpv_cmd
                                                        .arg(&direct_url)
                                                        .status() {
                                                        Ok(status) => {
                                                            if status.success() {
                                                                app.status_message = "Finished playing.".to_string();
                                                            } else {
                                                                app.status_message = format!("mpv exited with status: {}", status);
                                                            }
                                                        },
                                                        Err(e) => {
                                                            app.status_message = format!("Failed to launch mpv: {}. Is it installed?", e);
                                                        }
                                                    }

                                                    // Resume TUI
                                                    enable_raw_mode()?;
                                                    execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
                                                    terminal.hide_cursor()?;
                                                    terminal.clear()?;
                                                }
                                                Err(e) => {
                                                    app.status_message = format!("Failed to extract stream: {}", e);
                                                }
                                            }
                                        } else {
                                            app.status_message = "No streams found.".to_string();
                                        }
                                    }
                                    Err(e) => {
                                         app.status_message = format!("Error fetching stream: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Esc => {
                        app.current_screen = CurrentScreen::SearchResults;
                        app.status_message = "Back to Results.".to_string();
                    }
                    _ => {}
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Search box
                Constraint::Min(1),    // Main content
                Constraint::Length(1), // Status bar
            ]
            .as_ref(),
        )
        .split(f.area());

    // Search Box
    let search_text = format!("Search: {}", app.search_query);
    let search_block = Paragraph::new(search_text)
        .block(Block::default().borders(Borders::ALL).title("Anime Search"));
    f.render_widget(search_block, chunks[0]);

    // Main Content
    match app.current_screen {
        CurrentScreen::Search => { // Start screen, maybe show instructions or empty
            let welcome = Paragraph::new("Welcome to AnimeCLI! Type query and hit Enter.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(welcome, chunks[1]);
        }
        CurrentScreen::SearchResults => {
            let items: Vec<ListItem> = app.search_results
                .iter()
                .map(|i| {
                    let text = format!("{} ({}) - Score: {:?}", i.title, i.anime_type.as_deref().unwrap_or("?"), i.score.unwrap_or(0.0));
                    ListItem::new(text)
                })
                .collect();
            
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Results"))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
                .highlight_symbol(">> ");
            
            f.render_stateful_widget(list, chunks[1], &mut app.search_list_state);
        }
        CurrentScreen::EpisodeList => {
             let items: Vec<ListItem> = app.episode_list
                .iter()
                .map(|ep| {
                    let _text = format!("Episode {} - {}", ep.episode, ep.snapshot); // usage fix
                    ListItem::new(format!("Episode {}", ep.episode))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("Episodes (Page 1)")))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Green))
                .highlight_symbol(">> ");
                
            f.render_stateful_widget(list, chunks[1], &mut app.episode_list_state);
        }
    }

    // Status Bar
    let status = Paragraph::new(app.status_message.as_str())
        .style(Style::default().fg(Color::Gray));
    f.render_widget(status, chunks[2]);
}
