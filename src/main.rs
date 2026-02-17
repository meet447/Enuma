mod api;

use anyhow::Result;
use api::{AnimeClient, Anime, Episode, StreamItem};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::{io::{self, Stdout}, process::Command};
use serde::{Deserialize, Serialize};
use chrono;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HistoryItem {
    pub anime: Anime,
    pub episode_session: String,
    pub last_episode: String,
    pub last_watched: String,
}

#[derive(PartialEq, Clone)]
enum CurrentScreen {
    Search,
    SearchResults,
    EpisodeList,
    Library,
    History,
    QualitySelection,
}

struct App {
    client: AnimeClient,
    current_screen: CurrentScreen,
    search_query: String,
    
    // Search Results
    search_results: Vec<Anime>,
    search_list_state: ListState,
    
    // Episode List
    selected_anime: Option<Anime>,
    episode_list: Vec<Episode>,
    episode_list_state: ListState,
    ep_page: u32,
    ep_total_pages: u32,

    // Library
    library: Vec<Anime>,
    library_list_state: ListState,

    // History
    history: Vec<HistoryItem>,
    history_list_state: ListState,

    // Quality Selection
    available_streams: Vec<StreamItem>,
    quality_list_state: ListState,
    temp_play_data: Option<(Anime, String, String)>,
    previous_screen: Option<CurrentScreen>,

    // Status
    status_message: String,

    // Search focus state
    is_searching: bool,

    // Loading & Animation state
    is_loading: bool,
    animation_tick: u32,
}

impl App {
    fn new() -> Result<Self> {
        let library = Self::load_data::<Vec<Anime>>("library.json").unwrap_or_default();
        let history = Self::load_data::<Vec<HistoryItem>>("history.json").unwrap_or_default();
        
        Ok(Self {
            client: AnimeClient::new()?,
            current_screen: CurrentScreen::Search,
            search_query: String::new(),
            search_results: Vec::new(),
            search_list_state: ListState::default(),
            selected_anime: None,
            episode_list: Vec::new(),
            episode_list_state: ListState::default(),
            ep_page: 1,
            ep_total_pages: 1,
            library,
            library_list_state: ListState::default(),
            history,
            history_list_state: ListState::default(),
            available_streams: Vec::new(),
            quality_list_state: ListState::default(),
            temp_play_data: None,
            previous_screen: None,
            status_message: String::from("Press '/' to search, 'l' for library, 'h' for history"),
            is_searching: false,
            is_loading: false,
            animation_tick: 0,
        })
    }

    fn load_data<T: for<'de> Deserialize<'de>>(path: &str) -> Result<T> {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            anyhow::bail!("File not found")
        }
    }

    fn save_data<T: Serialize>(path: &str, data: &T) -> Result<()> {
        let content = serde_json::to_string_pretty(data)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    fn toggle_library(&mut self) {
        let anime = match self.current_screen {
            CurrentScreen::SearchResults => {
                self.search_list_state.selected()
                    .and_then(|i| self.search_results.get(i).cloned())
            }
            CurrentScreen::Library => {
                self.library_list_state.selected()
                    .and_then(|i| self.library.get(i).cloned())
            }
            CurrentScreen::History => {
                self.history_list_state.selected()
                    .and_then(|i| self.history.get(i).map(|h| h.anime.clone()))
            }
            _ => None,
        };

        if let Some(anime) = anime {
            if let Some(pos) = self.library.iter().position(|f| f.session == anime.session) {
                self.library.remove(pos);
                self.status_message = format!("Removed '{}' from library", anime.title);
            } else {
                self.library.push(anime.clone());
                self.status_message = format!("Added '{}' to library", anime.title);
            }
            let _ = Self::save_data("library.json", &self.library);
        }
    }

    fn record_history(&mut self, anime: Anime, ep_session: String, ep_num: String) {
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
        
        if let Some(pos) = self.history.iter().position(|h| h.anime.session == anime.session) {
            self.history.remove(pos);
        }
        
        self.history.insert(0, HistoryItem {
            anime,
            episode_session: ep_session,
            last_episode: ep_num,
            last_watched: now,
        });
        
        // Keep only top 50
        if self.history.len() > 50 {
            self.history.truncate(50);
        }
        
        let _ = Self::save_data("history.json", &self.history);
    }

    async fn perform_search(&mut self) {
        if self.search_query.is_empty() { 
            self.is_searching = false;
            return; 
        }
        self.is_loading = true;
        self.status_message = "Searching...".to_string();
        self.is_searching = false;
        match self.client.search(&self.search_query).await {
            Ok(res) => {
                self.is_loading = false;
                self.search_results = res.data;
                self.current_screen = CurrentScreen::SearchResults;
                self.search_list_state.select(Some(0));
                self.status_message = format!("Found {} results. 'f' to add to library, Enter to view.", self.search_results.len());
            }
            Err(e) => {
                self.is_loading = false;
                self.status_message = format!("Error: {}", e);
            }
        }
    }

    async fn load_episodes(&mut self, page: u32) {
        if let Some(anime) = &self.selected_anime {
            let session = anime.session.clone();
            self.is_loading = true;
            self.status_message = format!("Fetching episodes (Page {})...", page);
            match self.client.get_episodes(&session, page).await {
                Ok(res) => {
                    self.is_loading = false;
                    self.episode_list = res.episodes;
                    self.ep_page = res.page;
                    self.ep_total_pages = res.total_pages;
                    self.current_screen = CurrentScreen::EpisodeList;
                    self.episode_list_state.select(Some(0));
                    self.status_message = format!("Page {}/{}. Left/Right for pages. Enter to play.", self.ep_page, self.ep_total_pages);
                }
                Err(e) => {
                    self.is_loading = false;
                    self.status_message = format!("Error fetching episodes: {}", e);
                }
            }
        }
    }

    async fn play_episode(&mut self) -> Result<()> {
        let ep_data = if let Some(i) = self.episode_list_state.selected() {
            self.episode_list.get(i).map(|ep| (ep.session.clone(), ep.episode.clone()))
        } else {
            None
        };

        if let Some((ep_session, ep_num)) = ep_data {
            if let Some(anime) = self.selected_anime.clone() {
                self.prepare_stream_selection(anime, ep_session, ep_num).await?;
            }
        }
        Ok(())
    }

    async fn prepare_stream_selection(&mut self, anime: Anime, ep_session: String, ep_num: String) -> Result<()> {
        let series_session = anime.session.clone();
        self.selected_anime = Some(anime.clone());
        self.is_loading = true;
        self.status_message = format!("Fetching streams for Ep {}...", ep_num);
        
        match self.client.get_stream(&series_session, &ep_session).await {
            Ok(streams) => {
                self.is_loading = false;
                if streams.is_empty() {
                    self.status_message = "No streams found.".to_string();
                    return Ok(());
                }
                
                self.available_streams = streams;
                self.quality_list_state.select(Some(0));
                self.temp_play_data = Some((anime, ep_session, ep_num));
                self.previous_screen = Some(self.current_screen.clone());
                self.current_screen = CurrentScreen::QualitySelection;
                self.status_message = "Select video quality. Enter to play, Esc to go back.".to_string();
            }
            Err(e) => {
                 self.is_loading = false;
                 self.status_message = format!("Error fetching stream: {}", e);
            }
        }
        Ok(())
    }

    async fn play_selected_stream(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        let stream_idx = self.quality_list_state.selected();
        let play_data = self.temp_play_data.clone();
        
        if let (Some(idx), Some((anime, ep_session, ep_num))) = (stream_idx, play_data) {
            if let Some(link_item) = self.available_streams.get(idx).cloned() {
                let anime_title = anime.title.clone();
                let link = link_item.link.clone();
                let quality_name = link_item.name.clone();
                
                self.is_loading = true;
                self.status_message = format!("Extracting stream URL ({})...", quality_name);
                
                match self.client.extract_stream_url(&link).await {
                    Ok(direct_url) => {
                        self.is_loading = false;
                        self.record_history(anime, ep_session, ep_num.clone());
                        self.launch_mpv(terminal, &direct_url, &anime_title, &ep_num).await?;
                        if let Some(prev) = self.previous_screen.clone() {
                            self.current_screen = prev;
                        }
                    }
                    Err(e) => {
                        self.is_loading = false;
                        self.status_message = format!("Failed to extract stream: {}", e);
                    }
                }
            }
        }
        Ok(())
    }

    async fn launch_mpv(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>, url: &str, title: &str, ep: &str) -> Result<()> {
        execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        disable_raw_mode()?;
        terminal.show_cursor()?;

        let mut mpv_cmd = Command::new("mpv");
        mpv_cmd.arg("--referrer=https://kwik.cx/");
        mpv_cmd.arg(format!("--title=Enuma - {} - Ep {}", title, ep));
        
        match mpv_cmd.arg(url).status() {
            Ok(status) => {
                if status.success() {
                    self.status_message = format!("Finished playing Ep {}.", ep);
                } else {
                    self.status_message = format!("mpv exited with status: {}", status);
                }
            },
            Err(e) => {
                self.status_message = format!("Failed to launch mpv: {}. Is it installed?", e);
            }
        }

        enable_raw_mode()?;
        execute!(terminal.backend_mut(), EnterAlternateScreen, EnableMouseCapture)?;
        terminal.hide_cursor()?;
        terminal.clear()?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let app = App::new()?;
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
    let tick_rate = std::time::Duration::from_millis(100);
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if crossterm::event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if app.is_searching {
                    match key.code {
                        KeyCode::Enter => { app.perform_search().await; }
                        KeyCode::Esc => { app.is_searching = false; }
                        KeyCode::Backspace => { app.search_query.pop(); }
                        KeyCode::Char(c) => { app.search_query.push(c); }
                        _ => {}
                    }
                    continue;
                }

                match app.current_screen {
                    CurrentScreen::Search => match key.code {
                        KeyCode::Char('/') => {
                            app.is_searching = true;
                            app.search_query.clear();
                        }
                        KeyCode::Char('l') => {
                            app.current_screen = CurrentScreen::Library;
                            app.library_list_state.select(Some(0));
                        }
                        KeyCode::Char('h') => {
                            app.current_screen = CurrentScreen::History;
                            app.history_list_state.select(Some(0));
                        }
                        KeyCode::Esc => return Ok(()),
                        _ => {}
                    },
                CurrentScreen::SearchResults => match key.code {
                    KeyCode::Up => {
                        let i = match app.search_list_state.selected() {
                            Some(i) => if i == 0 { app.search_results.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.search_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.search_list_state.selected() {
                            Some(i) => if i >= app.search_results.len().saturating_sub(1) { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.search_list_state.select(Some(i));
                    }
                    KeyCode::Char('f') => { app.toggle_library(); }
                    KeyCode::Char('/') => { 
                        app.is_searching = true; 
                        app.search_query.clear();
                    }
                    KeyCode::Char('l') => {
                        app.current_screen = CurrentScreen::Library;
                        app.library_list_state.select(Some(0));
                    }
                    KeyCode::Char('h') => {
                        app.current_screen = CurrentScreen::History;
                        app.history_list_state.select(Some(0));
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.search_list_state.selected() {
                            if let Some(anime) = app.search_results.get(i).cloned() {
                                app.selected_anime = Some(anime);
                                app.load_episodes(1).await;
                            }
                        }
                    }
                    KeyCode::Esc => {
                        app.current_screen = CurrentScreen::Search;
                    }
                    _ => {}
                },
                CurrentScreen::Library => match key.code {
                    KeyCode::Up => {
                        let i = match app.library_list_state.selected() {
                            Some(i) => if i == 0 { app.library.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.library_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.library_list_state.selected() {
                            Some(i) => if i >= app.library.len().saturating_sub(1) { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.library_list_state.select(Some(i));
                    }
                    KeyCode::Char('f') => { app.toggle_library(); }
                    KeyCode::Char('/') => { 
                        app.is_searching = true;
                        app.search_query.clear();
                    }
                    KeyCode::Char('h') => {
                        app.current_screen = CurrentScreen::History;
                        app.history_list_state.select(Some(0));
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.library_list_state.selected() {
                            if let Some(anime) = app.library.get(i).cloned() {
                                app.selected_anime = Some(anime);
                                app.load_episodes(1).await;
                            }
                        }
                    }
                    KeyCode::Esc => { app.current_screen = CurrentScreen::Search; }
                    _ => {}
                },
                CurrentScreen::History => match key.code {
                    KeyCode::Up => {
                        let i = match app.history_list_state.selected() {
                            Some(i) => if i == 0 { app.history.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.history_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.history_list_state.selected() {
                            Some(i) => if i >= app.history.len().saturating_sub(1) { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.history_list_state.select(Some(i));
                    }
                    KeyCode::Char('f') => { app.toggle_library(); }
                    KeyCode::Char('/') => { 
                        app.is_searching = true;
                        app.search_query.clear();
                    }
                    KeyCode::Char('l') => {
                        app.current_screen = CurrentScreen::Library;
                        app.library_list_state.select(Some(0));
                    }
                    KeyCode::Char('e') => {
                        if let Some(i) = app.history_list_state.selected() {
                            if let Some(item) = app.history.get(i).cloned() {
                                app.selected_anime = Some(item.anime);
                                app.load_episodes(1).await;
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if let Some(i) = app.history_list_state.selected() {
                            if let Some(item) = app.history.get(i).cloned() {
                                app.prepare_stream_selection(item.anime, item.episode_session, item.last_episode).await?;
                            }
                        }
                    }
                    KeyCode::Esc => { app.current_screen = CurrentScreen::Search; }
                    _ => {}
                },
                CurrentScreen::EpisodeList => match key.code {
                    KeyCode::Up => {
                        let i = match app.episode_list_state.selected() {
                            Some(i) => if i == 0 { app.episode_list.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.episode_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.episode_list_state.selected() {
                            Some(i) => if i >= app.episode_list.len().saturating_sub(1) { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.episode_list_state.select(Some(i));
                    }
                    KeyCode::Left => {
                        if app.ep_page > 1 {
                            app.load_episodes(app.ep_page - 1).await;
                        }
                    }
                    KeyCode::Right => {
                        if app.ep_page < app.ep_total_pages {
                            app.load_episodes(app.ep_page + 1).await;
                        }
                    }
                    KeyCode::Char('/') => { 
                        app.is_searching = true;
                        app.search_query.clear();
                    }
                    KeyCode::Enter => {
                        app.play_episode().await?;
                    }
                    KeyCode::Esc => {
                        app.current_screen = match () {
                            _ if !app.search_results.is_empty() => CurrentScreen::SearchResults,
                            _ if !app.library.is_empty() => CurrentScreen::Library,
                            _ => CurrentScreen::Search,
                        };
                    }
                    _ => {}
                }
                CurrentScreen::QualitySelection => match key.code {
                    KeyCode::Up => {
                        let i = match app.quality_list_state.selected() {
                            Some(i) => if i == 0 { app.available_streams.len().saturating_sub(1) } else { i - 1 },
                            None => 0,
                        };
                        app.quality_list_state.select(Some(i));
                    }
                    KeyCode::Down => {
                        let i = match app.quality_list_state.selected() {
                            Some(i) => if i >= app.available_streams.len().saturating_sub(1) { 0 } else { i + 1 },
                            None => 0,
                        };
                        app.quality_list_state.select(Some(i));
                    }
                    KeyCode::Enter => {
                        app.play_selected_stream(terminal).await?;
                    }
                    KeyCode::Esc => {
                        if let Some(prev) = app.previous_screen.clone() {
                            app.current_screen = prev;
                        } else {
                            app.current_screen = CurrentScreen::EpisodeList;
                        }
                    }
                    _ => {}
                }
            }
        }
    } else {
            // No event happen, just tick
            app.animation_tick = app.animation_tick.wrapping_add(1);
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
        )
        .split(f.area());

    // Search Box
    let search_block = Paragraph::new(format!("Search: {}", app.search_query))
        .block(Block::default()
            .borders(Borders::ALL)
            .title(if app.is_searching { " Search [EDITING] " } else { " Enuma Search " })
            .border_style(Style::default().fg(if app.is_searching { Color::Yellow } else if app.current_screen == CurrentScreen::Search { Color::Cyan } else { Color::White })));
    f.render_widget(search_block, chunks[0]);

    // Main Content
    if app.is_loading {
        render_loading_animation(f, chunks[1], app.animation_tick);
    } else {
        match app.current_screen {
            CurrentScreen::Search => {
            let welcome = Paragraph::new("Welcome to Enuma!\n\nPress '/' to start searching.\n\nControls:\n- '/': Focus Search bar\n- Enter (while searching): Perform search\n- Esc (while searching): Cancel search\n\nNavigation:\n- 'l': View Library\n- 'h': View History\n- Esc: Exit app")
                .block(Block::default().borders(Borders::ALL).title(" Help ").border_style(Style::default().fg(Color::Gray)))
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(Color::White));
            f.render_widget(welcome, chunks[1]);
        }
        CurrentScreen::SearchResults => {
            render_anime_list(f, chunks[1], &app.search_results, &mut app.search_list_state, &app.library, " Results ");
        }
        CurrentScreen::Library => {
            if app.library.is_empty() {
                let empty = Paragraph::new("Library is empty. Search and press 'f' to add some!")
                    .block(Block::default().borders(Borders::ALL).title(" Library ").border_style(Style::default().fg(Color::Cyan)))
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(empty, chunks[1]);
            } else {
                render_anime_list(f, chunks[1], &app.library, &mut app.library_list_state, &app.library, " Library ");
            }
        }
        CurrentScreen::History => {
            if app.history.is_empty() {
                let empty = Paragraph::new("No watch history yet.")
                    .block(Block::default().borders(Borders::ALL).title(" History ").border_style(Style::default().fg(Color::Cyan)))
                    .style(Style::default().fg(Color::Yellow));
                f.render_widget(empty, chunks[1]);
            } else {
                render_history_list(f, chunks[1], &app.history, &mut app.history_list_state, &app.library);
            }
        }
        CurrentScreen::EpisodeList => {
             let items: Vec<ListItem> = app.episode_list
                .iter()
                .map(|ep| ListItem::new(format!(" Episode {}", ep.episode)))
                .collect();

            let title = format!(" Episodes - Page {}/{} ", app.ep_page, app.ep_total_pages);
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(Color::Cyan)))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Magenta))
                .highlight_symbol("▶ ");
                
            f.render_stateful_widget(list, chunks[1], &mut app.episode_list_state);
        }
        CurrentScreen::QualitySelection => {
             let items: Vec<ListItem> = app.available_streams
                .iter()
                .map(|s| ListItem::new(format!(" {}", s.name)))
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(" Select Quality ").border_style(Style::default().fg(Color::Cyan)))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
                .highlight_symbol("▶ ");
                
            f.render_stateful_widget(list, chunks[1], &mut app.quality_list_state);
        }
    }
}

fn render_loading_animation(f: &mut Frame, area: Rect, tick: u32) {
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame = frames[(tick as usize) % frames.len()];
    
    let text = format!("\n\n\n  {}  LOADING...  ", frame);
    let loading = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    
    f.render_widget(loading, area);
}
    // Status Bar
    let status = Paragraph::new(format!(" {}", app.status_message))
        .style(Style::default().fg(Color::Black).bg(Color::Cyan));
    f.render_widget(status, chunks[2]);
}

fn render_anime_list(f: &mut Frame, area: Rect, list_data: &[Anime], state: &mut ListState, library: &[Anime], title: &str) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let items: Vec<ListItem> = list_data
        .iter()
        .map(|i| {
            let lib_mark = if library.iter().any(|f| f.session == i.session) { "❤ " } else { "  " };
            let title = if i.title.len() > 40 { format!("{}...", &i.title[..37]) } else { i.title.clone() };
            ListItem::new(format!("{}{}", lib_mark, title))
        })
        .collect();
    
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title).border_style(Style::default().fg(Color::Cyan)))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
        .highlight_symbol("▶ ");
    
    f.render_stateful_widget(list, layout[0], state);

    // Details Panel
    if let Some(i) = state.selected() {
        if let Some(anime) = list_data.get(i) {
            render_details(f, layout[1], anime, library);
        }
    }
}

fn render_history_list(f: &mut Frame, area: Rect, list_data: &[HistoryItem], state: &mut ListState, library: &[Anime]) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    let items: Vec<ListItem> = list_data
        .iter()
        .map(|h| {
            let lib_mark = if library.iter().any(|f| f.session == h.anime.session) { "❤ " } else { "  " };
            let title = if h.anime.title.len() > 30 { format!("{}...", &h.anime.title[..27]) } else { h.anime.title.clone() };
            ListItem::new(format!("{}{:<35} Ep {:<3} [{}]", lib_mark, title, h.last_episode, h.last_watched))
        })
        .collect();
    
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" History ").border_style(Style::default().fg(Color::Cyan)))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))
        .highlight_symbol("▶ ");
    
    f.render_stateful_widget(list, layout[0], state);

    if let Some(i) = state.selected() {
        if let Some(item) = list_data.get(i) {
            render_details(f, layout[1], &item.anime, library);
        }
    }
}

fn render_details(f: &mut Frame, area: Rect, anime: &Anime, library: &[Anime]) {
    let is_lib = library.iter().any(|f| f.session == anime.session);
    let details = format!(
        "Title: {}\n\nType: {}\nStatus: {}\nEpisodes: {}\nScore: {}\nYear: {}\n\n{}",
        anime.title,
        anime.anime_type.as_deref().unwrap_or("Unknown"),
        anime.status,
        anime.episodes.map(|e| e.to_string()).unwrap_or_else(|| "Unknown".to_string()),
        anime.score.map(|s| s.to_string()).unwrap_or_else(|| "N/A".to_string()),
        anime.year.map(|y| y.to_string()).unwrap_or_else(|| "Unknown".to_string()),
        if is_lib { "[ In Library ❤ ]" } else { "[ Press 'f' to add to library ]" }
    );
    let details_p = Paragraph::new(details)
        .block(Block::default().borders(Borders::ALL).title(" Details ").border_style(Style::default().fg(Color::Gray)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));
    f.render_widget(details_p, area);
}
