use std::{cell::RefCell, collections::BTreeMap, io::Result, rc::Rc};

use chrono::{DateTime, Utc};
use gloo_net::http::Request;
use web_sys::WebSocket;
use gloo_timers::callback::Interval;
use ratzilla::event::KeyCode;
use ratzilla::ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Terminal,
};
use ratzilla::{DomBackend, WebRenderer};
use serde::{Deserialize, Serialize};
use wasm_bindgen_futures::spawn_local;
use web_sys::window;
use wasm_bindgen::prelude::*;

// ─── Data Models ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntityData {
    id: String,
    properties: serde_json::Map<String, serde_json::Value>,
    #[serde(alias = "lastUpdated", alias = "last_updated")]
    last_updated: String,
}

#[derive(Debug, Clone, Default)]
struct Entity {
    id: String,
    properties: BTreeMap<String, serde_json::Value>,
    last_updated: String,
}

#[derive(Debug, Clone, Default)]
struct Metrics {
    total_entities: u64,
    total_events: u64,
    events_per_second: f64,
    ws_connections: u64,
    active_publishers: u64,
}

#[derive(Debug, Clone)]
struct AgentMessage {
    from: String,
    to: String,
    message: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    entity_id: String,
    #[serde(default)]
    property: String,
    #[serde(default)]
    value: serde_json::Value,
    #[serde(default)]
    timestamp: String,
    // metrics fields
    #[serde(default)]
    entities: Option<MetricsEntities>,
    #[serde(default)]
    events: Option<MetricsEvents>,
    #[serde(default)]
    websocket: Option<MetricsWs>,
    #[serde(default)]
    publishers: Option<MetricsPublishers>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetricsEntities {
    #[serde(default)]
    total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetricsEvents {
    #[serde(default)]
    total: u64,
    #[serde(default)]
    rate_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetricsWs {
    #[serde(default)]
    connections: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MetricsPublishers {
    #[serde(default)]
    active: u64,
}

// ─── App State ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Panel {
    Entities,
    Detail,
    Messages,
}

struct AppState {
    entities: BTreeMap<String, Entity>,
    metrics: Metrics,
    messages: Vec<AgentMessage>,
    selected_entity: usize,
    table_state: TableState,
    active_panel: Panel,
    ws_connected: bool,
    event_log: Vec<String>, // recent events for the stream
    now_ms: f64,            // current time for staleness calc
}

impl AppState {
    fn new() -> Self {
        Self {
            entities: BTreeMap::new(),
            metrics: Metrics::default(),
            messages: Vec::new(),
            selected_entity: 0,
            table_state: TableState::default().with_selected(Some(0)),
            active_panel: Panel::Entities,
            ws_connected: false,
            event_log: Vec::new(),
            now_ms: js_sys::Date::now(),
        }
    }

    fn sorted_entity_ids(&self) -> Vec<String> {
        let mut ids: Vec<_> = self.entities.keys().cloned().collect();
        // Sort by last_updated descending (most recent first)
        ids.sort_by(|a, b| {
            let ta = self.entities.get(a).map(|e| e.last_updated.as_str()).unwrap_or("");
            let tb = self.entities.get(b).map(|e| e.last_updated.as_str()).unwrap_or("");
            tb.cmp(ta)
        });
        ids
    }

    fn selected_entity_data(&self) -> Option<&Entity> {
        let ids = self.sorted_entity_ids();
        ids.get(self.selected_entity)
            .and_then(|id| self.entities.get(id))
    }

    fn apply_state_update(&mut self, entity_id: &str, property: &str, value: serde_json::Value, timestamp: &str) {
        let entity = self.entities.entry(entity_id.to_string()).or_insert_with(|| Entity {
            id: entity_id.to_string(),
            properties: BTreeMap::new(),
            last_updated: String::new(),
        });
        entity.properties.insert(property.to_string(), value.clone());
        entity.last_updated = timestamp.to_string();

        // Check for agent messages
        let has_message = entity.properties.contains_key("message");
        let has_message_to = entity.properties.contains_key("message_to");
        if has_message && has_message_to {
            let msg_text = entity.properties.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let msg_to = entity.properties.get("message_to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !msg_text.is_empty() {
                // Avoid duplicate if last message is the same
                let dominated = self.messages.last()
                    .map(|m| m.from == entity_id && m.message == msg_text)
                    .unwrap_or(false);
                if !dominated {
                    self.messages.push(AgentMessage {
                        from: entity_id.to_string(),
                        to: msg_to,
                        message: msg_text,
                        timestamp: timestamp.to_string(),
                    });
                    // Keep last 100 messages
                    if self.messages.len() > 100 {
                        self.messages.remove(0);
                    }
                }
            }
        }

        // Log event
        let short_val = format!("{}", value);
        let short_val = if short_val.len() > 40 { format!("{}…", &short_val[..40]) } else { short_val };
        self.event_log.push(format!("{}.{} = {}", entity_id, property, short_val));
        if self.event_log.len() > 200 {
            self.event_log.remove(0);
        }
    }

    fn apply_metrics(&mut self, msg: &WsMessage) {
        if let Some(ref e) = msg.entities {
            self.metrics.total_entities = e.total;
        }
        if let Some(ref ev) = msg.events {
            self.metrics.total_events = ev.total;
            self.metrics.events_per_second = ev.rate_per_second;
        }
        if let Some(ref ws) = msg.websocket {
            self.metrics.ws_connections = ws.connections;
        }
        if let Some(ref p) = msg.publishers {
            self.metrics.active_publishers = p.active;
        }
    }

    fn delete_entity(&mut self, entity_id: &str) {
        self.entities.remove(entity_id);
        // Clamp selection
        let count = self.entities.len();
        if self.selected_entity >= count && count > 0 {
            self.selected_entity = count - 1;
        }
        self.table_state.select(Some(self.selected_entity));
    }
}

// ─── Staleness helpers ──────────────────────────────────────────────────────

fn staleness_color(last_updated: &str, now_ms: f64) -> Color {
    let age_secs = parse_age_secs(last_updated, now_ms);
    if age_secs < 60.0 {
        Color::Green
    } else if age_secs < 300.0 {
        Color::Yellow
    } else {
        Color::Red
    }
}

fn staleness_label(last_updated: &str, now_ms: f64) -> String {
    let age = parse_age_secs(last_updated, now_ms);
    if age < 5.0 {
        "now".to_string()
    } else if age < 60.0 {
        format!("{:.0}s", age)
    } else if age < 3600.0 {
        format!("{:.0}m", age / 60.0)
    } else if age < 86400.0 {
        format!("{:.0}h", age / 3600.0)
    } else {
        format!("{:.0}d", age / 86400.0)
    }
}

fn parse_age_secs(last_updated: &str, now_ms: f64) -> f64 {
    // Parse ISO timestamp and compare to now
    if let Ok(dt) = last_updated.parse::<DateTime<Utc>>() {
        let ts_ms = dt.timestamp_millis() as f64;
        (now_ms - ts_ms) / 1000.0
    } else {
        9999.0 // unknown = very stale
    }
}

// ─── API helpers ────────────────────────────────────────────────────────────

fn get_base_url() -> String {
    let win = window().expect("no window");
    let loc = win.location();
    let proto = loc.protocol().unwrap_or_else(|_| "http:".to_string());
    let host = loc.host().unwrap_or_else(|_| "localhost:3000".to_string());
    format!("{}//{}", proto, host)
}

fn get_ws_url() -> String {
    let win = window().expect("no window");
    let loc = win.location();
    let proto = loc.protocol().unwrap_or_else(|_| "http:".to_string());
    let ws_proto = if proto == "https:" { "wss:" } else { "ws:" };
    let host = loc.host().unwrap_or_else(|_| "localhost:3000".to_string());
    format!("{}//{}/api/ws", ws_proto, host)
}

// ─── UI Rendering ───────────────────────────────────────────────────────────

fn render_header(f: &mut ratzilla::ratatui::Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(30)])
        .split(area);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("⟁ ", Style::default().fg(Color::Magenta)),
        Span::styled("Flux", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
        Span::styled(" Monitor", Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(Color::DarkGray));

    let status_text = if state.ws_connected {
        Span::styled("● LIVE", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        Span::styled("● DISCONNECTED", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
    };
    let status = Paragraph::new(Line::from(vec![status_text]))
        .alignment(ratzilla::ratatui::layout::Alignment::Right)
        .block(Block::default().borders(Borders::BOTTOM).border_style(Color::DarkGray));

    f.render_widget(title, chunks[0]);
    f.render_widget(status, chunks[1]);
}

fn render_entity_list(f: &mut ratzilla::ratatui::Frame, area: Rect, state: &mut AppState) {
    let ids = state.sorted_entity_ids();
    let now_ms = state.now_ms;

    let rows: Vec<Row> = ids
        .iter()
        .map(|id| {
            let entity = &state.entities[id];
            let status = entity
                .properties
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("-");
            let color = staleness_color(&entity.last_updated, now_ms);
            let age = staleness_label(&entity.last_updated, now_ms);

            let status_style = match status {
                "active" | "online" | "healthy" => Style::default().fg(Color::Green),
                "warning" => Style::default().fg(Color::Yellow),
                "error" | "critical" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::DarkGray),
            };

            Row::new(vec![
                Cell::from(Span::styled("●", Style::default().fg(color))),
                Cell::from(Span::styled(id.clone(), Style::default().fg(Color::Cyan))),
                Cell::from(Span::styled(status.to_string(), status_style)),
                Cell::from(Span::styled(age, Style::default().fg(color))),
            ])
        })
        .collect();

    let header = Row::new(vec![
        Cell::from(Span::styled("", Style::default())),
        Cell::from(Span::styled("Entity", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Status", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
        Cell::from(Span::styled("Age", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))),
    ])
    .style(Style::default().bg(Color::DarkGray));

    let border_color = if state.active_panel == Panel::Entities {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Min(20),
            Constraint::Length(10),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .title(format!(" Entities ({}) ", ids.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    f.render_stateful_widget(table, area, &mut state.table_state);
}

fn render_detail(f: &mut ratzilla::ratatui::Frame, area: Rect, state: &AppState) {
    let border_color = if state.active_panel == Panel::Detail {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    if let Some(entity) = state.selected_entity_data() {
        let mut lines = vec![
            Line::from(vec![
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&entity.id, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Updated: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    staleness_label(&entity.last_updated, state.now_ms),
                    Style::default().fg(staleness_color(&entity.last_updated, state.now_ms)),
                ),
                Span::styled(
                    format!(" ({})", &entity.last_updated),
                    Style::default().fg(Color::DarkGray),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "─── Properties ───",
                Style::default().fg(Color::Magenta),
            )),
        ];

        for (key, value) in &entity.properties {
            let val_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => if *b { "✓".to_string() } else { "✗".to_string() },
                serde_json::Value::Null => "null".to_string(),
                other => format!("{}", other),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {}: ", key), Style::default().fg(Color::Yellow)),
                Span::styled(val_str, Style::default().fg(Color::White)),
            ]));
        }

        let detail = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Detail ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color)),
            )
            .wrap(Wrap { trim: false });
        f.render_widget(detail, area);
    } else {
        let empty = Paragraph::new(Span::styled(
            "No entity selected",
            Style::default().fg(Color::DarkGray),
        ))
        .block(
            Block::default()
                .title(" Detail ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color)),
        );
        f.render_widget(empty, area);
    }
}

fn render_messages(f: &mut ratzilla::ratatui::Frame, area: Rect, state: &AppState) {
    let border_color = if state.active_panel == Panel::Messages {
        Color::Magenta
    } else {
        Color::DarkGray
    };

    let lines: Vec<Line> = state
        .messages
        .iter()
        .rev()
        .take(area.height as usize - 2) // fit in visible area
        .map(|msg| {
            Line::from(vec![
                Span::styled(&msg.from, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(" → ", Style::default().fg(Color::DarkGray)),
                Span::styled(&msg.to, Style::default().fg(Color::Yellow)),
                Span::styled(": ", Style::default().fg(Color::DarkGray)),
                Span::styled(&msg.message, Style::default().fg(Color::White)),
            ])
        })
        .collect();

    let messages = Paragraph::new(if lines.is_empty() {
        Text::from(Span::styled("No agent messages yet", Style::default().fg(Color::DarkGray)))
    } else {
        Text::from(lines)
    })
    .block(
        Block::default()
            .title(format!(" Agent Messages ({}) ", state.messages.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color)),
    )
    .wrap(Wrap { trim: false });

    f.render_widget(messages, area);
}

fn render_metrics(f: &mut ratzilla::ratatui::Frame, area: Rect, state: &AppState) {
    let m = &state.metrics;
    let line = Line::from(vec![
        Span::styled(" ⚡ ", Style::default().fg(Color::Yellow)),
        Span::styled(format!("{:.1}", m.events_per_second), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::styled(" evt/s", Style::default().fg(Color::DarkGray)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("◈ ", Style::default().fg(Color::Magenta)),
        Span::styled(format!("{}", m.total_entities), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(" entities", Style::default().fg(Color::DarkGray)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("⇅ ", Style::default().fg(Color::Blue)),
        Span::styled(format!("{}", m.active_publishers), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(" publishers", Style::default().fg(Color::DarkGray)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("∑ ", Style::default().fg(Color::White)),
        Span::styled(format!("{}", m.total_events), Style::default().fg(Color::White)),
        Span::styled(" total", Style::default().fg(Color::DarkGray)),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled("⊙ ", Style::default().fg(Color::Green)),
        Span::styled(format!("{}", m.ws_connections), Style::default().fg(Color::Green)),
        Span::styled(" ws", Style::default().fg(Color::DarkGray)),
    ]);

    let metrics = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(metrics, area);
}

fn render_help(f: &mut ratzilla::ratatui::Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" ↑↓", Style::default().fg(Color::Yellow)),
        Span::styled(" navigate  ", Style::default().fg(Color::DarkGray)),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::styled(" switch panel  ", Style::default().fg(Color::DarkGray)),
    ]));
    f.render_widget(help, area);
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let state = Rc::new(RefCell::new(AppState::new()));

    let backend = DomBackend::new()?;
    let terminal = Terminal::new(backend)?;

    // ── Load initial state via HTTP ─────────────────────────────────────
    {
        let state_clone = state.clone();
        spawn_local(async move {
            let base = get_base_url();
            let url = format!("{}/api/state/entities", base);
            match Request::get(&url).send().await {
                Ok(resp) => {
                    if let Ok(entities) = resp.json::<Vec<EntityData>>().await {
                        let mut s = state_clone.borrow_mut();
                        for e in entities {
                            let mut props = BTreeMap::new();
                            for (k, v) in e.properties {
                                props.insert(k, v);
                            }
                            s.entities.insert(e.id.clone(), Entity {
                                id: e.id,
                                properties: props,
                                last_updated: e.last_updated,
                            });
                        }
                    }
                }
                Err(e) => {
                    web_sys::console::log_1(&format!("Failed to load initial state: {:?}", e).into());
                }
            }
        });
    }

    // ── Connect WebSocket ───────────────────────────────────────────────
    {
        let state_clone = state.clone();
        connect_websocket(state_clone);
    }

    // ── Keep now_ms updated (every 1s) ──────────────────────────────────
    {
        let state_clone = state.clone();
        let _interval = Interval::new(1_000, move || {
            state_clone.borrow_mut().now_ms = js_sys::Date::now();
        });
        // Leak the interval so it lives forever
        std::mem::forget(_interval);
    }

    // ── Key events ──────────────────────────────────────────────────────
    terminal.on_key_event({
        let state_clone = state.clone();
        move |key_event| {
            let mut s = state_clone.borrow_mut();
            let entity_count = s.entities.len();
            match key_event.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    if s.selected_entity > 0 {
                        s.selected_entity -= 1;
                        let selected = s.selected_entity;
                        s.table_state.select(Some(selected));
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if entity_count > 0 && s.selected_entity < entity_count - 1 {
                        s.selected_entity += 1;
                        let selected = s.selected_entity;
                        s.table_state.select(Some(selected));
                    }
                }
                KeyCode::Tab => {
                    s.active_panel = match s.active_panel {
                        Panel::Entities => Panel::Detail,
                        Panel::Detail => Panel::Messages,
                        Panel::Messages => Panel::Entities,
                    };
                }
                _ => {}
            }
        }
    });

    // ── Draw loop (runs on rAF) ─────────────────────────────────────────
    terminal.draw_web({
        let state_clone = state.clone();
        move |f| {
            let s = &mut *state_clone.borrow_mut();

            let outer = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2),  // header
                    Constraint::Min(10),   // main content
                    Constraint::Length(2),  // metrics bar
                    Constraint::Length(1),  // help
                ])
                .split(f.area());

            render_header(f, outer[0], s);

            // Main content: left (entity list) | right (detail + messages)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(outer[1]);

            render_entity_list(f, main_chunks[0], s);

            // Right side: detail on top, messages on bottom
            let right_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
                .split(main_chunks[1]);

            render_detail(f, right_chunks[0], s);
            render_messages(f, right_chunks[1], s);

            render_metrics(f, outer[2], s);
            render_help(f, outer[3]);
        }
    });

    Ok(())
}

// ─── WebSocket Connection ───────────────────────────────────────────────────

fn connect_websocket(state: Rc<RefCell<AppState>>) {
    let ws_url = get_ws_url();
    web_sys::console::log_1(&format!("Connecting to {}", ws_url).into());

    match WebSocket::new(&ws_url) {
        Ok(ws) => {
            // Set up open handler to send subscribe message
            let ws_clone = ws.clone();
            let state_clone = state.clone();
            let onopen = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::Event| {
                // Send subscribe message
                let sub_msg = serde_json::json!({"type": "subscribe", "entity_id": "*"});
                if let Err(e) = ws_clone.send_with_str(&sub_msg.to_string()) {
                    web_sys::console::log_1(&format!("WS send error: {:?}", e).into());
                } else {
                    state_clone.borrow_mut().ws_connected = true;
                }
            }) as Box<dyn FnMut(_)>);
            
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

            // Set up message handler
            let state_clone = state.clone();
            let onmessage = wasm_bindgen::closure::Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
                if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
                    let text_string = String::from(text);
                    if let Ok(ws_msg) = serde_json::from_str::<WsMessage>(&text_string) {
                        let mut s = state_clone.borrow_mut();
                        match ws_msg.msg_type.as_str() {
                            "state_update" => {
                                s.apply_state_update(
                                    &ws_msg.entity_id,
                                    &ws_msg.property,
                                    ws_msg.value.clone(),
                                    &ws_msg.timestamp,
                                );
                            }
                            "metrics_update" => {
                                s.apply_metrics(&ws_msg);
                            }
                            "entity_deleted" => {
                                s.delete_entity(&ws_msg.entity_id);
                            }
                            _ => {}
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);
            
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            
            // Set up error handler
            let state_clone2 = state.clone();
            let onerror = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::Event| {
                web_sys::console::log_1(&"WebSocket error, reconnecting...".into());
                state_clone2.borrow_mut().ws_connected = false;
                // Reconnect after delay
                let state_clone3 = state_clone2.clone();
                let _timeout = gloo_timers::callback::Timeout::new(3_000, move || {
                    connect_websocket(state_clone3);
                });
                std::mem::forget(_timeout);
            }) as Box<dyn FnMut(_)>);
            
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            // Set up close handler  
            let state_clone3 = state.clone();
            let onclose = wasm_bindgen::closure::Closure::wrap(Box::new(move |_e: web_sys::CloseEvent| {
                web_sys::console::log_1(&"WebSocket closed, reconnecting...".into());
                state_clone3.borrow_mut().ws_connected = false;
                // Reconnect after delay
                let state_clone4 = state_clone3.clone();
                let _timeout = gloo_timers::callback::Timeout::new(3_000, move || {
                    connect_websocket(state_clone4);
                });
                std::mem::forget(_timeout);
            }) as Box<dyn FnMut(_)>);
            
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

            // Keep closures alive
            onopen.forget();
            onmessage.forget();
            onerror.forget();
            onclose.forget();
        }
        Err(e) => {
            web_sys::console::log_1(&format!("WS connect failed: {:?}", e).into());
            // Retry after delay
            let state_clone = state.clone();
            let _timeout = gloo_timers::callback::Timeout::new(3_000, move || {
                connect_websocket(state_clone);
            });
            std::mem::forget(_timeout);
        }
    }
}
