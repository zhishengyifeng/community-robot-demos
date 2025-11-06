// ============================================================================
// Robot UI Module - Handles terminal UI rendering with ratatui
// ============================================================================

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::KeyCode,
};
use hyper::HeaderMap;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use std::collections::HashMap;
use std::io;

use super::keyboard_input::SpeedData;
use super::keyboard_input::KeyState;

/// Control state of the robot
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum ControlState {
    Uninitialized,
    InitializedButNotHold,
    CanMove,
}

/// Error message with timestamp
#[derive(Clone, Debug, Default)]
pub struct ErrorMessage {
    pub message: String,
    pub timestamp: Option<std::time::Instant>,
}

impl ErrorMessage {
    pub fn new(message: String) -> Self {
        Self {
            message,
            timestamp: Some(std::time::Instant::now()),
        }
    }

    pub fn is_expired(&self, duration: std::time::Duration) -> bool {
        if let Some(ts) = self.timestamp {
            ts.elapsed() > duration
        } else {
            false
        }
    }
}

/// Main UI Manager for robot control interface
pub struct RobotUi {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl RobotUi {
    /// Initialize the UI terminal
    pub fn new() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal })
    }

    /// Draw the complete UI with simplified parameters
    pub fn draw(
        &mut self,
        control_state: ControlState,
        target_speed: &SpeedData,
        actual_speed: Option<SpeedData>,
        pressed_keys: &HashMap<KeyCode, KeyState>,
        error_message: &ErrorMessage,
        emergency_stop: bool,
    ) -> io::Result<()> {
        self.terminal.draw(|f| {
            let size = f.area();

            // Create main layout
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),  // Title
                    Constraint::Length(7),  // Control hints
                    Constraint::Length(12), // Speed displays
                    Constraint::Length(3),  // Status
                ])
                .split(size);

            // Render each section
            f.render_widget(Self::render_title(), chunks[0]);
            f.render_widget(Self::render_controls(pressed_keys), chunks[1]);

            // Speed displays
            let speed_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50), // Target speed
                    Constraint::Percentage(50), // Actual speed
                ])
                .split(chunks[2]);

            f.render_widget(Self::render_target_speed(target_speed), speed_chunks[0]);
            f.render_widget(Self::render_actual_speed(&actual_speed), speed_chunks[1]);
            f.render_widget(
                Self::render_status(control_state, error_message, emergency_stop),
                chunks[3],
            );
        })?;

        Ok(())
    }

    /// Render the title bar
    fn render_title() -> Paragraph<'static> {
        Paragraph::new("Robot Base Advanced Control")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL))
    }

    /// Render keyboard controls with highlighting
    fn render_controls(pressed_keys: &HashMap<KeyCode, KeyState>) -> Paragraph<'static> {
        let key_style = |key: KeyCode| {
            if pressed_keys.contains_key(&key) {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            }
        };

        let controls = vec![
            Line::from(vec![
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("W", key_style(crossterm::event::KeyCode::Char('w'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Forward  ", Style::default().fg(Color::White)),
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("S", key_style(crossterm::event::KeyCode::Char('s'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Backward", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("A", key_style(crossterm::event::KeyCode::Char('a'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Left     ", Style::default().fg(Color::White)),
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("D", key_style(crossterm::event::KeyCode::Char('d'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Right", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("Q", key_style(crossterm::event::KeyCode::Char('q'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Rotate Left  ", Style::default().fg(Color::White)),
                Span::styled("[", Style::default().fg(Color::White)),
                Span::styled("E", key_style(crossterm::event::KeyCode::Char('e'))),
                Span::styled("]", Style::default().fg(Color::White)),
                Span::styled(" Rotate Right", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("[ESC/C]", Style::default().fg(Color::Red)),
                Span::styled(" Exit", Style::default().fg(Color::White)),
            ]),
        ];

        Paragraph::new(controls)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Keyboard Controls"),
            )
            .alignment(Alignment::Left)
    }

    /// Render target speed display
    fn render_target_speed(speed: &SpeedData) -> Paragraph<'static> {
        let lines = vec![
            Line::from(vec![
                Span::styled("X: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:+.3} m/s", speed.x),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Y: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:+.3} m/s", speed.y),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(vec![
                Span::styled("Z: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    format!("{:+.3} rad/s", speed.z),
                    Style::default().fg(Color::White),
                ),
            ]),
        ];

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Target Speed"),
            )
            .alignment(Alignment::Left)
    }

    /// Render actual speed display
    fn render_actual_speed(speed: &Option<SpeedData>) -> Paragraph<'static> {
        let lines = if let Some(s) = speed {
            vec![
                Line::from(vec![
                    Span::styled("X: ", Style::default().fg(Color::Gray)),
                    Span::styled(format!("{:+.3} m/s", s.x), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Y: ", Style::default().fg(Color::Gray)),
                    Span::styled(format!("{:+.3} m/s", s.y), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("Z: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("{:+.3} rad/s", s.z),
                        Style::default().fg(Color::White),
                    ),
                ]),
            ]
        } else {
            vec![Line::from(vec![Span::styled(
                "Waiting for data...",
                Style::default().fg(Color::Gray),
            )])]
        };

        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Actual Speed"),
            )
            .alignment(Alignment::Left)
    }

    /// Render status bar with state-based styling
    fn render_status(
        control_state: ControlState,
        error_message: &ErrorMessage,
        emergency_stop: bool,
    ) -> Paragraph<'static> {
        let has_error = !error_message.message.is_empty();

        let (status_text, status_style, border_style) = 
            if emergency_stop {
                (
                     format!("EMERGENCY STOP: {}", error_message.message),
                     Style::default()
                         .fg(Color::Red)
                         .add_modifier(Modifier::BOLD),
                     Style::default()
                         .fg(Color::Red)
                         .add_modifier(Modifier::BOLD),
                )
            }else { 
                match control_state {
                    ControlState::Uninitialized => (
                        "Status: Initializing...".to_string(),
                        Style::default().fg(Color::Cyan),
                        Style::default(),
                    ),
                    ControlState::InitializedButNotHold => {
                        if has_error {
                            (
                                format!("Warn: {}", error_message.message),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )
                        } else {
                            (
                                "Status: NO CONTROL".to_string(),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                                Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD),
                            )
                        }
                    }
                    ControlState::CanMove => { 
                            if has_error {
                                (
                                     format!("warn: {}", error_message.message),
                                    Style::default()
                                        .fg(Color::Yellow)
                                        .add_modifier(Modifier::BOLD),
                                    Style::default()
                                        .fg(Color::Yellow)
                                        .add_modifier(Modifier::BOLD),
                                )
                            } else {
                                (               
                                    "Status: Ready to Move".to_string(),
                                    Style::default()
                                        .fg(Color::Green)
                                        .add_modifier(Modifier::BOLD),
                                    Style::default(),
                                )
                            }
                        }
                }};

        let status_block = if has_error
            || emergency_stop
            || control_state == ControlState::InitializedButNotHold
        {
            Block::default()
                .borders(Borders::ALL)
                .title("Robot Status")
                .border_style(border_style)
        } else {
            Block::default()
                .borders(Borders::ALL)
                .title("Robot Status")
        };

        Paragraph::new(status_text)
            .style(status_style)
            .alignment(Alignment::Center)
            .block(status_block)
    }

    /// Cleanup terminal on exit
    pub fn cleanup(&mut self) -> io::Result<()> {
        disable_raw_mode()?;
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }
}

impl Drop for RobotUi {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
